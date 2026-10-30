use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::config::{DbConfig, Engine};
use crate::db::{test_connection, Db, SCHEMA_VERSION};
use crate::error::{AppError, AppResult};

pub struct AppState {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub config: RwLock<DbConfig>,
    pub db: RwLock<Option<Db>>,
}

impl AppState {
    async fn db(&self) -> AppResult<Db> {
        self.db.read().await.clone().ok_or(AppError::NotConfigured)
    }
}

#[derive(Serialize)]
pub struct Settings {
    pub business_name: String,
    pub currency: String,
    pub default_min_stock: i64,
    pub schema_version: i64,
}

#[derive(Serialize)]
pub struct BootState {
    pub engine: Engine,
    pub db_ready: bool,
    pub initialized: bool,
    pub settings: Option<Settings>,
}

async fn read_settings(db: &Db) -> AppResult<Option<Settings>> {
    let row = sqlx::query(
        "SELECT business_name, currency, default_min_stock, schema_version \
         FROM business_settings WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|r| Settings {
        business_name: r.get("business_name"),
        currency: r.get("currency"),
        default_min_stock: r.get("default_min_stock"),
        schema_version: r.get("schema_version"),
    }))
}

#[tauri::command]
pub async fn get_boot_state(state: State<'_, AppState>) -> AppResult<BootState> {
    let engine = state.config.read().await.engine;
    let db = state.db.read().await.clone();
    let settings = match &db {
        Some(db) => read_settings(db).await?,
        None => None,
    };
    Ok(BootState {
        engine,
        db_ready: db.is_some(),
        initialized: settings.is_some(),
        settings,
    })
}

#[tauri::command]
pub async fn test_db_connection(state: State<'_, AppState>, config: DbConfig) -> AppResult<()> {
    test_connection(&config, &state.data_dir).await
}

/// Guarda la configuración de motor, abre la conexión y migra.
#[tauri::command]
pub async fn set_db_config(state: State<'_, AppState>, config: DbConfig) -> AppResult<()> {
    let db = Db::open(&config, &state.data_dir).await?;
    config.save(&state.config_path)?;
    if let Some(old) = state.db.write().await.replace(db) {
        old.pool.close().await;
    }
    *state.config.write().await = config;
    Ok(())
}

#[derive(Deserialize)]
pub struct InitBusiness {
    pub business_name: String,
    pub default_min_stock: i64,
}

#[tauri::command]
pub async fn init_business(state: State<'_, AppState>, input: InitBusiness) -> AppResult<Settings> {
    let name = input.business_name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("el nombre del negocio es obligatorio".into()));
    }
    if input.default_min_stock < 0 {
        return Err(AppError::Validation("el stock mínimo no puede ser negativo".into()));
    }
    let db = state.db().await?;
    let mut tx = db.pool.begin().await?;
    if sqlx::query("SELECT id FROM business_settings WHERE id = 1")
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
    {
        return Err(AppError::AlreadyInitialized);
    }
    sqlx::query(
        "INSERT INTO business_settings (id, business_name, currency, default_min_stock, schema_version) \
         VALUES (1, ?, 'PEN', ?, ?)",
    )
    .bind(&name)
    .bind(input.default_min_stock)
    .bind(SCHEMA_VERSION)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO app_metadata (id, installation_id, schema_version, updated_at) VALUES (1, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(SCHEMA_VERSION)
    .bind(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Settings {
        business_name: name,
        currency: "PEN".into(),
        default_min_stock: input.default_min_stock,
        schema_version: SCHEMA_VERSION,
    })
}

/// Arranque: carga config y abre la BD si ya existe configuración previa.
pub async fn bootstrap(app: &AppHandle) -> AppResult<AppState> {
    let data_dir = app.path().app_data_dir().map_err(|e| AppError::Validation(e.to_string()))?;
    let config_path = data_dir.join("config.json");
    let config = DbConfig::load(&config_path)?;
    // SQLite por defecto abre sola; MySQL solo si ya fue configurado explícitamente.
    let db = match Db::open(&config, &data_dir).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("no se pudo abrir la BD al arrancar: {e}");
            None
        }
    };
    Ok(AppState {
        data_dir,
        config_path,
        config: RwLock::new(config),
        db: RwLock::new(db),
    })
}
