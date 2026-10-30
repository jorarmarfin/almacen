use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::config::{DbConfig, Engine};
use crate::catalog::{self, Category, ImportReport, Product, ProductFilter, ProductInput};
use crate::inventory::{self, AdjustKind, Movement};
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

#[tauri::command]
pub async fn list_categories(state: State<'_, AppState>) -> AppResult<Vec<Category>> {
    catalog::list_categories(&state.db().await?.pool).await
}

#[tauri::command]
pub async fn save_category(state: State<'_, AppState>, id: Option<String>, name: String, active: bool) -> AppResult<String> {
    catalog::save_category(&state.db().await?.pool, id.as_deref(), &name, active).await
}

#[tauri::command]
pub async fn delete_category(state: State<'_, AppState>, id: String) -> AppResult<()> {
    catalog::delete_category(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn list_products(state: State<'_, AppState>, filter: ProductFilter) -> AppResult<Vec<Product>> {
    catalog::list_products(&state.db().await?.pool, &filter).await
}

#[tauri::command]
pub async fn save_product(state: State<'_, AppState>, input: ProductInput) -> AppResult<String> {
    catalog::save_product(&state.db().await?.pool, &input).await
}

#[tauri::command]
pub async fn set_product_active(state: State<'_, AppState>, id: String, active: bool) -> AppResult<()> {
    catalog::set_product_active(&state.db().await?.pool, &id, active).await
}

#[tauri::command]
pub async fn import_products_csv(state: State<'_, AppState>, content: String, dry_run: bool) -> AppResult<ImportReport> {
    catalog::import_csv(&state.db().await?.pool, &content, dry_run).await
}

#[tauri::command]
pub fn get_products_template() -> String {
    catalog::products_template()
}

/// Escribe un archivo de texto elegido por el usuario en un diálogo "Guardar como".
#[tauri::command]
pub fn write_text_file(path: String, content: String) -> AppResult<()> {
    std::fs::write(path, content)?;
    Ok(())
}

#[tauri::command]
pub async fn adjust_stock(
    state: State<'_, AppState>,
    product_id: String,
    kind: AdjustKind,
    qty: i64,
    reason: String,
) -> AppResult<i64> {
    inventory::adjust_stock(&state.db().await?.pool, &product_id, kind, qty, &reason).await
}

#[tauri::command]
pub async fn get_kardex(
    state: State<'_, AppState>,
    product_id: String,
    from: Option<String>,
    to: Option<String>,
) -> AppResult<Vec<Movement>> {
    inventory::kardex(&state.db().await?.pool, &product_id, from.as_deref(), to.as_deref(), 500).await
}

/// Devuelve productos donde saldo != suma del kardex (debe ser vacío).
#[tauri::command]
pub async fn check_stock_integrity(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    Ok(inventory::stock_divergences(&state.db().await?.pool).await?.into_iter().map(|d| d.0).collect())
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
