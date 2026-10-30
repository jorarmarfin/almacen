use serde::Serialize;
use sqlx::{AnyConnection, AnyPool, Row};

use crate::error::{AppError, AppResult};
use crate::util::{new_id, next_counter, now};

pub const AJUSTE_ENTRADA: &str = "AJUSTE_ENTRADA";
pub const AJUSTE_SALIDA: &str = "AJUSTE_SALIDA";

/// Único punto que modifica stock: actualiza `stock_balances` y escribe el kardex
/// en la misma transacción del llamador. Bloquea stock negativo (RN-04).
/// Devuelve (saldo_antes, saldo_después).
pub async fn apply_movement(
    conn: &mut AnyConnection,
    product_id: &str,
    delta: i64,
    mtype: &str,
    origin_type: &str,
    origin_id: Option<&str>,
    note: Option<&str>,
) -> AppResult<(i64, i64)> {
    if delta == 0 {
        return Err(AppError::Validation("la cantidad no puede ser cero".into()));
    }
    // UPDATE condicional: atómico y portable (toma el lock de fila en ambos motores).
    let r = sqlx::query(
        "UPDATE stock_balances SET qty_base = qty_base + ? WHERE product_id = ? AND qty_base + ? >= 0",
    )
    .bind(delta)
    .bind(product_id)
    .bind(delta)
    .execute(&mut *conn)
    .await?;
    if r.rows_affected() == 0 {
        let cur: Option<i64> = sqlx::query_scalar("SELECT qty_base FROM stock_balances WHERE product_id = ?")
            .bind(product_id)
            .fetch_optional(&mut *conn)
            .await?;
        return Err(match cur {
            None => AppError::NotFound("producto".into()),
            Some(available) => AppError::InsufficientStock { available },
        });
    }
    let after: i64 = sqlx::query_scalar("SELECT qty_base FROM stock_balances WHERE product_id = ?")
        .bind(product_id)
        .fetch_one(&mut *conn)
        .await?;
    let before = after - delta;
    let seq = next_counter(&mut *conn, "stock_seq").await?;
    sqlx::query(
        "INSERT INTO stock_movements \
         (id, product_id, type, qty_base_signed, origin_type, origin_id, note, occurred_at, balance_before, balance_after, seq) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(product_id)
    .bind(mtype)
    .bind(delta)
    .bind(origin_type)
    .bind(origin_id)
    .bind(note)
    .bind(now())
    .bind(before)
    .bind(after)
    .bind(seq)
    .execute(&mut *conn)
    .await?;
    Ok((before, after))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AdjustKind {
    Entrada,
    Salida,
}

/// Ajuste manual con motivo obligatorio. Devuelve el nuevo saldo.
pub async fn adjust_stock(
    pool: &AnyPool,
    product_id: &str,
    kind: AdjustKind,
    qty: i64,
    reason: &str,
) -> AppResult<i64> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation("el motivo del ajuste es obligatorio".into()));
    }
    if qty <= 0 {
        return Err(AppError::Validation("la cantidad debe ser mayor a cero".into()));
    }
    let (delta, mtype) = match kind {
        AdjustKind::Entrada => (qty, AJUSTE_ENTRADA),
        AdjustKind::Salida => (-qty, AJUSTE_SALIDA),
    };
    let mut tx = pool.begin().await?;
    let (_, after) = apply_movement(&mut tx, product_id, delta, mtype, "AJUSTE", None, Some(reason)).await?;
    tx.commit().await?;
    Ok(after)
}

#[derive(Debug, Serialize)]
pub struct Movement {
    pub id: String,
    #[serde(rename = "type")]
    pub mtype: String,
    pub qty_base_signed: i64,
    pub origin_type: String,
    pub note: Option<String>,
    pub balance_before: Option<i64>,
    pub balance_after: Option<i64>,
    pub occurred_at: String,
}

/// Kardex de un producto, más reciente primero. `from`/`to` RFC3339 UTC (inclusive).
pub async fn kardex(
    pool: &AnyPool,
    product_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: i64,
) -> AppResult<Vec<Movement>> {
    let mut sql = String::from(
        "SELECT id, type, qty_base_signed, origin_type, note, balance_before, balance_after, occurred_at \
         FROM stock_movements WHERE product_id = ?",
    );
    if from.is_some() {
        sql.push_str(" AND occurred_at >= ?");
    }
    if to.is_some() {
        sql.push_str(" AND occurred_at <= ?");
    }
    sql.push_str(" ORDER BY occurred_at DESC, seq DESC LIMIT ?");
    // Solo se concatenan fragmentos estáticos; los valores van como parámetros.
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(product_id.to_string());
    if let Some(f) = from {
        q = q.bind(f.to_string());
    }
    if let Some(t) = to {
        q = q.bind(t.to_string());
    }
    let rows = q.bind(limit.clamp(1, 1000)).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| Movement {
            id: r.get("id"),
            mtype: r.get("type"),
            qty_base_signed: r.get("qty_base_signed"),
            origin_type: r.get("origin_type"),
            note: r.get("note"),
            balance_before: r.get("balance_before"),
            balance_after: r.get("balance_after"),
            occurred_at: r.get("occurred_at"),
        })
        .collect())
}

/// Productos donde `stock_balances` difiere de la suma del kardex. Debe ser vacío (DoD).
pub async fn stock_divergences(pool: &AnyPool) -> AppResult<Vec<(String, i64, i64)>> {
    let rows = sqlx::query(
        "SELECT b.product_id AS product_id, b.qty_base AS balance, \
         CAST(COALESCE((SELECT SUM(m.qty_base_signed) FROM stock_movements m WHERE m.product_id = b.product_id), 0) AS SIGNED) AS summed \
         FROM stock_balances b",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("product_id"), r.get::<i64, _>("balance"), r.get::<i64, _>("summed")))
        .filter(|(_, b, s)| b != s)
        .collect())
}
