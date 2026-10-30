use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, AnyPool, Row};

use crate::error::{AppError, AppResult};
use crate::inventory::apply_movement;
use crate::util::{new_id, next_counter, now};

// ---------- Proveedores ----------

#[derive(Debug, Serialize)]
pub struct Supplier {
    pub id: String,
    pub name: String,
    pub tax_id: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct SupplierInput {
    pub id: Option<String>,
    pub name: String,
    pub tax_id: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub active: bool,
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub async fn list_suppliers(pool: &AnyPool) -> AppResult<Vec<Supplier>> {
    let rows = sqlx::query("SELECT id, name, tax_id, phone, notes, active FROM suppliers ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| Supplier {
            id: r.get("id"),
            name: r.get("name"),
            tax_id: r.get("tax_id"),
            phone: r.get("phone"),
            notes: r.get("notes"),
            active: r.get::<i64, _>("active") != 0,
        })
        .collect())
}

pub async fn save_supplier(pool: &AnyPool, input: &SupplierInput) -> AppResult<String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("el nombre del proveedor es obligatorio".into()));
    }
    match &input.id {
        Some(id) => {
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM suppliers WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound("proveedor".into()));
            }
            sqlx::query("UPDATE suppliers SET name = ?, tax_id = ?, phone = ?, notes = ?, active = ? WHERE id = ?")
                .bind(name)
                .bind(clean(&input.tax_id))
                .bind(clean(&input.phone))
                .bind(clean(&input.notes))
                .bind(input.active as i64)
                .bind(id)
                .execute(pool)
                .await?;
            Ok(id.clone())
        }
        None => {
            let id = new_id();
            sqlx::query("INSERT INTO suppliers (id, name, tax_id, phone, notes, active) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(name)
                .bind(clean(&input.tax_id))
                .bind(clean(&input.phone))
                .bind(clean(&input.notes))
                .bind(input.active as i64)
                .execute(pool)
                .await?;
            Ok(id)
        }
    }
}

