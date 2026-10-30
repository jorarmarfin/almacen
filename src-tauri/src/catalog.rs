use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, AnyPool, Row};

use crate::error::{AppError, AppResult};
use crate::util::{new_id, next_counter, now};

// ---------- Categorías ----------

#[derive(Debug, Serialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub product_count: i64,
}

pub async fn list_categories(pool: &AnyPool) -> AppResult<Vec<Category>> {
    let rows = sqlx::query(
        "SELECT c.id AS id, c.name AS name, c.active AS active, \
         (SELECT COUNT(*) FROM products p WHERE p.category_id = c.id) AS product_count \
         FROM categories c ORDER BY c.name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Category {
            id: r.get("id"),
            name: r.get("name"),
            active: r.get::<i64, _>("active") != 0,
            product_count: r.get("product_count"),
        })
        .collect())
}

/// Solo se elimina una categoría sin productos; si los tiene, se desactiva.
pub async fn delete_category(pool: &AnyPool, id: &str) -> AppResult<()> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE category_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if n > 0 {
        return Err(AppError::Validation(format!(
            "la categoría tiene {n} producto(s); desactívala en lugar de eliminarla"
        )));
    }
    sqlx::query("DELETE FROM categories WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn save_category(pool: &AnyPool, id: Option<&str>, name: &str, active: bool) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("el nombre de la categoría es obligatorio".into()));
    }
    let dup: Option<String> = sqlx::query_scalar("SELECT id FROM categories WHERE name = ? AND id <> ?")
        .bind(name)
        .bind(id.unwrap_or(""))
        .fetch_optional(pool)
        .await?;
    if dup.is_some() {
        return Err(AppError::Validation(format!("ya existe la categoría «{name}»")));
    }
    match id {
        Some(id) => {
            sqlx::query("UPDATE categories SET name = ?, active = ? WHERE id = ?")
                .bind(name)
                .bind(active as i64)
                .bind(id)
                .execute(pool)
                .await?;
            Ok(id.to_string())
        }
        None => {
            let id = new_id();
            sqlx::query("INSERT INTO categories (id, name, active) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(name)
                .bind(active as i64)
                .execute(pool)
                .await?;
            Ok(id)
        }
    }
}

async fn category_by_name(conn: &mut AnyConnection, name: &str) -> AppResult<String> {
    let found: Option<String> = sqlx::query_scalar("SELECT id FROM categories WHERE name = ?")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(id) = found {
        return Ok(id);
    }
    let id = new_id();
    sqlx::query("INSERT INTO categories (id, name, active) VALUES (?, ?, 1)")
        .bind(&id)
        .bind(name)
        .execute(&mut *conn)
        .await?;
    Ok(id)
}

// ---------- Productos ----------

