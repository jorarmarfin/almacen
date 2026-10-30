use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sqlx::{AnyPool, Row};

use crate::cash::TypeTotal;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub sales_total_cents: i64,
    pub sales_count: i64,
    pub active_products: i64,
    pub low_stock_count: i64,
    pub cash_open: bool,
}

/// KPIs de Inicio para el rango dado (normalmente "hoy" en UTC calculado por la UI).
pub async fn dashboard(pool: &AnyPool, from: &str, to: &str) -> AppResult<Dashboard> {
    let r = sqlx::query(
        "SELECT COUNT(*) AS n, CAST(COALESCE(SUM(total_cents), 0) AS SIGNED) AS t FROM sales \
         WHERE status = 'CONFIRMED' AND sold_at >= ? AND sold_at <= ?",
    )
    .bind(from.to_string())
    .bind(to.to_string())
    .fetch_one(pool)
    .await?;
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE active = 1").fetch_one(pool).await?;
    let low: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM products p LEFT JOIN stock_balances b ON b.product_id = p.id \
         WHERE p.active = 1 AND COALESCE(b.qty_base, 0) <= p.min_stock",
    )
    .fetch_one(pool)
    .await?;
    let open: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cash_sessions WHERE status = 'OPEN'").fetch_one(pool).await?;
    Ok(Dashboard {
        sales_total_cents: r.get("t"),
        sales_count: r.get("n"),
        active_products: active,
        low_stock_count: low,
        cash_open: open > 0,
    })
}

#[derive(Debug, Serialize)]
pub struct DayTotal {
    pub date: String,
    pub count: i64,
    pub total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct SalesReport {
    /// Ventas confirmadas (netas de anuladas).
    pub total_cents: i64,
    pub count: i64,
    /// Anulaciones se informan aparte, nunca restadas en silencio.
    pub canceled_count: i64,
    pub canceled_total_cents: i64,
    pub by_method: Vec<TypeTotal>,
    pub by_day: Vec<DayTotal>,
    /// Margen bruto ESTIMADO: ingreso neto de línea - costo de referencia capturado al vender × unidades base.
    pub revenue_cents: i64,
    pub cost_cents: i64,
    pub margin_cents: i64,
}

pub fn local_date(iso: &str) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| iso.chars().take(10).collect())
}

pub fn local_datetime(iso: &str) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

pub async fn sales_report(pool: &AnyPool, from: &str, to: &str) -> AppResult<SalesReport> {
    let (f, t) = (from.to_string(), to.to_string());
    let mut report = SalesReport {
        total_cents: 0,
        count: 0,
        canceled_count: 0,
        canceled_total_cents: 0,
        by_method: vec![],
        by_day: vec![],
        revenue_cents: 0,
        cost_cents: 0,
        margin_cents: 0,
    };
    for r in sqlx::query(
        "SELECT status AS st, COUNT(*) AS n, CAST(COALESCE(SUM(total_cents), 0) AS SIGNED) AS t FROM sales \
         WHERE sold_at >= ? AND sold_at <= ? GROUP BY status",
    )
    .bind(f.clone())
    .bind(t.clone())
    .fetch_all(pool)
    .await?
    {
        match r.get::<String, _>("st").as_str() {
            "CONFIRMED" => {
                report.count = r.get("n");
                report.total_cents = r.get("t");
            }
            _ => {
                report.canceled_count = r.get("n");
                report.canceled_total_cents = r.get("t");
            }
        }
    }
    report.by_method = sqlx::query(
        "SELECT p.method AS k, CAST(SUM(p.amount_cents) AS SIGNED) AS t FROM sale_payments p \
         JOIN sales s ON s.id = p.sale_id WHERE s.status = 'CONFIRMED' AND s.sold_at >= ? AND s.sold_at <= ? \
         GROUP BY p.method ORDER BY p.method",
    )
    .bind(f.clone())
    .bind(t.clone())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| TypeTotal { key: r.get("k"), total_cents: r.get("t") })
    .collect();

    // Agrupar por día LOCAL en Rust: independiente del motor y de la zona horaria de la BD.
    let mut days: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for r in sqlx::query("SELECT sold_at, total_cents FROM sales WHERE status = 'CONFIRMED' AND sold_at >= ? AND sold_at <= ?")
        .bind(f.clone())
        .bind(t.clone())
        .fetch_all(pool)
        .await?
    {
        let e = days.entry(local_date(&r.get::<String, _>("sold_at"))).or_default();
        e.0 += 1;
        e.1 += r.get::<i64, _>("total_cents");
    }
    report.by_day = days.into_iter().map(|(date, (count, total_cents))| DayTotal { date, count, total_cents }).collect();

    let m = sqlx::query(
        "SELECT CAST(COALESCE(SUM(i.line_total_cents), 0) AS SIGNED) AS rev, \
         CAST(COALESCE(SUM(i.cost_snapshot_cents_per_base_unit * i.qty_presentations * i.factor_snapshot), 0) AS SIGNED) AS cost \
         FROM sale_items i JOIN sales s ON s.id = i.sale_id \
         WHERE s.status = 'CONFIRMED' AND s.sold_at >= ? AND s.sold_at <= ?",
    )
    .bind(f)
    .bind(t)
    .fetch_one(pool)
    .await?;
    report.revenue_cents = m.get("rev");
    report.cost_cents = m.get("cost");
    report.margin_cents = report.revenue_cents - report.cost_cents;
    Ok(report)
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TopMetric {
    Units,
    Amount,
}

#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_id: String,
    pub name: String,
    pub units_base: i64,
    pub amount_cents: i64,
}

