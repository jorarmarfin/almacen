//! Respaldo/restauración, migraciones con datos y paridad de columnas (spec #9, #10, #11 y DoD).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sqlx::AnyPool;

use crate::backup::*;
use crate::cash::*;
use crate::catalog::*;
use crate::config::Engine;
use crate::db::tests::{mysql_env, sqlite_mem, MYSQL_LOCK};
use crate::db::Db;
use crate::inventory::*;
use crate::purchases::*;
use crate::sales::*;
use crate::util::new_id;

fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("mibodega-test-{}", new_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Tabla -> filas serializadas y ordenadas (comparable entre motores).
fn snapshot(path: &Path) -> BTreeMap<String, Vec<String>> {
    let b = load_backup(path).unwrap();
    b.data
        .into_iter()
        .map(|(t, _, rows)| {
            let mut v: Vec<String> = rows.into_iter().map(|r| serde_json::to_string(&r).unwrap()).collect();
            v.sort();
            (t.to_string(), v)
        })
        .collect()
}

/// Negocio con datos en todas las áreas: catálogo, compra, ventas (mixta + anulada), caja cerrada y abierta.
async fn populate(pool: &AnyPool) {
    sqlx::query("INSERT INTO business_settings (id, business_name, currency, default_min_stock, schema_version) VALUES (1, 'Bodega Ñandú', 'PEN', 5, 2)")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app_metadata (id, installation_id, schema_version, updated_at) VALUES (1, 'inst-123', 2, '2026-10-30T00:00:00.000Z')")
        .execute(pool).await.unwrap();
    let cat = save_category(pool, None, "Bebidas", true).await.unwrap();
    let pid = save_product(
        pool,
        &ProductInput {
            id: None,
            category_id: Some(cat),
            sku: None,
            barcode: Some("775".into()),
            name: "Gaseosa ñ ¡ü!".into(),
            base_unit: "unidad".into(),
            reference_cost_cents: 0,
            min_stock: Some(5),
            presentations: vec![
                PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: 250 },
                PresentationInput { id: None, name: "Caja x24".into(), factor_to_base: 24, sale_price_cents: 5000 },
            ],
        },
    )
    .await
    .unwrap();
    let p = list_products(pool, &ProductFilter::default()).await.unwrap().remove(0);
    let unit = p.presentations.iter().find(|x| x.factor_to_base == 1).unwrap().id.clone();
    let caja = p.presentations.iter().find(|x| x.factor_to_base == 24).unwrap().id.clone();
    let sup = save_supplier(
        pool,
        &SupplierInput { id: None, name: "Distribuidora".into(), tax_id: Some("20123456789".into()), phone: None, notes: Some("Entrega los martes".into()), active: true },
    )
    .await
    .unwrap();
    let c = save_purchase(
        pool,
        &PurchaseInput {
            id: None,
            supplier_id: Some(sup),
            purchased_at: None,
            items: vec![PurchaseItemInput { product_id: pid.clone(), presentation_id: caja.clone(), qty_presentations: 2, unit_cost_cents: 3600 }],
        },
    )
    .await
    .unwrap();
    confirm_purchase(pool, &c).await.unwrap();
    open_cash(pool, 1000).await.unwrap();
    let pay = |m: &str, a: i64| SalePaymentInput { method: m.into(), amount_cents: a, received_cents: None, reference: None };
    let line = |q: i64| SaleItemInput { product_id: pid.clone(), presentation_id: unit.clone(), qty_presentations: q, discount_cents: 0 };
    confirm_sale(pool, &SaleInput { client_request_id: "a".into(), discount_cents: 0, items: vec![line(4)], payments: vec![pay("EFECTIVO", 600), pay("YAPE", 400)] }).await.unwrap();
    let s2 = confirm_sale(pool, &SaleInput { client_request_id: "b".into(), discount_cents: 0, items: vec![line(2)], payments: vec![pay("EFECTIVO", 500)] }).await.unwrap();
    cancel_sale(pool, &s2.summary.id, "error").await.unwrap();
    add_manual_movement(pool, ManualKind::Egreso, "EFECTIVO", 200, "bolsas").await.unwrap();
    close_cash(pool, 1_390, Some("faltan 10")).await.unwrap();
    open_cash(pool, 500).await.unwrap();
    adjust_stock(pool, &pid, AdjustKind::Salida, 1, "merma").await.unwrap();
}

async fn export_to(db: &Db, dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    export_backup(&db.pool, db.engine, &p).await.unwrap();
    p
}

