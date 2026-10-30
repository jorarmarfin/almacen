//! Respaldo/restauración agnóstico de motor (SQLite y MySQL).
//! Formato `.mibodega-backup`: zip con `manifest.json` y `data/<tabla>.ndjson` (una fila JSON por línea).
//! Sirve para restaurar en el mismo motor o migrar de uno a otro.
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, AnyPool, Row};

use crate::config::Engine;
use crate::db::SCHEMA_VERSION;
use crate::error::{AppError, AppResult};
use crate::inventory::stock_divergences_conn;
use crate::util::now;

pub const FORMAT: &str = "mibodega-backup";
pub const FORMAT_VERSION: i64 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum K {
    I,
    T,
}
use K::{I, T};

/// Tablas en orden de dependencia (padres primero). Debe cubrir todas las columnas del esquema:
/// lo verifica un test contra la BD real de cada motor.
pub const TABLES: &[(&str, &[(&str, K)])] = &[
    ("business_settings", &[("id", I), ("business_name", T), ("currency", T), ("default_min_stock", I), ("schema_version", I)]),
    ("app_metadata", &[("id", I), ("installation_id", T), ("schema_version", I), ("backup_info", T), ("updated_at", T)]),
    ("counters", &[("name", T), ("value", I)]),
    ("categories", &[("id", T), ("name", T), ("active", I)]),
    (
        "products",
        &[
            ("id", T), ("category_id", T), ("sku", T), ("barcode", T), ("name", T), ("base_unit", T),
            ("reference_cost_cents", I), ("min_stock", I), ("active", I), ("created_at", T), ("updated_at", T),
        ],
    ),
    (
        "product_presentations",
        &[("id", T), ("product_id", T), ("name", T), ("factor_to_base", I), ("sale_price_cents", I), ("active", I)],
    ),
    ("suppliers", &[("id", T), ("name", T), ("tax_id", T), ("phone", T), ("notes", T), ("active", I)]),
    (
        "purchases",
        &[("id", T), ("number", T), ("supplier_id", T), ("status", T), ("purchased_at", T), ("total_cents", I), ("created_at", T)],
    ),
    (
        "purchase_items",
        &[
            ("id", T), ("purchase_id", T), ("product_id", T), ("presentation_id", T), ("qty_presentations", I),
            ("factor_snapshot", I), ("unit_cost_cents", I), ("total_cents", I),
        ],
    ),
    (
        "sales",
        &[
            ("id", T), ("number", T), ("client_request_id", T), ("status", T), ("sold_at", T), ("subtotal_cents", I),
            ("discount_cents", I), ("total_cents", I), ("canceled_at", T), ("cancel_reason", T),
        ],
    ),
    (
        "sale_items",
        &[
            ("id", T), ("sale_id", T), ("product_id", T), ("presentation_id", T), ("qty_presentations", I),
            ("factor_snapshot", I), ("unit_price_cents", I), ("discount_cents", I), ("line_total_cents", I),
            ("cost_snapshot_cents_per_base_unit", I),
        ],
    ),
    (
        "sale_payments",
        &[("id", T), ("sale_id", T), ("method", T), ("amount_cents", I), ("received_cents", I), ("change_cents", I), ("reference", T)],
    ),
    (
        "stock_movements",
        &[
            ("id", T), ("product_id", T), ("type", T), ("qty_base_signed", I), ("origin_type", T), ("origin_id", T),
            ("note", T), ("occurred_at", T), ("balance_before", I), ("balance_after", I), ("seq", I),
        ],
    ),
    ("stock_balances", &[("product_id", T), ("qty_base", I)]),
    (
        "cash_sessions",
        &[
            ("id", T), ("opened_at", T), ("opening_cash_cents", I), ("closed_at", T), ("counted_cash_cents", I),
            ("expected_cash_cents", I), ("difference_cents", I), ("note", T), ("status", T), ("open_flag", I),
        ],
    ),
    (
        "cash_movements",
        &[
            ("id", T), ("cash_session_id", T), ("type", T), ("method", T), ("amount_cents_signed", I),
            ("source_type", T), ("source_id", T), ("concept", T), ("occurred_at", T),
        ],
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub rows: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: i64,
    pub schema_version: i64,
    pub created_at: String,
    pub installation_id: Option<String>,
    pub business_name: Option<String>,
    pub engine: String,
    pub app_version: String,
    pub tables: std::collections::BTreeMap<String, TableInfo>,
}

fn sha_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError::Validation(msg.into())
}

