use serde::Serialize;
use sqlx::{AnyConnection, AnyPool, Row};

use crate::error::{AppError, AppResult};
use crate::util::{new_id, now};

#[derive(Debug, Serialize)]
pub struct CashSession {
    pub id: String,
    pub opened_at: String,
    pub opening_cash_cents: i64,
    pub status: String,
    /// apertura + movimientos de efectivo (ventas, ingresos, egresos) — RF-05.
    pub expected_cash_cents: i64,
}

/// Saldo físico esperado de una sesión: apertura + suma firmada de movimientos en efectivo.
pub async fn expected_cash(conn: &mut AnyConnection, session_id: &str) -> AppResult<i64> {
    let opening: i64 = sqlx::query_scalar("SELECT opening_cash_cents FROM cash_sessions WHERE id = ?")
        .bind(session_id)
        .fetch_one(&mut *conn)
        .await?;
    let moved: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(amount_cents_signed), 0) AS SIGNED) FROM cash_movements \
         WHERE cash_session_id = ? AND method = 'EFECTIVO'",
    )
    .bind(session_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(opening + moved)
}

/// Sesión abierta actual (RN-07: a lo sumo una).
pub async fn open_session_id(conn: &mut AnyConnection) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT id FROM cash_sessions WHERE status = 'OPEN'")
        .fetch_optional(&mut *conn)
        .await?)
}

pub async fn current_session(pool: &AnyPool) -> AppResult<Option<CashSession>> {
    let mut conn = pool.acquire().await?;
    let row = sqlx::query("SELECT id, opened_at, opening_cash_cents, status FROM cash_sessions WHERE status = 'OPEN'")
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        None => Ok(None),
        Some(r) => {
            let id: String = r.get("id");
            let expected = expected_cash(&mut conn, &id).await?;
            Ok(Some(CashSession {
                id,
                opened_at: r.get("opened_at"),
                opening_cash_cents: r.get("opening_cash_cents"),
                status: r.get("status"),
                expected_cash_cents: expected,
            }))
        }
    }
}

pub async fn open_cash(pool: &AnyPool, opening_cash_cents: i64) -> AppResult<String> {
    if opening_cash_cents < 0 {
        return Err(AppError::Validation("el saldo inicial no puede ser negativo".into()));
    }
    let mut tx = pool.begin().await?;
    if open_session_id(&mut tx).await?.is_some() {
        return Err(AppError::Validation("ya hay una caja abierta".into()));
    }
    let id = new_id();
    // open_flag UNIQUE respalda la regla aun ante condiciones de carrera (RN-07).
    sqlx::query(
        "INSERT INTO cash_sessions (id, opened_at, opening_cash_cents, status, open_flag) VALUES (?, ?, ?, 'OPEN', 1)",
    )
    .bind(&id)
    .bind(now())
    .bind(opening_cash_cents)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Validation("ya hay una caja abierta".into()))?;
    tx.commit().await?;
    Ok(id)
}

/// Movimiento de efectivo firmado dentro de la transacción del llamador.
#[allow(clippy::too_many_arguments)]
pub async fn record_movement(
    conn: &mut AnyConnection,
    session_id: &str,
    mtype: &str,
    method: &str,
    signed_cents: i64,
    source_type: &str,
    source_id: Option<&str>,
    concept: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO cash_movements (id, cash_session_id, type, method, amount_cents_signed, source_type, source_id, concept, occurred_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(session_id)
    .bind(mtype)
    .bind(method)
    .bind(signed_cents)
    .bind(source_type)
    .bind(source_id)
    .bind(concept)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}