#[derive(Debug, Clone, Deserialize)]
pub struct PresentationInput {
    pub id: Option<String>,
    pub name: String,
    pub factor_to_base: i64,
    pub sale_price_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductInput {
    pub id: Option<String>,
    pub category_id: Option<String>,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub name: String,
    pub base_unit: String,
    pub reference_cost_cents: i64,
    pub min_stock: Option<i64>,
    pub presentations: Vec<PresentationInput>,
}

#[derive(Debug, Serialize)]
pub struct Presentation {
    pub id: String,
    pub name: String,
    pub factor_to_base: i64,
    pub sale_price_cents: i64,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct Product {
    pub id: String,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub sku: String,
    pub barcode: Option<String>,
    pub name: String,
    pub base_unit: String,
    pub reference_cost_cents: i64,
    pub min_stock: i64,
    pub active: bool,
    pub stock: i64,
    pub presentations: Vec<Presentation>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductFilter {
    pub search: Option<String>,
    pub category_id: Option<String>,
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub low_stock_only: bool,
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn validate(input: &ProductInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("el nombre es obligatorio".into()));
    }
    if input.base_unit.trim().is_empty() {
        return Err(AppError::Validation("la unidad base es obligatoria".into()));
    }
    if input.reference_cost_cents < 0 {
        return Err(AppError::Validation("el costo no puede ser negativo".into()));
    }
    if input.min_stock.is_some_and(|m| m < 0) {
        return Err(AppError::Validation("el stock mínimo no puede ser negativo".into()));
    }
    if input.presentations.is_empty() {
        return Err(AppError::Validation("agrega al menos una presentación".into()));
    }
    let mut names = HashSet::new();
    for p in &input.presentations {
        if p.name.trim().is_empty() {
            return Err(AppError::Validation("toda presentación necesita nombre".into()));
        }
        if p.factor_to_base <= 0 {
            return Err(AppError::Validation(format!("el factor de «{}» debe ser un entero mayor a cero", p.name)));
        }
        if p.sale_price_cents < 0 {
            return Err(AppError::Validation(format!("el precio de «{}» no puede ser negativo", p.name)));
        }
        if !names.insert(p.name.trim().to_lowercase()) {
            return Err(AppError::Validation(format!("presentación repetida: «{}»", p.name)));
        }
    }
    Ok(())
}

async fn next_free_sku(conn: &mut AnyConnection) -> AppResult<String> {
    loop {
        let n = next_counter(conn, "sku").await?;
        let sku = format!("P-{n:06}");
        let taken: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE sku = ?")
            .bind(&sku)
            .fetch_optional(&mut *conn)
            .await?;
        if taken.is_none() {
            return Ok(sku);
        }
    }
}

/// Crea o edita un producto con sus presentaciones dentro de la transacción del llamador.
/// Presentaciones ausentes en la edición se desactivan (nunca se borran).
pub async fn save_product_tx(conn: &mut AnyConnection, input: &ProductInput) -> AppResult<String> {
    validate(input)?;
    let sku_in = clean(&input.sku);
    let barcode = clean(&input.barcode);
    let name = input.name.trim().to_string();
    let own_id = input.id.clone().unwrap_or_default();

    if let Some(sku) = &sku_in {
        let dup: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE sku = ? AND id <> ?")
            .bind(sku)
            .bind(&own_id)
            .fetch_optional(&mut *conn)
            .await?;
        if dup.is_some() {
            return Err(AppError::Validation(format!("el SKU «{sku}» ya existe")));
        }
    }
    if let Some(bc) = &barcode {
        let dup: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE barcode = ? AND id <> ?")
            .bind(bc)
            .bind(&own_id)
            .fetch_optional(&mut *conn)
            .await?;
        if dup.is_some() {
            return Err(AppError::Validation(format!("el código de barras «{bc}» ya existe")));
        }
    }

    let now = now();
    let product_id = match &input.id {
        Some(id) => {
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound("producto".into()));
            }
            let sku = match sku_in {
                Some(s) => s,
                None => sqlx::query_scalar("SELECT sku FROM products WHERE id = ?").bind(id).fetch_one(&mut *conn).await?,
            };
            let min_stock = match input.min_stock {
                Some(m) => m,
                None => sqlx::query_scalar("SELECT min_stock FROM products WHERE id = ?").bind(id).fetch_one(&mut *conn).await?,
            };
            sqlx::query(
                "UPDATE products SET category_id = ?, sku = ?, barcode = ?, name = ?, base_unit = ?, \
                 reference_cost_cents = ?, min_stock = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&input.category_id)
            .bind(&sku)
            .bind(&barcode)
            .bind(&name)
            .bind(input.base_unit.trim())
            .bind(input.reference_cost_cents)
            .bind(min_stock)
            .bind(&now)
            .bind(id)
            .execute(&mut *conn)
            .await?;
            id.clone()
        }
        None => {
            let id = new_id();
            let sku = match sku_in {
                Some(s) => s,
                None => next_free_sku(conn).await?,
            };
            let min_stock = match input.min_stock {
                Some(m) => m,
                None => sqlx::query_scalar("SELECT default_min_stock FROM business_settings WHERE id = 1")
                    .fetch_optional(&mut *conn)
                    .await?
                    .unwrap_or(0),
            };
            sqlx::query(
                "INSERT INTO products (id, category_id, sku, barcode, name, base_unit, reference_cost_cents, \
                 min_stock, active, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(&id)
            .bind(&input.category_id)
            .bind(&sku)
            .bind(&barcode)
            .bind(&name)
            .bind(input.base_unit.trim())
            .bind(input.reference_cost_cents)
            .bind(min_stock)
            .bind(&now)
            .bind(&now)
            .execute(&mut *conn)
            .await?;
            sqlx::query("INSERT INTO stock_balances (product_id, qty_base) VALUES (?, 0)")
                .bind(&id)
                .execute(&mut *conn)
                .await?;
            id
        }
    };

    let existing: HashSet<String> = sqlx::query_scalar("SELECT id FROM product_presentations WHERE product_id = ?")
        .bind(&product_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
    let mut kept = HashSet::new();
    for p in &input.presentations {
        match &p.id {
            Some(pid) if existing.contains(pid) => {
                sqlx::query(
                    "UPDATE product_presentations SET name = ?, factor_to_base = ?, sale_price_cents = ?, active = 1 \
                     WHERE id = ? AND product_id = ?",
                )
                .bind(p.name.trim())
                .bind(p.factor_to_base)
                .bind(p.sale_price_cents)
                .bind(pid)
                .bind(&product_id)
                .execute(&mut *conn)
                .await?;
                kept.insert(pid.clone());
            }
            _ => {
                let pid = new_id();
                sqlx::query(
                    "INSERT INTO product_presentations (id, product_id, name, factor_to_base, sale_price_cents, active) \
                     VALUES (?, ?, ?, ?, ?, 1)",
                )
                .bind(&pid)
                .bind(&product_id)
                .bind(p.name.trim())
                .bind(p.factor_to_base)
                .bind(p.sale_price_cents)
                .execute(&mut *conn)
                .await?;
                kept.insert(pid);
            }
        }
    }
    for old in existing.difference(&kept) {
        sqlx::query("UPDATE product_presentations SET active = 0 WHERE id = ?")
            .bind(old)
            .execute(&mut *conn)
            .await?;
    }
    Ok(product_id)
}

pub async fn save_product(pool: &AnyPool, input: &ProductInput) -> AppResult<String> {
    let mut tx = pool.begin().await?;
    let id = save_product_tx(&mut tx, input).await?;
    tx.commit().await?;
    Ok(id)
}

/// Los productos con historial solo se desactivan, nunca se eliminan.
pub async fn set_product_active(pool: &AnyPool, id: &str, active: bool) -> AppResult<()> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("producto".into()));
    }
    sqlx::query("UPDATE products SET active = ?, updated_at = ? WHERE id = ?")
        .bind(active as i64)
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_products(pool: &AnyPool, f: &ProductFilter) -> AppResult<Vec<Product>> {
    let mut sql = String::from(
        "SELECT p.id AS id, p.category_id AS category_id, c.name AS category_name, p.sku AS sku, p.barcode AS barcode, \
         p.name AS name, p.base_unit AS base_unit, p.reference_cost_cents AS reference_cost_cents, \
         p.min_stock AS min_stock, p.active AS active, COALESCE(b.qty_base, 0) AS stock \
         FROM products p LEFT JOIN categories c ON c.id = p.category_id \
         LEFT JOIN stock_balances b ON b.product_id = p.id WHERE 1 = 1",
    );
    let search = f.search.as_ref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    if search.is_some() {
        sql.push_str(" AND (LOWER(p.name) LIKE ? OR LOWER(p.sku) LIKE ? OR LOWER(p.barcode) LIKE ?)");
    }
    if f.category_id.is_some() {
        sql.push_str(" AND p.category_id = ?");
    }
    if !f.include_inactive {
        sql.push_str(" AND p.active = 1");
    }
    if f.low_stock_only {
        sql.push_str(" AND COALESCE(b.qty_base, 0) <= p.min_stock");
    }
    sql.push_str(" ORDER BY p.name LIMIT 1000");
    // Solo fragmentos estáticos concatenados; los valores van como parámetros.
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    if let Some(s) = &search {
        let like = format!("%{s}%");
        q = q.bind(like.clone()).bind(like.clone()).bind(like);
    }
    if let Some(c) = &f.category_id {
        q = q.bind(c.clone());
    }
    let rows = q.fetch_all(pool).await?;

    // Solo las presentaciones de los productos devueltos (máx. 1000 por el LIMIT).
    let ids: Vec<String> = rows.iter().map(|r| r.get::<String, _>("id")).collect();
    let mut pres_rows = Vec::new();
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        // Solo se concatenan marcadores `?`; los ids van como parámetros.
        let mut q = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, product_id, name, factor_to_base, sale_price_cents, active \
             FROM product_presentations WHERE product_id IN ({marks}) ORDER BY factor_to_base, name"
        )));
        for id in chunk {
            q = q.bind(id.clone());
        }
        pres_rows.extend(q.fetch_all(pool).await?);
    }
    let mut by_product: std::collections::HashMap<String, Vec<Presentation>> = Default::default();
    for r in pres_rows {
        by_product.entry(r.get("product_id")).or_default().push(Presentation {
            id: r.get("id"),
            name: r.get("name"),
            factor_to_base: r.get("factor_to_base"),
            sale_price_cents: r.get("sale_price_cents"),
            active: r.get::<i64, _>("active") != 0,
        });
    }
    Ok(rows
        .into_iter()
        .map(|r| {
            let id: String = r.get("id");
            Product {
                presentations: by_product.remove(&id).unwrap_or_default(),
                id,
                category_id: r.get("category_id"),
                category_name: r.get("category_name"),
                sku: r.get("sku"),
                barcode: r.get("barcode"),
                name: r.get("name"),
                base_unit: r.get("base_unit"),
                reference_cost_cents: r.get("reference_cost_cents"),
                min_stock: r.get("min_stock"),
                active: r.get::<i64, _>("active") != 0,
                stock: r.get("stock"),
            }
        })
        .collect())
}

