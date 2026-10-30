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

// ---------- Movimientos manuales, cierre e historial (RF-05) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ManualKind {
    Ingreso,
    Egreso,
    Retiro,
}

/// Ingreso/egreso/retiro manual en la caja abierta. Solo el EFECTIVO altera el saldo físico;
/// un egreso o retiro en efectivo no puede dejar el esperado en negativo.
pub async fn add_manual_movement(
    pool: &AnyPool,
    kind: ManualKind,
    method: &str,
    amount_cents: i64,
    concept: &str,
) -> AppResult<()> {
    let concept = concept.trim();
    if concept.is_empty() {
        return Err(AppError::Validation("el concepto es obligatorio".into()));
    }
    if amount_cents <= 0 {
        return Err(AppError::Validation("el monto debe ser mayor a cero".into()));
    }
    if !["EFECTIVO", "YAPE", "PLIN", "OTRO"].contains(&method) {
        return Err(AppError::Validation(format!("método inválido: {method}")));
    }
    let mut tx = pool.begin().await?;
    let session = open_session_id(&mut tx)
        .await?
        .ok_or_else(|| AppError::Validation("no hay una caja abierta".into()))?;
    let (mtype, signed) = match kind {
        ManualKind::Ingreso => ("INGRESO", amount_cents),
        ManualKind::Egreso => ("EGRESO", -amount_cents),
        ManualKind::Retiro => ("RETIRO", -amount_cents),
    };
    if method == "EFECTIVO" && signed < 0 && expected_cash(&mut tx, &session).await? + signed < 0 {
        return Err(AppError::Validation("el efectivo en caja no alcanza para este egreso".into()));
    }
    record_movement(&mut tx, &session, mtype, method, signed, "MANUAL", None, concept).await?;
    tx.commit().await?;
    Ok(())
}

/// Cierra la sesión abierta con el efectivo contado. Queda inmutable (sin más movimientos).
pub async fn close_cash(pool: &AnyPool, counted_cents: i64, note: Option<&str>) -> AppResult<String> {
    if counted_cents < 0 {
        return Err(AppError::Validation("el efectivo contado no puede ser negativo".into()));
    }
    let mut tx = pool.begin().await?;
    let session = open_session_id(&mut tx)
        .await?
        .ok_or_else(|| AppError::Validation("no hay una caja abierta".into()))?;
    let expected = expected_cash(&mut tx, &session).await?;
    let r = sqlx::query(
        "UPDATE cash_sessions SET status = 'CLOSED', open_flag = NULL, closed_at = ?, counted_cash_cents = ?, \
         expected_cash_cents = ?, difference_cents = ?, note = ? WHERE id = ? AND status = 'OPEN'",
    )
    .bind(now())
    .bind(counted_cents)
    .bind(expected)
    .bind(counted_cents - expected)
    .bind(note.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string))
    .bind(&session)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() != 1 {
        return Err(AppError::Validation("la caja ya fue cerrada".into()));
    }
    tx.commit().await?;
    Ok(session)
}

#[derive(Debug, Serialize)]
pub struct SessionRow {
    pub id: String,
    pub opened_at: String,
    pub opening_cash_cents: i64,
    pub closed_at: Option<String>,
    pub counted_cash_cents: Option<i64>,
    pub expected_cash_cents: Option<i64>,
    pub difference_cents: Option<i64>,
    pub note: Option<String>,
    pub status: String,
}

fn session_row(r: &sqlx::any::AnyRow) -> SessionRow {
    SessionRow {
        id: r.get("id"),
        opened_at: r.get("opened_at"),
        opening_cash_cents: r.get("opening_cash_cents"),
        closed_at: r.get("closed_at"),
        counted_cash_cents: r.get("counted_cash_cents"),
        expected_cash_cents: r.get("expected_cash_cents"),
        difference_cents: r.get("difference_cents"),
        note: r.get("note"),
        status: r.get("status"),
    }
}

const SESSION_COLS: &str = "id, opened_at, opening_cash_cents, closed_at, counted_cash_cents, expected_cash_cents, difference_cents, note, status";

pub async fn list_sessions(pool: &AnyPool) -> AppResult<Vec<SessionRow>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {SESSION_COLS} FROM cash_sessions ORDER BY opened_at DESC LIMIT 200"
    )))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(session_row).collect())
}

#[derive(Debug, Serialize)]
pub struct CashMovementRow {
    pub id: String,
    #[serde(rename = "type")]
    pub mtype: String,
    pub method: String,
    pub amount_cents_signed: i64,
    pub concept: String,
    pub occurred_at: String,
}

#[derive(Debug, Serialize)]
pub struct TypeTotal {
    pub key: String,
    pub total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct SessionDetail {
    pub session: SessionRow,
    /// Saldo esperado vigente (abierta) o el congelado al cierre.
    pub expected_cash_cents: i64,
    pub movements: Vec<CashMovementRow>,
    /// Efectivo por tipo de movimiento (VENTA, ANULACION_VENTA, INGRESO, EGRESO, RETIRO).
    pub cash_by_type: Vec<TypeTotal>,
    /// Cobros de ventas confirmadas dentro de la sesión, por método (incluye Yape/Plin que no tocan la caja).
    pub sales_by_method: Vec<TypeTotal>,
}

pub async fn session_detail(pool: &AnyPool, id: &str) -> AppResult<SessionDetail> {
    let mut conn = pool.acquire().await?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {SESSION_COLS} FROM cash_sessions WHERE id = ?")))
        .bind(id.to_string())
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::NotFound("sesión de caja".into()))?;
    let session = session_row(&row);
    let expected = match session.expected_cash_cents {
        Some(e) if session.status == "CLOSED" => e,
        _ => expected_cash(&mut conn, id).await?,
    };
    let movements = sqlx::query(
        "SELECT id, type, method, amount_cents_signed, concept, occurred_at FROM cash_movements \
         WHERE cash_session_id = ? ORDER BY occurred_at, id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| CashMovementRow {
        id: r.get("id"),
        mtype: r.get("type"),
        method: r.get("method"),
        amount_cents_signed: r.get("amount_cents_signed"),
        concept: r.get("concept"),
        occurred_at: r.get("occurred_at"),
    })
    .collect();
    let cash_by_type = sqlx::query(
        "SELECT type AS k, CAST(SUM(amount_cents_signed) AS SIGNED) AS t FROM cash_movements \
         WHERE cash_session_id = ? AND method = 'EFECTIVO' GROUP BY type ORDER BY type",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| TypeTotal { key: r.get("k"), total_cents: r.get("t") })
    .collect();
    let upper = session.closed_at.clone().unwrap_or_else(now);
    let sales_by_method = sqlx::query(
        "SELECT p.method AS k, CAST(SUM(p.amount_cents) AS SIGNED) AS t FROM sale_payments p \
         JOIN sales s ON s.id = p.sale_id WHERE s.status = 'CONFIRMED' AND s.sold_at >= ? AND s.sold_at <= ? \
         GROUP BY p.method ORDER BY p.method",
    )
    .bind(session.opened_at.clone())
    .bind(upper)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| TypeTotal { key: r.get("k"), total_cents: r.get("t") })
    .collect();
    Ok(SessionDetail { session, expected_cash_cents: expected, movements, cash_by_type, sales_by_method })
}
