use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::config::{DbConfig, Engine};
use crate::catalog::{self, Category, ImportReport, Product, ProductFilter, ProductInput};
use crate::inventory::{self, AdjustKind, Movement};
use crate::purchases::{self, PurchaseDetail, PurchaseInput, PurchaseSummary, Supplier, SupplierInput};
use crate::backup::{self, Manifest, RestoreReport};
use crate::cash::{self, CashSession, ManualKind, SessionDetail, SessionRow};
use crate::reports::{self, Dashboard, SalesReport, TopMetric, TopProduct};
use crate::sales::{self, SaleDetail, SaleInput, SaleSummary};
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
    let mut config = config;
    // La carpeta de respaldo automático no viene del formulario de conexión: se conserva.
    config.auto_backup_dir = state.config.read().await.auto_backup_dir.clone();
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

#[tauri::command]
pub async fn list_suppliers(state: State<'_, AppState>) -> AppResult<Vec<Supplier>> {
    purchases::list_suppliers(&state.db().await?.pool).await
}

#[tauri::command]
pub async fn save_supplier(state: State<'_, AppState>, input: SupplierInput) -> AppResult<String> {
    purchases::save_supplier(&state.db().await?.pool, &input).await
}

#[tauri::command]
pub async fn delete_supplier(state: State<'_, AppState>, id: String) -> AppResult<()> {
    purchases::delete_supplier(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn list_purchases(state: State<'_, AppState>, status: Option<String>) -> AppResult<Vec<PurchaseSummary>> {
    purchases::list_purchases(&state.db().await?.pool, status.as_deref()).await
}

#[tauri::command]
pub async fn get_purchase(state: State<'_, AppState>, id: String) -> AppResult<PurchaseDetail> {
    purchases::get_purchase(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn save_purchase(state: State<'_, AppState>, input: PurchaseInput) -> AppResult<String> {
    purchases::save_purchase(&state.db().await?.pool, &input).await
}

#[tauri::command]
pub async fn confirm_purchase(state: State<'_, AppState>, id: String) -> AppResult<()> {
    purchases::confirm_purchase(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn cancel_purchase_draft(state: State<'_, AppState>, id: String) -> AppResult<()> {
    purchases::cancel_draft(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn current_cash(state: State<'_, AppState>) -> AppResult<Option<CashSession>> {
    cash::current_session(&state.db().await?.pool).await
}

#[tauri::command]
pub async fn open_cash(state: State<'_, AppState>, opening_cash_cents: i64) -> AppResult<String> {
    cash::open_cash(&state.db().await?.pool, opening_cash_cents).await
}

#[tauri::command]
pub async fn confirm_sale(state: State<'_, AppState>, input: SaleInput) -> AppResult<SaleDetail> {
    sales::confirm_sale(&state.db().await?.pool, &input).await
}

#[tauri::command]
pub async fn cancel_sale(state: State<'_, AppState>, id: String, reason: String) -> AppResult<SaleDetail> {
    sales::cancel_sale(&state.db().await?.pool, &id, &reason).await
}

#[tauri::command]
pub async fn list_sales(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    status: Option<String>,
) -> AppResult<Vec<SaleSummary>> {
    sales::list_sales(&state.db().await?.pool, from.as_deref(), to.as_deref(), status.as_deref()).await
}

#[tauri::command]
pub async fn get_sale(state: State<'_, AppState>, id: String) -> AppResult<SaleDetail> {
    sales::get_sale(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn add_cash_movement(
    state: State<'_, AppState>,
    kind: ManualKind,
    method: String,
    amount_cents: i64,
    concept: String,
) -> AppResult<()> {
    cash::add_manual_movement(&state.db().await?.pool, kind, &method, amount_cents, &concept).await
}

#[derive(Serialize)]
pub struct CloseResult {
    pub session_id: String,
    pub auto_backup: Option<String>,
    /// Un fallo del respaldo automático NO revierte el cierre de caja: se informa aparte.
    pub auto_backup_error: Option<String>,
}

#[tauri::command]
pub async fn close_cash(state: State<'_, AppState>, counted_cents: i64, note: Option<String>) -> AppResult<CloseResult> {
    let db = state.db().await?;
    let session_id = cash::close_cash(&db.pool, counted_cents, note.as_deref()).await?;
    let dir = state.config.read().await.auto_backup_dir.clone();
    let (auto_backup, auto_backup_error) = match dir {
        None => (None, None),
        Some(d) => match backup::auto_backup(&db.pool, db.engine, std::path::Path::new(&d)).await {
            Ok(p) => (Some(p.display().to_string()), None),
            Err(e) => (None, Some(e.to_string())),
        },
    };
    Ok(CloseResult { session_id, auto_backup, auto_backup_error })
}

#[tauri::command]
pub async fn get_auto_backup_dir(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state.config.read().await.auto_backup_dir.clone())
}

/// `None` desactiva el respaldo automático. Se prueba que la carpeta sea escribible.
#[tauri::command]
pub async fn set_auto_backup_dir(state: State<'_, AppState>, dir: Option<String>) -> AppResult<()> {
    if let Some(d) = &dir {
        std::fs::create_dir_all(d)?;
        let probe = std::path::Path::new(d).join(".mibodega-write-test");
        std::fs::write(&probe, b"ok").map_err(|_| AppError::Validation("no se puede escribir en esa carpeta".into()))?;
        let _ = std::fs::remove_file(probe);
    }
    let mut cfg = state.config.write().await;
    cfg.auto_backup_dir = dir;
    cfg.save(&state.config_path)?;
    Ok(())
}

#[tauri::command]
pub async fn list_cash_sessions(state: State<'_, AppState>) -> AppResult<Vec<SessionRow>> {
    cash::list_sessions(&state.db().await?.pool).await
}

#[tauri::command]
pub async fn get_cash_session(state: State<'_, AppState>, id: String) -> AppResult<SessionDetail> {
    cash::session_detail(&state.db().await?.pool, &id).await
}

#[tauri::command]
pub async fn dashboard(state: State<'_, AppState>, from: String, to: String) -> AppResult<Dashboard> {
    reports::dashboard(&state.db().await?.pool, &from, &to).await
}

#[tauri::command]
pub async fn sales_report(state: State<'_, AppState>, from: String, to: String) -> AppResult<SalesReport> {
    reports::sales_report(&state.db().await?.pool, &from, &to).await
}

#[tauri::command]
pub async fn top_products(
    state: State<'_, AppState>,
    from: String,
    to: String,
    metric: TopMetric,
    limit: i64,
) -> AppResult<Vec<TopProduct>> {
    reports::top_products(&state.db().await?.pool, &from, &to, metric, limit).await
}

/// `kind`: products | kardex | sales. Devuelve el CSV como texto para guardarlo con el diálogo nativo.
#[tauri::command]
pub async fn export_csv(state: State<'_, AppState>, kind: String, from: String, to: String) -> AppResult<String> {
    let pool = state.db().await?.pool;
    match kind.as_str() {
        "products" => reports::export_products_csv(&pool).await,
        "kardex" => reports::export_kardex_csv(&pool, &from, &to).await,
        "sales" => reports::export_sales_csv(&pool, &from, &to).await,
        other => Err(AppError::Validation(format!("exportación desconocida: {other}"))),
    }
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub engine: Engine,
    pub last_backup_at: Option<String>,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let engine = state.config.read().await.engine;
    let last = match state.db.read().await.clone() {
        Some(db) => sqlx::query_scalar::<_, Option<String>>("SELECT backup_info FROM app_metadata WHERE id = 1")
            .fetch_optional(&db.pool)
            .await?
            .flatten()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["last_export_at"].as_str().map(str::to_string)),
        None => None,
    };
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        engine,
        last_backup_at: last,
    })
}

#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>, path: String, password: Option<String>) -> AppResult<Manifest> {
    let db = state.db().await?;
    backup::export_backup_pw(&db.pool, db.engine, std::path::Path::new(&path), password.as_deref()).await
}

#[tauri::command]
pub async fn export_sqlite_native(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let db = state.db().await?;
    backup::export_sqlite_native(&db.pool, db.engine, std::path::Path::new(&path)).await
}

/// Valida un respaldo y devuelve su manifiesto sin modificar nada.
#[tauri::command]
pub fn inspect_backup(path: String, password: Option<String>) -> AppResult<Manifest> {
    backup::inspect_backup_pw(std::path::Path::new(&path), password.as_deref())
}

#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, path: String, password: Option<String>) -> AppResult<RestoreReport> {
    let db = state.db().await?;
    let pre_dir = state.data_dir.join("backups");
    backup::restore_backup_pw(&db.pool, db.engine, std::path::Path::new(&path), &pre_dir, password.as_deref()).await
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
