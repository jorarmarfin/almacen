use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, AnyPool, Row};

use crate::cash;
use crate::error::{AppError, AppResult};
use crate::inventory::apply_movement;
use crate::util::{new_id, next_counter, now};

#[derive(Debug, Deserialize)]
pub struct SaleItemInput {
    pub product_id: String,
    pub presentation_id: String,
    pub qty_presentations: i64,
    /// Descuento de la línea en céntimos (total de la línea, no por unidad).
    #[serde(default)]
    pub discount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct SalePaymentInput {
    pub method: String,
    /// Importe asignado a este método (neto cobrado).
    pub amount_cents: i64,
    /// Solo efectivo: lo entregado por el cliente (>= amount); el vuelto = received - amount.
    pub received_cents: Option<i64>,
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaleInput {
    /// Clave de idempotencia generada por la UI al iniciar la venta (RN-05).
    pub client_request_id: String,
    /// Descuento sobre el total (después de descuentos de línea).
    #[serde(default)]
    pub discount_cents: i64,
    pub items: Vec<SaleItemInput>,
    pub payments: Vec<SalePaymentInput>,
}

#[derive(Debug, Serialize)]
pub struct SaleSummary {
    pub id: String,
    pub number: String,
    pub status: String,
    pub sold_at: String,
    pub subtotal_cents: i64,
    pub discount_cents: i64,
    pub total_cents: i64,
    pub canceled_at: Option<String>,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SaleItemView {
    pub product_name: String,
    pub presentation_name: String,
    pub qty_presentations: i64,
    pub factor_snapshot: i64,
    pub unit_price_cents: i64,
    pub discount_cents: i64,
    pub line_total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct SalePaymentView {
    pub method: String,
    pub amount_cents: i64,
    pub received_cents: Option<i64>,
    pub change_cents: Option<i64>,
    pub reference: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SaleDetail {
    #[serde(flatten)]
    pub summary: SaleSummary,
    pub items: Vec<SaleItemView>,
    pub payments: Vec<SalePaymentView>,
}

const METHODS: [&str; 4] = ["EFECTIVO", "YAPE", "PLIN", "OTRO"];

fn overflow() -> AppError {
    AppError::Validation("monto fuera de rango".into())
}

struct Line {
    product_id: String,
    product_name: String,
    presentation_id: String,
    qty: i64,
    factor: i64,
    unit_price: i64,
    discount: i64,
    line_total: i64,
    cost_per_base: i64,
}

async fn find_by_request(conn: &mut AnyConnection, request_id: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT id FROM sales WHERE client_request_id = ?")
        .bind(request_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Confirma una venta en una sola transacción: cabecera, líneas, pagos, stock y caja.
/// Idempotente por `client_request_id`: repetir devuelve la misma venta sin duplicar nada.
pub async fn confirm_sale(pool: &AnyPool, input: &SaleInput) -> AppResult<SaleDetail> {
    let request_id = input.client_request_id.trim();
    if request_id.is_empty() {
        return Err(AppError::Validation("falta la clave de la operación".into()));
    }
    match confirm_inner(pool, input, request_id).await {
        Ok(id) => get_sale(pool, &id).await,
        Err(e) => {
            // Carrera entre dos confirmaciones idénticas: la UNIQUE rechazó la segunda; devolver la primera.
            let mut conn = pool.acquire().await?;
            if let Some(id) = find_by_request(&mut conn, request_id).await? {
                drop(conn);
                return get_sale(pool, &id).await;
            }
            Err(e)
        }
    }
}

async fn confirm_inner(pool: &AnyPool, input: &SaleInput, request_id: &str) -> AppResult<String> {
    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;

    if let Some(existing) = find_by_request(conn, request_id).await? {
        return Ok(existing);
    }
    if input.items.is_empty() {
        return Err(AppError::Validation("el carrito está vacío".into()));
    }
    if input.discount_cents < 0 {
        return Err(AppError::Validation("el descuento no puede ser negativo".into()));
    }
    let session_id = cash::open_session_id(conn)
        .await?
        .ok_or_else(|| AppError::Validation("abre la caja para poder vender".into()))?;

    // Líneas con precio, factor y costo congelados desde la BD (la UI no dicta precios).
    let mut lines = Vec::new();
    let mut subtotal: i64 = 0;
    for it in &input.items {
        if it.qty_presentations <= 0 {
            return Err(AppError::Validation("la cantidad debe ser mayor a cero".into()));
        }
        if it.discount_cents < 0 {
            return Err(AppError::Validation("el descuento no puede ser negativo".into()));
        }
        let row = sqlx::query(
            "SELECT p.name AS pname, p.reference_cost_cents AS cost, pp.factor_to_base AS factor, pp.sale_price_cents AS price \
             FROM products p JOIN product_presentations pp ON pp.product_id = p.id \
             WHERE p.id = ? AND pp.id = ? AND p.active = 1 AND pp.active = 1",
        )
        .bind(&it.product_id)
        .bind(&it.presentation_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::Validation("producto o presentación no disponible para la venta".into()))?;
        let price: i64 = row.get("price");
        let gross = it.qty_presentations.checked_mul(price).ok_or_else(overflow)?;
        if it.discount_cents > gross {
            return Err(AppError::Validation(format!(
                "el descuento de «{}» supera el importe de la línea",
                row.get::<String, _>("pname")
            )));
        }
        let line_total = gross - it.discount_cents;
        subtotal = subtotal.checked_add(line_total).ok_or_else(overflow)?;
        lines.push(Line {
            product_id: it.product_id.clone(),
            product_name: row.get("pname"),
            presentation_id: it.presentation_id.clone(),
            qty: it.qty_presentations,
            factor: row.get("factor"),
            unit_price: price,
            discount: it.discount_cents,
            line_total,
            cost_per_base: row.get("cost"),
        });
    }
    if input.discount_cents > subtotal {
        return Err(AppError::Validation("el descuento supera el subtotal".into()));
    }
    let total = subtotal - input.discount_cents;
    if total <= 0 {
        return Err(AppError::Validation("el total de la venta debe ser mayor a cero".into()));
    }

    // Pagos: deben cuadrar exactamente con el total (RN-06).
    if input.payments.is_empty() {
        return Err(AppError::Validation("registra al menos una forma de pago".into()));
    }
    let mut paid: i64 = 0;
    for p in &input.payments {
        if !METHODS.contains(&p.method.as_str()) {
            return Err(AppError::Validation(format!("método de pago inválido: {}", p.method)));
        }
        if p.amount_cents <= 0 {
            return Err(AppError::Validation("el importe de cada pago debe ser mayor a cero".into()));
        }
        if p.method == "EFECTIVO" {
            if let Some(r) = p.received_cents {
                if r < p.amount_cents {
                    return Err(AppError::Validation("el efectivo recibido no cubre el importe".into()));
                }
            }
        }
        paid = paid.checked_add(p.amount_cents).ok_or_else(overflow)?;
    }
    if paid != total {
        return Err(AppError::Validation(format!(
            "los pagos ({paid}) no cuadran con el total ({total}) en céntimos"
        )));
    }

    let sale_id = new_id();
    let n = next_counter(conn, "sale").await?;
    let number = format!("V-{n:06}");
    sqlx::query(
        "INSERT INTO sales (id, number, client_request_id, status, sold_at, subtotal_cents, discount_cents, total_cents) \
         VALUES (?, ?, ?, 'CONFIRMED', ?, ?, ?, ?)",
    )
    .bind(&sale_id)
    .bind(&number)
    .bind(request_id)
    .bind(now())
    .bind(subtotal)
    .bind(input.discount_cents)
    .bind(total)
    .execute(&mut *conn)
    .await?;

    for l in &lines {
        sqlx::query(
            "INSERT INTO sale_items (id, sale_id, product_id, presentation_id, qty_presentations, factor_snapshot, \
             unit_price_cents, discount_cents, line_total_cents, cost_snapshot_cents_per_base_unit) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&sale_id)
        .bind(&l.product_id)
        .bind(&l.presentation_id)
        .bind(l.qty)
        .bind(l.factor)
        .bind(l.unit_price)
        .bind(l.discount)
        .bind(l.line_total)
        .bind(l.cost_per_base)
        .execute(&mut *conn)
        .await?;
        let base_qty = l.qty.checked_mul(l.factor).ok_or_else(overflow)?;
        apply_movement(&mut *conn, &l.product_id, -base_qty, "VENTA", "VENTA", Some(&sale_id), Some(&number))
            .await
            .map_err(|e| match e {
                AppError::InsufficientStock { available } => AppError::Validation(format!(
                    "stock insuficiente de «{}» (disponible: {available})",
                    l.product_name
                )),
                other => other,
            })?;
    }

    let mut cash_in: i64 = 0;
    for p in &input.payments {
        let is_cash = p.method == "EFECTIVO";
        let received = if is_cash { Some(p.received_cents.unwrap_or(p.amount_cents)) } else { None };
        let change = received.map(|r| r - p.amount_cents);
        sqlx::query(
            "INSERT INTO sale_payments (id, sale_id, method, amount_cents, received_cents, change_cents, reference) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&sale_id)
        .bind(&p.method)
        .bind(p.amount_cents)
        .bind(received)
        .bind(change)
        .bind(p.reference.as_ref().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()))
        .execute(&mut *conn)
        .await?;
        if is_cash {
            cash_in += p.amount_cents;
        }
    }
    // Solo el efectivo afecta el saldo físico de caja (RF-04/05).
    if cash_in > 0 {
        cash::record_movement(conn, &session_id, "VENTA", "EFECTIVO", cash_in, "SALE", Some(&sale_id), &number).await?;
    }
    tx.commit().await?;
    Ok(sale_id)
}

/// Anula una venta confirmada exactamente una vez: repone stock y revierte el efectivo.
/// El contra-movimiento va a la caja abierta actual (RN-11: no se alteran periodos cerrados).
pub async fn cancel_sale(pool: &AnyPool, sale_id: &str, reason: &str) -> AppResult<SaleDetail> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation("el motivo de la anulación es obligatorio".into()));
    }
    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;
    let r = sqlx::query(
        "UPDATE sales SET status = 'CANCELED', canceled_at = ?, cancel_reason = ? WHERE id = ? AND status = 'CONFIRMED'",
    )
    .bind(now())
    .bind(reason)
    .bind(sale_id)
    .execute(&mut *conn)
    .await?;
    if r.rows_affected() != 1 {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sales WHERE id = ?")
            .bind(sale_id)
            .fetch_optional(&mut *conn)
            .await?;
        return Err(match exists {
            None => AppError::NotFound("venta".into()),
            Some(_) => AppError::Validation("la venta ya fue anulada".into()),
        });
    }
    let number: String = sqlx::query_scalar("SELECT number FROM sales WHERE id = ?")
        .bind(sale_id)
        .fetch_one(&mut *conn)
        .await?;
    let items = sqlx::query("SELECT product_id, qty_presentations, factor_snapshot FROM sale_items WHERE sale_id = ?")
        .bind(sale_id)
        .fetch_all(&mut *conn)
        .await?;
    for it in items {
        let pid: String = it.get("product_id");
        let base: i64 = it.get::<i64, _>("qty_presentations") * it.get::<i64, _>("factor_snapshot");
        apply_movement(&mut *conn, &pid, base, "ANULACION_VENTA", "VENTA", Some(sale_id), Some(reason)).await?;
    }
    let cash_out: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(amount_cents), 0) AS SIGNED) FROM sale_payments WHERE sale_id = ? AND method = 'EFECTIVO'",
    )
    .bind(sale_id)
    .fetch_one(&mut *conn)
    .await?;
    if cash_out > 0 {
        let session = cash::open_session_id(conn)
            .await?
            .ok_or_else(|| AppError::Validation("abre la caja para anular una venta cobrada en efectivo".into()))?;
        cash::record_movement(
            conn,
            &session,
            "ANULACION_VENTA",
            "EFECTIVO",
            -cash_out,
            "SALE",
            Some(sale_id),
            &format!("Anulación {number}"),
        )
        .await?;
    }
    tx.commit().await?;
    get_sale(pool, sale_id).await
}

fn summary_of(r: &sqlx::any::AnyRow) -> SaleSummary {
    SaleSummary {
        id: r.get("id"),
        number: r.get("number"),
        status: r.get("status"),
        sold_at: r.get("sold_at"),
        subtotal_cents: r.get("subtotal_cents"),
        discount_cents: r.get("discount_cents"),
        total_cents: r.get("total_cents"),
        canceled_at: r.get("canceled_at"),
        cancel_reason: r.get("cancel_reason"),
    }
}

const SALE_COLS: &str = "id, number, status, sold_at, subtotal_cents, discount_cents, total_cents, canceled_at, cancel_reason";

/// Ventas más recientes primero. `from`/`to` RFC3339 UTC inclusive; `status` opcional.
pub async fn list_sales(
    pool: &AnyPool,
    from: Option<&str>,
    to: Option<&str>,
    status: Option<&str>,
) -> AppResult<Vec<SaleSummary>> {
    let mut sql = format!("SELECT {SALE_COLS} FROM sales WHERE 1 = 1");
    if from.is_some() {
        sql.push_str(" AND sold_at >= ?");
    }
    if to.is_some() {
        sql.push_str(" AND sold_at <= ?");
    }
    if status.is_some() {
        sql.push_str(" AND status = ?");
    }
    sql.push_str(" ORDER BY sold_at DESC, number DESC LIMIT 500");
    // Solo fragmentos estáticos; los valores van como parámetros.
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for v in [from, to, status].into_iter().flatten() {
        q = q.bind(v.to_string());
    }
    Ok(q.fetch_all(pool).await?.iter().map(summary_of).collect())
}

pub async fn get_sale(pool: &AnyPool, id: &str) -> AppResult<SaleDetail> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {SALE_COLS} FROM sales WHERE id = ?")))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("venta".into()))?;
    let items = sqlx::query(
        "SELECT p.name AS product_name, pp.name AS presentation_name, i.qty_presentations AS qty, \
         i.factor_snapshot AS factor, i.unit_price_cents AS price, i.discount_cents AS discount, i.line_total_cents AS total \
         FROM sale_items i JOIN products p ON p.id = i.product_id \
         JOIN product_presentations pp ON pp.id = i.presentation_id WHERE i.sale_id = ? ORDER BY p.name",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let pays = sqlx::query(
        "SELECT method, amount_cents, received_cents, change_cents, reference FROM sale_payments WHERE sale_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(SaleDetail {
        summary: summary_of(&row),
        items: items
            .into_iter()
            .map(|r| SaleItemView {
                product_name: r.get("product_name"),
                presentation_name: r.get("presentation_name"),
                qty_presentations: r.get("qty"),
                factor_snapshot: r.get("factor"),
                unit_price_cents: r.get("price"),
                discount_cents: r.get("discount"),
                line_total_cents: r.get("total"),
            })
            .collect(),
        payments: pays
            .into_iter()
            .map(|r| SalePaymentView {
                method: r.get("method"),
                amount_cents: r.get("amount_cents"),
                received_cents: r.get("received_cents"),
                change_cents: r.get("change_cents"),
                reference: r.get("reference"),
            })
            .collect(),
    })
}