// ---------- Importación CSV ----------

#[derive(Debug, Serialize)]
pub struct RowResult {
    pub line: usize,
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub rows: Vec<RowResult>,
    pub imported: usize,
    pub dry_run: bool,
}

/// Plantilla CSV descargable: separador `;` y BOM UTF-8 para que Excel (es-PE) la abra en columnas.
/// Las líneas que empiezan con `#` se ignoran al importar (filas de ejemplo/ayuda).
pub fn products_template() -> String {
    let mut s = String::from("\u{feff}");
    s.push_str("# PLANTILLA DE PRODUCTOS - las líneas que empiezan con # se ignoran. Borra los ejemplos o déjalos.\n");
    s.push_str("# nombre es obligatorio. sku vacío = automático. costo/precio en soles (12.50). stock_inicial genera un ajuste STOCK_INICIAL.\n");
    s.push_str("nombre;sku;codigo_barras;categoria;unidad;costo;precio;stock_minimo;stock_inicial\n");
    s.push_str("#Arroz Costeño 1kg;ARR-001;7750182000011;Abarrotes;unidad;3.50;4.20;10;50\n");
    s.push_str("#Gaseosa 500ml;;;Bebidas;unidad;1.50;2.50;12;24\n");
    s
}

/// Columnas: nombre, sku, codigo_barras, categoria, unidad, costo, precio, stock_minimo, stock_inicial.
/// Cada fila va en su propia transacción; con `dry_run` se valida y se revierte.
pub async fn import_csv(pool: &AnyPool, content: &str, dry_run: bool) -> AppResult<ImportReport> {
    use crate::domain::money::parse_cents;
    use crate::inventory::{apply_movement, AJUSTE_ENTRADA};

    let content = content.trim_start_matches('\u{feff}');
    // El separador se detecta en la cabecera (primera línea que no es comentario).
    let header_line = content.lines().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    let delim = if header_line.is_some_and(|l| l.contains(';') && !l.contains(',')) { b';' } else { b',' };
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).comment(Some(b'#')).flexible(true).trim(csv::Trim::All).from_reader(content.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| AppError::Validation(format!("CSV inválido: {e}")))?
        .iter()
        .map(|h| h.to_lowercase())
        .collect();
    if !headers.iter().any(|h| h == "nombre") {
        return Err(AppError::Validation("el CSV necesita la columna «nombre»".into()));
    }
    let col = |rec: &csv::StringRecord, name: &str| -> Option<String> {
        headers.iter().position(|h| h == name).and_then(|i| rec.get(i)).map(str::to_string).filter(|s| !s.is_empty())
    };

    let mut rows = Vec::new();
    let mut imported = 0;
    // dry_run: una sola transacción para todo el archivo (detecta duplicados entre filas) y rollback final.
    let mut dry_tx = if dry_run { Some(pool.begin().await?) } else { None };
    for rec in rdr.records() {
        let line = rec.as_ref().ok().and_then(|r| r.position()).map(|p| p.line() as usize).unwrap_or(0);
        let result: AppResult<()> = async {
            let rec = rec.map_err(|e| AppError::Validation(e.to_string()))?;
            let money = |name: &str| -> AppResult<i64> {
                match col(&rec, name) {
                    None => Ok(0),
                    Some(v) => parse_cents(&v).ok_or_else(|| AppError::Validation(format!("«{name}» inválido: {v}"))),
                }
            };
            let int = |name: &str| -> AppResult<Option<i64>> {
                match col(&rec, name) {
                    None => Ok(None),
                    Some(v) => v
                        .parse::<i64>()
                        .ok()
                        .filter(|n| *n >= 0)
                        .map(Some)
                        .ok_or_else(|| AppError::Validation(format!("«{name}» inválido: {v}"))),
                }
            };
            let price = money("precio")?;
            let cost = money("costo")?;
            let min_stock = int("stock_minimo")?;
            let initial = int("stock_inicial")?.unwrap_or(0);

            let mut own_tx = if dry_run { None } else { Some(pool.begin().await?) };
            let conn: &mut AnyConnection = match (&mut own_tx, &mut dry_tx) {
                (Some(t), _) => &mut *t,
                (None, Some(t)) => &mut *t,
                _ => unreachable!(),
            };
            let category_id = match col(&rec, "categoria") {
                Some(c) => Some(category_by_name(&mut *conn, &c).await?),
                None => None,
            };
            let input = ProductInput {
                id: None,
                category_id,
                sku: col(&rec, "sku"),
                barcode: col(&rec, "codigo_barras"),
                name: col(&rec, "nombre").unwrap_or_default(),
                base_unit: col(&rec, "unidad").unwrap_or_else(|| "unidad".into()),
                reference_cost_cents: cost,
                min_stock,
                presentations: vec![PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: price }],
            };
            let pid = save_product_tx(&mut *conn, &input).await?;
            if initial > 0 {
                apply_movement(&mut *conn, &pid, initial, AJUSTE_ENTRADA, "AJUSTE", None, Some("STOCK_INICIAL")).await?;
            }
            if let Some(t) = own_tx {
                t.commit().await?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                imported += 1;
                rows.push(RowResult { line, ok: true, message: None });
            }
            Err(e) => rows.push(RowResult { line, ok: false, message: Some(e.to_string()) }),
        }
    }
    if let Some(t) = dry_tx {
        t.rollback().await?;
    }
    Ok(ImportReport { rows, imported, dry_run })
}