/// Solo se elimina un proveedor sin compras; si no, se desactiva.
pub async fn delete_supplier(pool: &AnyPool, id: &str) -> AppResult<()> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM purchases WHERE supplier_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if n > 0 {
        return Err(AppError::Validation(format!(
            "el proveedor tiene {n} compra(s); desactívalo en lugar de eliminarlo"
        )));
    }
    sqlx::query("DELETE FROM suppliers WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

// ---------- Compras ----------

pub const DRAFT: &str = "DRAFT";
pub const CONFIRMED: &str = "CONFIRMED";
pub const CANCELED: &str = "CANCELED";

#[derive(Debug, Deserialize)]
pub struct PurchaseItemInput {
    pub product_id: String,
    pub presentation_id: String,
    pub qty_presentations: i64,
    pub unit_cost_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseInput {
    pub id: Option<String>,
    pub supplier_id: Option<String>,
    pub purchased_at: Option<String>,
    pub items: Vec<PurchaseItemInput>,
}

#[derive(Debug, Serialize)]
pub struct PurchaseSummary {
    pub id: String,
    pub number: String,
    pub supplier_id: Option<String>,
    pub supplier_name: Option<String>,
    pub status: String,
    pub purchased_at: String,
    pub total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct PurchaseItemView {
    pub id: String,
    pub product_id: String,
    pub product_name: String,
    pub presentation_id: String,
    pub presentation_name: String,
    pub qty_presentations: i64,
    pub factor_snapshot: i64,
    pub unit_cost_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct PurchaseDetail {
    #[serde(flatten)]
    pub summary: PurchaseSummary,
    pub items: Vec<PurchaseItemView>,
}

/// Costo por unidad base redondeado al céntimo más cercano (half-up), en enteros.
pub fn cost_per_base_unit(unit_cost_cents: i64, factor: i64) -> i64 {
    (unit_cost_cents + factor / 2) / factor
}

async fn status_of(conn: &mut AnyConnection, id: &str) -> AppResult<String> {
    let st: Option<String> = sqlx::query_scalar("SELECT status FROM purchases WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    st.ok_or_else(|| AppError::NotFound("compra".into()))
}

/// Crea o edita un borrador. Una compra confirmada no se puede editar (RF-03).
pub async fn save_purchase(pool: &AnyPool, input: &PurchaseInput) -> AppResult<String> {
    if input.items.is_empty() {
        return Err(AppError::Validation("agrega al menos un producto a la compra".into()));
    }
    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;

    if let Some(sid) = &input.supplier_id {
        let ok: Option<String> = sqlx::query_scalar("SELECT id FROM suppliers WHERE id = ?")
            .bind(sid)
            .fetch_optional(&mut *conn)
            .await?;
        if ok.is_none() {
            return Err(AppError::NotFound("proveedor".into()));
        }
    }

    // Resolver líneas con factor congelado
    let mut lines = Vec::new();
    let mut total: i64 = 0;
    for it in &input.items {
        if it.qty_presentations <= 0 {
            return Err(AppError::Validation("la cantidad debe ser mayor a cero".into()));
        }
        if it.unit_cost_cents < 0 {
            return Err(AppError::Validation("el costo no puede ser negativo".into()));
        }
        let factor: Option<i64> = sqlx::query_scalar(
            "SELECT factor_to_base FROM product_presentations WHERE id = ? AND product_id = ? AND active = 1",
        )
        .bind(&it.presentation_id)
        .bind(&it.product_id)
        .fetch_optional(&mut *conn)
        .await?;
        let factor = factor.ok_or_else(|| AppError::Validation("presentación inválida o inactiva".into()))?;
        let line_total = it
            .qty_presentations
            .checked_mul(it.unit_cost_cents)
            .ok_or_else(|| AppError::Validation("monto fuera de rango".into()))?;
        total = total
            .checked_add(line_total)
            .ok_or_else(|| AppError::Validation("monto fuera de rango".into()))?;
        lines.push((it, factor, line_total));
    }

    let purchased_at = input.purchased_at.clone().unwrap_or_else(now);
    let id = match &input.id {
        Some(id) => {
            if status_of(conn, id).await? != DRAFT {
                return Err(AppError::Validation("solo se pueden editar compras en borrador".into()));
            }
            sqlx::query("UPDATE purchases SET supplier_id = ?, purchased_at = ?, total_cents = ? WHERE id = ?")
                .bind(&input.supplier_id)
                .bind(&purchased_at)
                .bind(total)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            sqlx::query("DELETE FROM purchase_items WHERE purchase_id = ?").bind(id).execute(&mut *conn).await?;
            id.clone()
        }
        None => {
            let id = new_id();
            let n = next_counter(conn, "purchase").await?;
            sqlx::query(
                "INSERT INTO purchases (id, number, supplier_id, status, purchased_at, total_cents, created_at) \
                 VALUES (?, ?, ?, 'DRAFT', ?, ?, ?)",
            )
            .bind(&id)
            .bind(format!("C-{n:06}"))
            .bind(&input.supplier_id)
            .bind(&purchased_at)
            .bind(total)
            .bind(now())
            .execute(&mut *conn)
            .await?;
            id
        }
    };
    for (it, factor, line_total) in lines {
        sqlx::query(
            "INSERT INTO purchase_items (id, purchase_id, product_id, presentation_id, qty_presentations, \
             factor_snapshot, unit_cost_cents, total_cents) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&id)
        .bind(&it.product_id)
        .bind(&it.presentation_id)
        .bind(it.qty_presentations)
        .bind(factor)
        .bind(it.unit_cost_cents)
        .bind(line_total)
        .execute(&mut *conn)
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Confirma una vez: entrada de stock (COMPRA) por línea y último costo como referencia.
/// Todo en una transacción; la segunda confirmación se rechaza.
pub async fn confirm_purchase(pool: &AnyPool, id: &str) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;
    let st = status_of(conn, id).await?;
    if st != DRAFT {
        return Err(AppError::Validation(format!("la compra ya está {}", if st == CONFIRMED { "confirmada" } else { "anulada" })));
    }
    // Transición condicional: si dos confirmaciones compiten, solo una cambia la fila.
    let r = sqlx::query("UPDATE purchases SET status = 'CONFIRMED' WHERE id = ? AND status = 'DRAFT'")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if r.rows_affected() != 1 {
        return Err(AppError::Validation("la compra ya fue confirmada".into()));
    }
    let number: String = sqlx::query_scalar("SELECT number FROM purchases WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    let items = sqlx::query(
        "SELECT product_id, qty_presentations, factor_snapshot, unit_cost_cents \
         FROM purchase_items WHERE purchase_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    if items.is_empty() {
        return Err(AppError::Validation("la compra no tiene líneas".into()));
    }
    let ts = now();
    for r in items {
        let product_id: String = r.get("product_id");
        let qty: i64 = r.get("qty_presentations");
        let factor: i64 = r.get("factor_snapshot");
        let unit_cost: i64 = r.get("unit_cost_cents");
        let base_qty = qty
            .checked_mul(factor)
            .ok_or_else(|| AppError::Validation("cantidad fuera de rango".into()))?;
        apply_movement(&mut *conn, &product_id, base_qty, "COMPRA", "COMPRA", Some(id), Some(&number)).await?;
        sqlx::query("UPDATE products SET reference_cost_cents = ?, updated_at = ? WHERE id = ?")
            .bind(cost_per_base_unit(unit_cost, factor))
            .bind(&ts)
            .bind(&product_id)
            .execute(&mut *conn)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Un borrador descartado queda como CANCELED (trazable, sin efecto en stock).
pub async fn cancel_draft(pool: &AnyPool, id: &str) -> AppResult<()> {
    let r = sqlx::query("UPDATE purchases SET status = 'CANCELED' WHERE id = ? AND status = 'DRAFT'")
        .bind(id)
        .execute(pool)
        .await?;
    if r.rows_affected() != 1 {
        return Err(AppError::Validation("solo se pueden descartar compras en borrador".into()));
    }
    Ok(())
}

fn summary_of(r: &sqlx::any::AnyRow) -> PurchaseSummary {
    PurchaseSummary {
        id: r.get("id"),
        number: r.get("number"),
        supplier_id: r.get("supplier_id"),
        supplier_name: r.get("supplier_name"),
        status: r.get("status"),
        purchased_at: r.get("purchased_at"),
        total_cents: r.get("total_cents"),
    }
}

const SUMMARY_SQL: &str = "SELECT p.id AS id, p.number AS number, p.supplier_id AS supplier_id, s.name AS supplier_name, \
     p.status AS status, p.purchased_at AS purchased_at, p.total_cents AS total_cents \
     FROM purchases p LEFT JOIN suppliers s ON s.id = p.supplier_id";

pub async fn list_purchases(pool: &AnyPool, status: Option<&str>) -> AppResult<Vec<PurchaseSummary>> {
    let rows = match status {
        Some(st) => {
            sqlx::query(sqlx::AssertSqlSafe(format!("{SUMMARY_SQL} WHERE p.status = ? ORDER BY p.purchased_at DESC, p.number DESC LIMIT 500")))
                .bind(st.to_string())
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query(sqlx::AssertSqlSafe(format!("{SUMMARY_SQL} ORDER BY p.purchased_at DESC, p.number DESC LIMIT 500")))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.iter().map(summary_of).collect())
}

pub async fn get_purchase(pool: &AnyPool, id: &str) -> AppResult<PurchaseDetail> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SUMMARY_SQL} WHERE p.id = ?")))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("compra".into()))?;
    let items = sqlx::query(
        "SELECT i.id AS id, i.product_id AS product_id, pr.name AS product_name, i.presentation_id AS presentation_id, \
         pp.name AS presentation_name, i.qty_presentations AS qty_presentations, i.factor_snapshot AS factor_snapshot, \
         i.unit_cost_cents AS unit_cost_cents, i.total_cents AS total_cents \
         FROM purchase_items i JOIN products pr ON pr.id = i.product_id \
         JOIN product_presentations pp ON pp.id = i.presentation_id \
         WHERE i.purchase_id = ? ORDER BY pr.name",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(PurchaseDetail {
        summary: summary_of(&row),
        items: items
            .into_iter()
            .map(|r| PurchaseItemView {
                id: r.get("id"),
                product_id: r.get("product_id"),
                product_name: r.get("product_name"),
                presentation_id: r.get("presentation_id"),
                presentation_name: r.get("presentation_name"),
                qty_presentations: r.get("qty_presentations"),
                factor_snapshot: r.get("factor_snapshot"),
                unit_cost_cents: r.get("unit_cost_cents"),
                total_cents: r.get("total_cents"),
            })
            .collect(),
    })
}
