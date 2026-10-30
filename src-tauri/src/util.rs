use sqlx::AnyConnection;

use crate::error::AppResult;

/// Timestamp UTC RFC3339 con milisegundos; orden lexicográfico = cronológico.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Siguiente valor de un contador transaccional (numeración humana, SKU auto).
/// Debe llamarse dentro de una transacción.
pub async fn next_counter(conn: &mut AnyConnection, name: &str) -> AppResult<i64> {
    let r = sqlx::query("UPDATE counters SET value = value + 1 WHERE name = ?")
        .bind(name)
        .execute(&mut *conn)
        .await?;
    if r.rows_affected() == 0 {
        sqlx::query("INSERT INTO counters (name, value) VALUES (?, 1)")
            .bind(name)
            .execute(&mut *conn)
            .await?;
        return Ok(1);
    }
    let v: i64 = sqlx::query_scalar("SELECT value FROM counters WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *conn)
        .await?;
    Ok(v)
}