async fn roundtrip(db: &Db) {
    let dir = tmp_dir();
    populate(&db.pool).await;
    let original = export_to(db, &dir, "a.mibodega-backup").await;
    let snap_a = snapshot(&original);
    let m = inspect_backup(&original).unwrap();
    assert_eq!((m.schema_version, m.business_name.as_deref(), m.installation_id.as_deref()), (2, Some("Bodega Ñandú"), Some("inst-123")));
    assert!(m.tables["sales"].rows == 2 && m.tables["stock_movements"].rows > 3);

    // Alterar la BD (otra venta + ajuste) y restaurar: vuelve exactamente al respaldo
    adjust_stock(&db.pool, &list_products(&db.pool, &ProductFilter::default()).await.unwrap()[0].id, AdjustKind::Entrada, 50, "x").await.unwrap();
    let before_restore = export_to(db, &dir, "changed.mibodega-backup").await;
    assert_ne!(snapshot(&before_restore), snap_a);
    let rep = restore_backup(&db.pool, db.engine, &original, &dir.join("pre")).await.unwrap();
    assert!(rep.pre_restore_backup.as_deref().is_some_and(|p| Path::new(p).exists()), "debe crear respaldo previo");
    let after = export_to(db, &dir, "b.mibodega-backup").await;
    assert_eq!(snapshot(&after), snap_a, "la restauración reproduce ventas, inventario y caja");
    assert!(stock_divergences(&db.pool).await.unwrap().is_empty());
    // el respaldo previo permite deshacer
    restore_backup(&db.pool, db.engine, Path::new(rep.pre_restore_backup.as_ref().unwrap()), &dir.join("pre")).await.unwrap();
    assert_eq!(snapshot(&export_to(db, &dir, "c.mibodega-backup").await)["stock_movements"], snapshot(&before_restore)["stock_movements"]);

    // La app sigue operando tras restaurar (caja abierta restaurada, numeración continúa)
    let cur = current_session(&db.pool).await.unwrap().unwrap();
    assert_eq!(cur.opening_cash_cents, 500);
    std::fs::remove_dir_all(dir).ok();
}

/// #9 respaldo corrupto: se rechaza y la BD activa no cambia.
async fn corrupt(db: &Db) {
    let dir = tmp_dir();
    populate(&db.pool).await;
    let good = export_to(db, &dir, "good.mibodega-backup").await;
    let before = snapshot(&export_to(db, &dir, "before.mibodega-backup").await);

    // 1) tabla alterada con manifiesto intacto -> checksum no coincide
    let bad_path = dir.join("bad.mibodega-backup");
    rewrite_zip(&good, &bad_path, |name, bytes| {
        if name == "data/products.ndjson" {
            let s = String::from_utf8(bytes).unwrap().replace("Gaseosa", "Gaseosx");
            return s.into_bytes();
        }
        bytes
    });
    let e = inspect_backup(&bad_path).unwrap_err().to_string();
    assert!(e.contains("suma de verificación"), "{e}");
    assert!(restore_backup(&db.pool, db.engine, &bad_path, &dir.join("pre")).await.is_err());

    // 2) esquema más nuevo
    let newer = dir.join("newer.mibodega-backup");
    rewrite_zip(&good, &newer, |name, bytes| {
        if name == "manifest.json" {
            return String::from_utf8(bytes).unwrap().replace("\"schema_version\": 2", "\"schema_version\": 999").into_bytes();
        }
        bytes
    });
    assert!(inspect_backup(&newer).unwrap_err().to_string().contains("más nueva"));

    // 3) archivo basura / truncado
    let junk = dir.join("junk.mibodega-backup");
    std::fs::write(&junk, b"esto no es un zip").unwrap();
    assert!(restore_backup(&db.pool, db.engine, &junk, &dir.join("pre")).await.is_err());
    let data = std::fs::read(&good).unwrap();
    let trunc = dir.join("trunc.mibodega-backup");
    std::fs::write(&trunc, &data[..data.len() / 2]).unwrap();
    assert!(restore_backup(&db.pool, db.engine, &trunc, &dir.join("pre")).await.is_err());

    // 4) FK rota dentro de un respaldo con checksums válidos: la tx revierte todo
    let broken = dir.join("broken.mibodega-backup");
    rewrite_zip_fixing_hashes(&good, &broken, "sale_items", |s| s.replace("\"sale_id\":\"", "\"sale_id\":\"zzz-"));
    assert!(inspect_backup(&broken).is_ok(), "checksums coherentes");
    assert!(restore_backup(&db.pool, db.engine, &broken, &dir.join("pre")).await.is_err());

    // `backup_info` cambia en cada export (marca del último respaldo); todo lo demás debe ser idéntico.
    let strip = |mut m: BTreeMap<String, Vec<String>>| {
        m.remove("app_metadata");
        m
    };
    let after = snapshot(&export_to(db, &dir, "after.mibodega-backup").await);
    assert!(strip(before) == strip(after), "la BD activa no se modificó");
    std::fs::remove_dir_all(dir).ok();
}