fn engine_name(e: Engine) -> &'static str {
    match e {
        Engine::Sqlite => "sqlite",
        Engine::Mysql => "mysql",
    }
}

/// Serializa una tabla a NDJSON (orden determinista por la primera columna).
async fn dump_table(conn: &mut AnyConnection, table: &str, cols: &[(&str, K)]) -> AppResult<(Vec<u8>, i64)> {
    let list = cols.iter().map(|c| c.0).collect::<Vec<_>>().join(", ");
    // Nombres de tabla/columna son constantes del código (TABLES), no entrada del usuario.
    let sql = format!("SELECT {list} FROM {table} ORDER BY {}", cols[0].0);
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(&mut *conn).await?;
    let mut out = Vec::new();
    for r in &rows {
        let mut obj = Map::new();
        for (name, kind) in cols {
            let v = match kind {
                I => r.try_get::<Option<i64>, _>(*name)?.map(Value::from),
                T => r.try_get::<Option<String>, _>(*name)?.map(Value::from),
            };
            obj.insert((*name).to_string(), v.unwrap_or(Value::Null));
        }
        serde_json::to_writer(&mut out, &Value::Object(obj))?;
        out.push(b'\n');
    }
    Ok((out, rows.len() as i64))
}

/// Exporta un respaldo completo y consistente (una sola transacción de lectura = snapshot).
pub async fn export_backup(pool: &AnyPool, engine: Engine, path: &Path) -> AppResult<Manifest> {
    export_backup_pw(pool, engine, path, None).await
}

/// Igual que `export_backup`; con `password` cifra TODO el archivo (zip AES-256, legible con 7-Zip).
pub async fn export_backup_pw(pool: &AnyPool, engine: Engine, path: &Path, password: Option<&str>) -> AppResult<Manifest> {
    let password = password.filter(|p| !p.is_empty());
    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut tables = std::collections::BTreeMap::new();
    for (table, cols) in TABLES {
        let (bytes, rows) = dump_table(conn, table, cols).await?;
        tables.insert((*table).to_string(), TableInfo { rows, sha256: sha_hex(&bytes) });
        files.push((format!("data/{table}.ndjson"), bytes));
    }
    let meta = sqlx::query("SELECT installation_id FROM app_metadata WHERE id = 1").fetch_optional(&mut *conn).await?;
    let biz: Option<String> =
        sqlx::query_scalar("SELECT business_name FROM business_settings WHERE id = 1").fetch_optional(&mut *conn).await?;
    tx.rollback().await?; // solo lectura
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        schema_version: SCHEMA_VERSION,
        created_at: now(),
        installation_id: meta.map(|r| r.get::<String, _>("installation_id")),
        business_name: biz,
        engine: engine_name(engine).into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        tables,
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Se escribe a un temporal y se renombra: nunca queda un respaldo a medias con el nombre final.
    let tmp = path.with_extension("tmp");
    {
        let file = std::fs::File::create(&tmp)?;
        let mut zip = zip::ZipWriter::new(file);
        let mut opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if let Some(pw) = password {
            opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
        }
        zip.start_file("manifest.json", opts).map_err(|e| bad(e.to_string()))?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        for (name, bytes) in &files {
            zip.start_file(name.as_str(), opts).map_err(|e| bad(e.to_string()))?;
            zip.write_all(bytes)?;
        }
        zip.finish().map_err(|e| bad(e.to_string()))?;
    }
    std::fs::rename(&tmp, path)?;

    // Registro informativo del último respaldo (después del snapshot, no forma parte de él).
    sqlx::query("UPDATE app_metadata SET backup_info = ? WHERE id = 1")
        .bind(json!({ "last_export_at": manifest.created_at }).to_string())
        .execute(pool)
        .await?;
    Ok(manifest)
}