/// Más vendidos; `metric` define el orden (unidades base o monto) y la UI lo indica.
pub async fn top_products(pool: &AnyPool, from: &str, to: &str, metric: TopMetric, limit: i64) -> AppResult<Vec<TopProduct>> {
    let order = match metric {
        TopMetric::Units => "units DESC, amount DESC",
        TopMetric::Amount => "amount DESC, units DESC",
    };
    // `order` es una constante elegida por enum; los valores van como parámetros.
    let sql = format!(
        "SELECT p.id AS id, p.name AS name, CAST(SUM(i.qty_presentations * i.factor_snapshot) AS SIGNED) AS units, \
         CAST(SUM(i.line_total_cents) AS SIGNED) AS amount FROM sale_items i \
         JOIN sales s ON s.id = i.sale_id JOIN products p ON p.id = i.product_id \
         WHERE s.status = 'CONFIRMED' AND s.sold_at >= ? AND s.sold_at <= ? \
         GROUP BY p.id, p.name ORDER BY {order}, p.name LIMIT ?"
    );
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(from.to_string())
        .bind(to.to_string())
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| TopProduct { product_id: r.get("id"), name: r.get("name"), units_base: r.get("units"), amount_cents: r.get("amount") })
        .collect())
}

// ---------- Exportación CSV (Excel es-PE: BOM UTF-8, separador coma, punto decimal) ----------

fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", cents.abs() / 100, cents.abs() % 100)
}

fn to_csv(header: &[&str], rows: Vec<Vec<String>>) -> AppResult<String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(header).map_err(|e| AppError::Validation(e.to_string()))?;
    for r in rows {
        w.write_record(&r).map_err(|e| AppError::Validation(e.to_string()))?;
    }
    let bytes = w.into_inner().map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(format!("\u{feff}{}", String::from_utf8_lossy(&bytes)))
}

pub async fn export_products_csv(pool: &AnyPool) -> AppResult<String> {
    let products = crate::catalog::list_products(
        pool,
        &crate::catalog::ProductFilter { include_inactive: true, ..Default::default() },
    )
    .await?;
    let rows = products
        .into_iter()
        .map(|p| {
            let price = p
                .presentations
                .iter()
                .find(|x| x.active && x.factor_to_base == 1)
                .or_else(|| p.presentations.iter().find(|x| x.active))
                .map(|x| money(x.sale_price_cents))
                .unwrap_or_default();
            vec![
                p.sku,
                p.barcode.unwrap_or_default(),
                p.name,
                p.category_name.unwrap_or_default(),
                p.base_unit,
                money(p.reference_cost_cents),
                price,
                p.stock.to_string(),
                p.min_stock.to_string(),
                if p.active { "SI" } else { "NO" }.into(),
            ]
        })
        .collect();
    to_csv(
        &["sku", "codigo_barras", "nombre", "categoria", "unidad", "costo", "precio", "stock", "stock_minimo", "activo"],
        rows,
    )
}

pub async fn export_kardex_csv(pool: &AnyPool, from: &str, to: &str) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT m.occurred_at AS at, p.sku AS sku, p.name AS name, m.type AS type, m.qty_base_signed AS qty, \
         m.balance_before AS before, m.balance_after AS after, m.origin_type AS origin, m.note AS note \
         FROM stock_movements m JOIN products p ON p.id = m.product_id \
         WHERE m.occurred_at >= ? AND m.occurred_at <= ? ORDER BY m.occurred_at, m.seq, m.id",
    )
    .bind(from.to_string())
    .bind(to.to_string())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| {
        let opt = |c: &str| r.get::<Option<i64>, _>(c).map(|v| v.to_string()).unwrap_or_default();
        vec![
            local_datetime(&r.get::<String, _>("at")),
            r.get("sku"),
            r.get("name"),
            r.get("type"),
            r.get::<i64, _>("qty").to_string(),
            opt("before"),
            opt("after"),
            r.get("origin"),
            r.get::<Option<String>, _>("note").unwrap_or_default(),
        ]
    })
    .collect();
    to_csv(&["fecha", "sku", "producto", "tipo", "cantidad", "saldo_antes", "saldo_despues", "origen", "motivo"], rows)
}

pub async fn export_sales_csv(pool: &AnyPool, from: &str, to: &str) -> AppResult<String> {
    let method_sum = |m: &str| {
        format!(
            "(SELECT CAST(COALESCE(SUM(sp.amount_cents), 0) AS SIGNED) FROM sale_payments sp WHERE sp.sale_id = s.id AND sp.method = '{m}')"
        )
    };
    // Los nombres de método son constantes del código, no entrada del usuario.
    let sql = format!(
        "SELECT s.number AS number, s.sold_at AS at, s.status AS status, s.subtotal_cents AS sub, s.discount_cents AS disc, \
         s.total_cents AS total, {} AS efectivo, {} AS yape, {} AS plin, {} AS otro, s.cancel_reason AS reason \
         FROM sales s WHERE s.sold_at >= ? AND s.sold_at <= ? ORDER BY s.sold_at, s.number",
        method_sum("EFECTIVO"),
        method_sum("YAPE"),
        method_sum("PLIN"),
        method_sum("OTRO")
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| {
            vec![
                r.get("number"),
                local_datetime(&r.get::<String, _>("at")),
                if r.get::<String, _>("status") == "CONFIRMED" { "CONFIRMADA" } else { "ANULADA" }.into(),
                money(r.get("sub")),
                money(r.get("disc")),
                money(r.get("total")),
                money(r.get("efectivo")),
                money(r.get("yape")),
                money(r.get("plin")),
                money(r.get("otro")),
                r.get::<Option<String>, _>("reason").unwrap_or_default(),
            ]
        })
        .collect();
    to_csv(
        &["numero", "fecha", "estado", "subtotal", "descuento", "total", "efectivo", "yape", "plin", "otro", "motivo_anulacion"],
        rows,
    )
}