fn rewrite_zip(src: &Path, dst: &Path, f: impl Fn(&str, Vec<u8>) -> Vec<u8>) {
    use std::io::{Read, Write};
    let mut zin = zip::ZipArchive::new(std::fs::File::open(src).unwrap()).unwrap();
    let mut zout = zip::ZipWriter::new(std::fs::File::create(dst).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..zin.len() {
        let mut file = zin.by_index(i).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        zout.start_file(name.as_str(), opts).unwrap();
        zout.write_all(&f(&name, bytes)).unwrap();
    }
    zout.finish().unwrap();
}

/// Modifica una tabla y recalcula su hash en el manifiesto (simula un respaldo coherente pero inválido).
fn rewrite_zip_fixing_hashes(src: &Path, dst: &Path, table: &str, edit: impl Fn(String) -> String) {
    use sha2::{Digest, Sha256};
    let new_hash = std::cell::RefCell::new(String::new());
    rewrite_zip(src, dst, |name, bytes| {
        if name == format!("data/{table}.ndjson") {
            let out = edit(String::from_utf8(bytes).unwrap()).into_bytes();
            *new_hash.borrow_mut() = Sha256::digest(&out).iter().map(|b| format!("{b:02x}")).collect();
            return out;
        }
        bytes
    });
    let old = load_backup(src).unwrap().manifest.tables[table].sha256.clone();
    let tmp = dst.with_extension("fix");
    let nh = new_hash.borrow().clone();
    rewrite_zip(dst, &tmp, |name, bytes| {
        if name == "manifest.json" {
            return String::from_utf8(bytes).unwrap().replace(&old, &nh).into_bytes();
        }
        bytes
    });
    std::fs::rename(tmp, dst).unwrap();
}

#[tokio::test]
async fn backup_roundtrip_sqlite() {
    roundtrip(&sqlite_mem().await).await;
}

#[tokio::test]
async fn backup_corrupt_sqlite() {
    corrupt(&sqlite_mem().await).await;
}

#[tokio::test]
async fn backup_roundtrip_and_corrupt_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    if let Some(db) = mysql_env().await {
        roundtrip(&db).await;
        let db = mysql_env().await.unwrap();
        corrupt(&db).await;
    } else {
        eprintln!("MYSQL_TEST_URL no definida: se omite");
    }
}