/// Respaldo leído y verificado en memoria (aún sin tocar la BD).
pub struct LoadedBackup {
    pub manifest: Manifest,
    pub data: Vec<(&'static str, &'static [(&'static str, K)], Vec<Map<String, Value>>)>,
}

/// Valida formato, versión de esquema, tablas y sumas SHA-256 antes de cualquier cambio (RN-12).
pub fn load_backup(path: &Path) -> AppResult<LoadedBackup> {
    load_backup_pw(path, None)
}

/// Lee una entrada del zip, descifrándola si hace falta. Distingue "falta contraseña" de "contraseña incorrecta".
fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str, password: Option<&str>) -> AppResult<Vec<u8>> {
    use zip::result::ZipError;
    let mut bytes = Vec::new();
    let res = match password {
        Some(pw) => zip.by_name_decrypt(name, pw.as_bytes()).map(|mut f| f.read_to_end(&mut bytes).map(|_| ())),
        None => zip.by_name(name).map(|mut f| f.read_to_end(&mut bytes).map(|_| ())),
    };
    match res {
        Ok(Ok(())) => Ok(bytes),
        // Con AES, una contraseña incorrecta puede fallar al leer o al verificar la autenticación
        Ok(Err(_)) if password.is_some() => Err(bad("contraseña incorrecta o archivo dañado")),
        Ok(Err(_)) => Err(bad(format!("archivo corrupto: {name}"))),
        Err(ZipError::InvalidPassword) => Err(bad("contraseña incorrecta")),
        Err(ZipError::UnsupportedArchive(m)) if password.is_none() && m.to_lowercase().contains("password") => {
            Err(AppError::PasswordRequired)
        }
        Err(ZipError::FileNotFound) => Err(bad(format!("falta {name} en el respaldo"))),
        Err(e) => Err(bad(format!("respaldo ilegible ({name}): {e}"))),
    }
}

pub fn load_backup_pw(path: &Path, password: Option<&str>) -> AppResult<LoadedBackup> {
    let password = password.filter(|p| !p.is_empty());
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| bad("el archivo no es un respaldo válido de Mi Bodega"))?;
    let manifest: Manifest = {
        let bytes = read_entry(&mut zip, "manifest.json", password)?;
        serde_json::from_slice(&bytes).map_err(|_| bad("manifiesto inválido"))?
    };
    if manifest.format != FORMAT || manifest.format_version != FORMAT_VERSION {
        return Err(bad("formato de respaldo desconocido"));
    }
    if manifest.schema_version > SCHEMA_VERSION {
        return Err(bad(format!(
            "el respaldo es de una versión más nueva de la app (esquema {} > {}); actualiza Mi Bodega",
            manifest.schema_version, SCHEMA_VERSION
        )));
    }
    let mut data = Vec::new();
    for (table, cols) in TABLES {
        let info = manifest.tables.get(*table).ok_or_else(|| bad(format!("falta la tabla {table} en el manifiesto")))?;
        let bytes = read_entry(&mut zip, &format!("data/{table}.ndjson"), password)?;
        if sha_hex(&bytes) != info.sha256 {
            return Err(bad(format!("suma de verificación incorrecta en {table}: el respaldo está dañado")));
        }
        let mut rows = Vec::new();
        for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let v: Value = serde_json::from_slice(line).map_err(|_| bad(format!("fila inválida en {table}")))?;
            let Value::Object(o) = v else { return Err(bad(format!("fila inválida en {table}"))) };
            if let Some(k) = o.keys().find(|k| !cols.iter().any(|c| c.0 == k.as_str())) {
                return Err(bad(format!("columna desconocida «{k}» en {table}")));
            }
            rows.push(o);
        }
        if rows.len() as i64 != info.rows {
            return Err(bad(format!("{table}: el número de filas no coincide con el manifiesto")));
        }
        data.push((*table, *cols, rows));
    }
    Ok(LoadedBackup { manifest, data })
}

/// Resumen para mostrar antes de restaurar (valida sin modificar nada).
pub fn inspect_backup(path: &Path) -> AppResult<Manifest> {
    inspect_backup_pw(path, None)
}

pub fn inspect_backup_pw(path: &Path, password: Option<&str>) -> AppResult<Manifest> {
    Ok(load_backup_pw(path, password)?.manifest)
}

#[derive(Debug, Serialize)]
pub struct RestoreReport {
    pub pre_restore_backup: Option<String>,
    pub tables: std::collections::BTreeMap<String, i64>,
}

/// Reemplaza TODOS los datos por los del respaldo, en una sola transacción: si algo falla
/// (FK, invariante saldo == kardex) se revierte y la BD queda como estaba.
/// Antes crea un respaldo de seguridad en `pre_dir` (si la BD actual es legible).
pub async fn restore_backup(
    pool: &AnyPool,
    engine: Engine,
    path: &Path,
    pre_dir: &Path,
) -> AppResult<RestoreReport> {
    restore_backup_pw(pool, engine, path, pre_dir, None).await
}

