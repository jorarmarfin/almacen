use std::path::Path;
use std::sync::Once;

use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::{AnyPool, Executor};

use crate::config::{DbConfig, Engine};
use crate::error::{AppError, AppResult};

pub const SCHEMA_VERSION: i64 = 1;

static DRIVERS: Once = Once::new();

#[derive(Clone)]
pub struct Db {
    pub pool: AnyPool,
    pub engine: Engine,
}

fn connect_url(cfg: &DbConfig, data_dir: &Path) -> AppResult<String> {
    match cfg.engine {
        Engine::Sqlite => {
            let p = DbConfig::sqlite_path(data_dir);
            Ok(format!("sqlite://{}?mode=rwc", p.display()))
        }
        Engine::Mysql => cfg
            .mysql
            .as_ref()
            .map(|m| m.url())
            .ok_or_else(|| AppError::Validation("falta configuración de MySQL".into())),
    }
}

impl Db {
    /// Abre el pool (sin migrar). SQLite: WAL + foreign_keys=ON.
    pub async fn connect_url(url: &str, engine: Engine, max_conns: u32) -> AppResult<Self> {
        DRIVERS.call_once(sqlx::any::install_default_drivers);
        let opts: AnyConnectOptions = url.parse()?;
        let pool = AnyPoolOptions::new()
            .max_connections(max_conns)
            .after_connect(move |conn, _| {
                Box::pin(async move {
                    if engine == Engine::Sqlite {
                        conn.execute("PRAGMA foreign_keys = ON").await?;
                        conn.execute("PRAGMA journal_mode = WAL").await?;
                    }
                    Ok(())
                })
            })
            .connect_with(opts)
            .await?;
        Ok(Self { pool, engine })
    }

    pub async fn open(cfg: &DbConfig, data_dir: &Path) -> AppResult<Self> {
        if cfg.engine == Engine::Sqlite {
            std::fs::create_dir_all(data_dir)?;
        }
        let url = connect_url(cfg, data_dir)?;
        let max = if cfg.engine == Engine::Sqlite { 1 } else { 5 };
        let db = Self::connect_url(&url, cfg.engine, max).await?;
        db.migrate().await?;
        Ok(db)
    }

    pub async fn migrate(&self) -> AppResult<()> {
        match self.engine {
            Engine::Sqlite => sqlx::migrate!("./migrations/sqlite").run(&self.pool).await?,
            Engine::Mysql => sqlx::migrate!("./migrations/mysql").run(&self.pool).await?,
        }
        Ok(())
    }
}

/// Prueba de conexión sin migrar ni persistir nada.
pub async fn test_connection(cfg: &DbConfig, data_dir: &Path) -> AppResult<()> {
    let url = connect_url(cfg, data_dir)?;
    let db = Db::connect_url(&url, cfg.engine, 1).await?;
    sqlx::query("SELECT 1").execute(&db.pool).await?;
    db.pool.close().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sqlite_mem() -> Db {
        let db = Db::connect_url("sqlite::memory:", Engine::Sqlite, 1).await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    /// MySQL real solo si `MYSQL_TEST_URL` está definida (CI). Limpia el esquema antes.
    async fn mysql_env() -> Option<Db> {
        let url = std::env::var("MYSQL_TEST_URL").ok()?;
        let db = Db::connect_url(&url, Engine::Mysql, 2).await.unwrap();
        sqlx::query("SET FOREIGN_KEY_CHECKS = 0").execute(&db.pool).await.unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE()",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        for t in tables {
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS `{t}`"))).execute(&db.pool).await.unwrap();
        }
        sqlx::query("SET FOREIGN_KEY_CHECKS = 1").execute(&db.pool).await.unwrap();
        db.migrate().await.unwrap();
        Some(db)
    }

    async fn check_fk_enforced(db: &Db) {
        let r = sqlx::query(
            "INSERT INTO product_presentations (id, product_id, name, factor_to_base, sale_price_cents) \
             VALUES ('p1', 'no-existe', 'unidad', 1, 100)",
        )
        .execute(&db.pool)
        .await;
        assert!(r.is_err(), "FK debe rechazar producto inexistente");
    }

    async fn check_rollback(db: &Db) {
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("INSERT INTO categories (id, name, active) VALUES ('c1', 'Bebidas', 1)")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    async fn check_single_open_cash(db: &Db) {
        let ins = "INSERT INTO cash_sessions (id, opened_at, opening_cash_cents, status, open_flag) \
                   VALUES (?, '2026-10-30T00:00:00.000Z', 0, 'OPEN', 1)";
        sqlx::query(ins).bind("a").execute(&db.pool).await.unwrap();
        assert!(sqlx::query(ins).bind("b").execute(&db.pool).await.is_err(), "RN-07");
        sqlx::query("UPDATE cash_sessions SET status='CLOSED', open_flag=NULL WHERE id='a'")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query(ins).bind("c").execute(&db.pool).await.unwrap();
    }

    async fn check_all(db: &Db) {
        check_fk_enforced(db).await;
        check_rollback(db).await;
        check_single_open_cash(db).await;
    }

    #[tokio::test]
    async fn sqlite_schema_and_rules() {
        check_all(&sqlite_mem().await).await;
    }

    #[tokio::test]
    async fn mysql_schema_and_rules() {
        match mysql_env().await {
            Some(db) => check_all(&db).await,
            None => eprintln!("MYSQL_TEST_URL no definida: se omite prueba MySQL"),
        }
    }
}