/// Migrar entre motores: SQLite -> MySQL -> SQLite sin pérdida.
#[tokio::test]
async fn backup_cross_engine() {
    let _g = MYSQL_LOCK.lock().await;
    let Some(my) = mysql_env().await else {
        eprintln!("MYSQL_TEST_URL no definida: se omite");
        return;
    };
    let dir = tmp_dir();
    let lite = sqlite_mem().await;
    populate(&lite.pool).await;
    let a = export_to(&lite, &dir, "lite.mibodega-backup").await;
    let snap = snapshot(&a);

    restore_backup(&my.pool, Engine::Mysql, &a, &dir.join("pre")).await.unwrap();
    let b = export_to(&my, &dir, "my.mibodega-backup").await;
    assert_eq!(snapshot(&b), snap, "SQLite -> MySQL");
    assert_eq!(inspect_backup(&b).unwrap().engine, "mysql");

    let lite2 = sqlite_mem().await;
    restore_backup(&lite2.pool, Engine::Sqlite, &b, &dir.join("pre2")).await.unwrap();
    let c = export_to(&lite2, &dir, "lite2.mibodega-backup").await;
    assert_eq!(snapshot(&c), snap, "MySQL -> SQLite");
    // y la app opera sobre el motor destino
    assert!(current_session(&my.pool).await.unwrap().is_some());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn sqlite_native_copy() {
    let dir = tmp_dir();
    // VACUUM INTO necesita una BD en archivo
    let file = dir.join("live.db");
    let db = Db::connect_url(&format!("sqlite://{}?mode=rwc", file.display()), Engine::Sqlite, 1).await.unwrap();
    db.migrate().await.unwrap();
    populate(&db.pool).await;
    let copy = dir.join("copy.db");
    export_sqlite_native(&db.pool, Engine::Sqlite, &copy).await.unwrap();
    let c = Db::connect_url(&format!("sqlite://{}", copy.display()), Engine::Sqlite, 1).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales").fetch_one(&c.pool).await.unwrap();
    assert_eq!(n, 2);
    assert!(export_sqlite_native(&sqlite_mem().await.pool, Engine::Mysql, &dir.join("x.db")).await.is_err());
    std::fs::remove_dir_all(dir).ok();
}

/// Las columnas de TABLES deben coincidir con el esquema real: si falta una, el respaldo perdería datos.
async fn columns_match(db: &Db) {
    for (table, cols) in TABLES {
        let mut actual: Vec<String> = match db.engine {
            Engine::Sqlite => sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT name FROM pragma_table_info('{table}')")))
                .fetch_all(&db.pool).await.unwrap(),
            Engine::Mysql => sqlx::query_scalar(
                "SELECT CAST(column_name AS CHAR) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = ?",
            )
            .bind(*table)
            .fetch_all(&db.pool).await.unwrap(),
        };
        let mut expected: Vec<String> = cols.iter().map(|c| c.0.to_string()).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "columnas de {table}");
    }
    // y no hay tablas de negocio sin respaldar
    let tables: Vec<String> = match db.engine {
        Engine::Sqlite => sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '\\_sqlx%' ESCAPE '\\' AND name NOT LIKE 'sqlite_%'")
            .fetch_all(&db.pool).await.unwrap(),
        Engine::Mysql => sqlx::query_scalar("SELECT CAST(table_name AS CHAR) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name NOT LIKE '\\_sqlx%'")
            .fetch_all(&db.pool).await.unwrap(),
    };
    let mut got = tables;
    got.sort();
    let mut want: Vec<String> = TABLES.iter().map(|t| t.0.to_string()).collect();
    want.sort();
    assert_eq!(got, want);
}

#[tokio::test]
async fn backup_columns_match_schema_sqlite() {
    columns_match(&sqlite_mem().await).await;
}

#[tokio::test]
async fn backup_columns_match_schema_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    if let Some(db) = mysql_env().await {
        columns_match(&db).await;
    }
}