pub async fn restore_backup_pw(
    pool: &AnyPool,
    engine: Engine,
    path: &Path,
    pre_dir: &Path,
    password: Option<&str>,
) -> AppResult<RestoreReport> {
    let loaded = load_backup_pw(path, password)?; // valida todo antes de tocar la BD

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let pre_path = pre_dir.join(format!("pre-restore-{stamp}.mibodega-backup"));
    let pre = match export_backup(pool, engine, &pre_path).await {
        Ok(_) => Some(pre_path.display().to_string()),
        Err(e) => {
            eprintln!("no se pudo crear el respaldo previo: {e}");
            None
        }
    };

    let mut tx = pool.begin().await?;
    let conn: &mut AnyConnection = &mut tx;
    for (table, _) in TABLES.iter().rev() {
        sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table}"))).execute(&mut *conn).await?;
    }
    let mut report = std::collections::BTreeMap::new();
    for (table, cols, rows) in &loaded.data {
        for row in rows {
            let present: Vec<&(&str, K)> = cols.iter().filter(|c| row.contains_key(c.0)).collect();
            let names = present.iter().map(|c| c.0).collect::<Vec<_>>().join(", ");
            let marks = vec!["?"; present.len()].join(", ");
            // Tabla y columnas salen de TABLES; los valores van como parámetros.
            let mut q = sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {table} ({names}) VALUES ({marks})")));
            for (name, kind) in &present {
                let v = &row[*name];
                q = match kind {
                    I => q.bind(if v.is_null() { None } else { Some(v.as_i64().ok_or_else(|| bad(format!("valor no numérico en {table}.{name}")))?) }),
                    T => q.bind(if v.is_null() { None } else { Some(v.as_str().ok_or_else(|| bad(format!("valor no texto en {table}.{name}")))?.to_string()) }),
                };
            }
            q.execute(&mut *conn).await.map_err(|e| bad(format!("no se pudo restaurar {table}: {e}")))?;
        }
        report.insert((*table).to_string(), rows.len() as i64);
    }
    let bad_stock = stock_divergences_conn(conn).await?;
    if !bad_stock.is_empty() {
        return Err(bad(format!("el respaldo es inconsistente: {} producto(s) con saldo distinto al kardex", bad_stock.len())));
    }
    tx.commit().await?;
    Ok(RestoreReport { pre_restore_backup: pre, tables: report })
}

/// Copia nativa consistente de SQLite (`VACUUM INTO`). Falla si el destino ya existe.
pub async fn export_sqlite_native(pool: &AnyPool, engine: Engine, path: &Path) -> AppResult<()> {
    if engine != Engine::Sqlite {
        return Err(bad("la copia nativa .db solo existe para SQLite"));
    }
    if path.exists() {
        std::fs::remove_file(path)?; // el diálogo de guardado ya confirmó el reemplazo
    }
    let escaped = path.display().to_string().replace('\'', "''");
    // El destino es una ruta elegida por el propio usuario en el diálogo del sistema.
    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{escaped}'"))).execute(pool).await?;
    Ok(())
}

/// Cuántos respaldos automáticos se conservan en la carpeta (los más recientes).
pub const AUTO_KEEP: usize = 14;

/// Borra los respaldos automáticos más antiguos, conservando los `keep` más recientes.
/// Solo toca archivos `auto-*.mibodega-backup`; nunca otros archivos de la carpeta.
pub fn rotate_auto_backups(dir: &Path, keep: usize) -> AppResult<usize> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("auto-") && n.ends_with(".mibodega-backup")
        })
        .collect();
    files.sort_by_key(|e| e.file_name()); // el sello de fecha en el nombre ordena cronológicamente
    let excess = files.len().saturating_sub(keep);
    for f in files.iter().take(excess) {
        std::fs::remove_file(f.path())?;
    }
    Ok(excess)
}

/// Respaldo automático (sin contraseña) en `dir` y rotación. Devuelve la ruta creada.
pub async fn auto_backup(pool: &AnyPool, engine: Engine, dir: &Path) -> AppResult<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = format!("auto-{}.mibodega-backup", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let path = dir.join(name);
    export_backup(pool, engine, &path).await?;
    rotate_auto_backups(dir, AUTO_KEEP)?;
    Ok(path)
}