/// Actualización de versión con datos: aplicar solo la migración 1, insertar, luego migrar al esquema actual.
async fn upgrade_keeps_data(db: &Db, mut v1: sqlx::migrate::Migrator, full: sqlx::migrate::Migrator) {
    let only_first: Vec<_> = v1.migrations.iter().filter(|m| m.version == 1).cloned().collect();
    v1.migrations = std::borrow::Cow::Owned(only_first);
    v1.run(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO products (id, sku, name, base_unit, reference_cost_cents, min_stock, active, created_at, updated_at) VALUES ('p1','S1','Arroz','unidad',100,1,1,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO stock_balances (product_id, qty_base) VALUES ('p1', 10)").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO stock_movements (id, product_id, type, qty_base_signed, origin_type, occurred_at) VALUES ('m1','p1','AJUSTE_ENTRADA',10,'AJUSTE','2026-01-01T00:00:00.000Z')").execute(&db.pool).await.unwrap();

    full.run(&db.pool).await.unwrap(); // actualiza a la última versión
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products").fetch_one(&db.pool).await.unwrap();
    assert_eq!(n, 1);
    let (before, seq): (Option<i64>, Option<i64>) = {
        let r = sqlx::query("SELECT balance_before, seq FROM stock_movements WHERE id = 'm1'").fetch_one(&db.pool).await.unwrap();
        use sqlx::Row;
        (r.get("balance_before"), r.get("seq"))
    };
    assert_eq!((before, seq), (None, None), "datos antiguos intactos, columnas nuevas nulas");
    // y sigue operando: un ajuste nuevo funciona sobre datos migrados
    assert_eq!(adjust_stock(&db.pool, "p1", AdjustKind::Salida, 3, "merma").await.unwrap(), 7);
    assert!(stock_divergences(&db.pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn upgrade_sqlite() {
    let db = Db::connect_url("sqlite::memory:", Engine::Sqlite, 1).await.unwrap();
    upgrade_keeps_data(&db, sqlx::migrate!("./migrations/sqlite"), sqlx::migrate!("./migrations/sqlite")).await;
}

#[tokio::test]
async fn upgrade_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    let Some(db) = mysql_env().await else { return };
    // esquema vacío sin migrar
    sqlx::query("SET FOREIGN_KEY_CHECKS = 0").execute(&db.pool).await.unwrap();
    let tables: Vec<String> = sqlx::query_scalar("SELECT CAST(table_name AS CHAR) FROM information_schema.tables WHERE table_schema = DATABASE()")
        .fetch_all(&db.pool).await.unwrap();
    for t in tables {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS `{t}`"))).execute(&db.pool).await.unwrap();
    }
    sqlx::query("SET FOREIGN_KEY_CHECKS = 1").execute(&db.pool).await.unwrap();
    upgrade_keeps_data(&db, sqlx::migrate!("./migrations/mysql"), sqlx::migrate!("./migrations/mysql")).await;
}

/// Respaldo cifrado: el contenido no es legible, exige contraseña y restaura con la correcta.
async fn encrypted_backup(db: &Db) {
    use crate::error::AppError;
    let dir = tmp_dir();
    populate(&db.pool).await;
    let plain = export_to(db, &dir, "plain.mibodega-backup").await;
    let snap = snapshot(&plain);

    let enc = dir.join("enc.mibodega-backup");
    export_backup_pw(&db.pool, db.engine, &enc, Some("clave-123")).await.unwrap();
    let raw = std::fs::read(&enc).unwrap();
    let contains = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(&raw, b"business_name") && !contains(&raw, "Bodega".as_bytes()) && !contains(&raw, b"Gaseosa"), "nada legible en claro");

    assert!(matches!(inspect_backup(&enc), Err(AppError::PasswordRequired)));
    assert!(inspect_backup_pw(&enc, Some("mala")).is_err());
    let m = inspect_backup_pw(&enc, Some("clave-123")).unwrap();
    assert_eq!(m.business_name.as_deref(), Some("Bodega Ñandú"));

    // Sin contraseña / con contraseña mala: no se modifica la BD
    adjust_stock(&db.pool, &list_products(&db.pool, &ProductFilter::default()).await.unwrap()[0].id, AdjustKind::Entrada, 7, "x").await.unwrap();
    let changed = snapshot(&export_to(db, &dir, "changed.mibodega-backup").await);
    assert!(matches!(
        restore_backup_pw(&db.pool, db.engine, &enc, &dir.join("pre"), None).await,
        Err(AppError::PasswordRequired)
    ));
    assert!(restore_backup_pw(&db.pool, db.engine, &enc, &dir.join("pre"), Some("mala")).await.is_err());
    let still = snapshot(&export_to(db, &dir, "still.mibodega-backup").await);
    let strip = |mut m: BTreeMap<String, Vec<String>>| {
        m.remove("app_metadata");
        m
    };
    assert!(strip(changed) == strip(still), "BD intacta tras intentos fallidos");

    // Con la contraseña correcta vuelve exactamente al respaldo
    restore_backup_pw(&db.pool, db.engine, &enc, &dir.join("pre"), Some("clave-123")).await.unwrap();
    let after = snapshot(&export_to(db, &dir, "after.mibodega-backup").await);
    assert!(strip(after) == strip(snap));
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn backup_encrypted_sqlite() {
    encrypted_backup(&sqlite_mem().await).await;
}

#[tokio::test]
async fn backup_encrypted_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    if let Some(db) = mysql_env().await {
        encrypted_backup(&db).await;
    }
}

#[test]
fn rotation_keeps_latest_and_only_touches_auto_files() {
    let dir = tmp_dir();
    for i in 0..20 {
        std::fs::write(dir.join(format!("auto-20261030-{i:06}.mibodega-backup")), b"x").unwrap();
    }
    std::fs::write(dir.join("manual.mibodega-backup"), b"x").unwrap();
    std::fs::write(dir.join("notas.txt"), b"x").unwrap();
    assert_eq!(rotate_auto_backups(&dir, 14).unwrap(), 6);
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    left.sort();
    assert_eq!(left.iter().filter(|n| n.starts_with("auto-")).count(), 14);
    assert!(left.contains(&"auto-20261030-000019.mibodega-backup".to_string()), "se conservan los más recientes");
    assert!(!left.contains(&"auto-20261030-000000.mibodega-backup".to_string()));
    assert!(left.contains(&"manual.mibodega-backup".to_string()) && left.contains(&"notas.txt".to_string()));
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn auto_backup_creates_valid_file() {
    let dir = tmp_dir();
    let db = sqlite_mem().await;
    populate(&db.pool).await;
    let p = auto_backup(&db.pool, Engine::Sqlite, &dir.join("respaldos")).await.unwrap();
    assert!(p.file_name().unwrap().to_string_lossy().starts_with("auto-"));
    assert_eq!(inspect_backup(&p).unwrap().tables["sales"].rows, 2);
    std::fs::remove_dir_all(dir).ok();
}
