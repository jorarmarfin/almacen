//! Proveedores y compras; corren en SQLite y, con `MYSQL_TEST_URL`, en MySQL.
use crate::catalog::*;
use crate::db::tests::{mysql_env, sqlite_mem, MYSQL_LOCK};
use crate::db::Db;
use crate::error::AppError;
use crate::inventory::*;
use crate::purchases::*;

async fn product_with_box(db: &Db, name: &str) -> (String, String, String) {
    let id = save_product(
        &db.pool,
        &ProductInput {
            id: None,
            category_id: None,
            sku: None,
            barcode: None,
            name: name.into(),
            base_unit: "unidad".into(),
            reference_cost_cents: 0,
            min_stock: Some(0),
            presentations: vec![
                PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: 250 },
                PresentationInput { id: None, name: "Caja x24".into(), factor_to_base: 24, sale_price_cents: 5000 },
            ],
        },
    )
    .await
    .unwrap();
    let p = list_products(&db.pool, &ProductFilter { search: Some(name.into()), ..Default::default() })
        .await
        .unwrap()
        .remove(0);
    let unit = p.presentations.iter().find(|x| x.factor_to_base == 1).unwrap().id.clone();
    let caja = p.presentations.iter().find(|x| x.factor_to_base == 24).unwrap().id.clone();
    (id, unit, caja)
}

fn item(p: &str, pres: &str, qty: i64, cost: i64) -> PurchaseItemInput {
    PurchaseItemInput { product_id: p.into(), presentation_id: pres.into(), qty_presentations: qty, unit_cost_cents: cost }
}

async fn run_all(db: &Db) {
    let pool = &db.pool;
    let (pid, unit, caja) = product_with_box(db, "Gaseosa").await;

    // Proveedores
    let sup = save_supplier(pool, &SupplierInput { id: None, name: "Distribuidora Sur".into(), tax_id: Some("20123456789".into()), phone: None, notes: None, active: true })
        .await
        .unwrap();
    assert!(save_supplier(pool, &SupplierInput { id: None, name: " ".into(), tax_id: None, phone: None, notes: None, active: true }).await.is_err());

    // Validaciones de líneas
    let bad = PurchaseInput { id: None, supplier_id: None, purchased_at: None, items: vec![item(&pid, &caja, 0, 100)] };
    assert!(matches!(save_purchase(pool, &bad).await, Err(AppError::Validation(_))));
    let empty = PurchaseInput { id: None, supplier_id: None, purchased_at: None, items: vec![] };
    assert!(save_purchase(pool, &empty).await.is_err());
    let wrong = PurchaseInput { id: None, supplier_id: None, purchased_at: None, items: vec![item("otro", &caja, 1, 100)] };
    assert!(save_purchase(pool, &wrong).await.is_err());

    // Borrador: no toca stock; numeración C-000001
    let c1 = save_purchase(pool, &PurchaseInput { id: None, supplier_id: Some(sup.clone()), purchased_at: None, items: vec![item(&pid, &caja, 1, 3600)] })
        .await
        .unwrap();
    let d = get_purchase(pool, &c1).await.unwrap();
    assert_eq!(d.summary.number, "C-000001");
    assert_eq!((d.summary.status.as_str(), d.summary.total_cents), ("DRAFT", 3600));
    assert_eq!(d.items[0].factor_snapshot, 24);
    let stock = |_: ()| async { list_products(pool, &ProductFilter { include_inactive: true, ..Default::default() }).await.unwrap().remove(0).stock };
    assert_eq!(stock(()).await, 0);

    // Editar borrador reemplaza líneas
    save_purchase(pool, &PurchaseInput { id: Some(c1.clone()), supplier_id: Some(sup.clone()), purchased_at: None, items: vec![item(&pid, &caja, 2, 3600), item(&pid, &unit, 3, 160)] })
        .await
        .unwrap();
    let d = get_purchase(pool, &c1).await.unwrap();
    assert_eq!((d.items.len(), d.summary.total_cents), (2, 2 * 3600 + 3 * 160));
    // quitar la línea suelta: 1 caja de 24 a 36.00
    save_purchase(pool, &PurchaseInput { id: Some(c1.clone()), supplier_id: Some(sup.clone()), purchased_at: None, items: vec![item(&pid, &caja, 1, 3600)] })
        .await
        .unwrap();

    // Confirmar: caja de 24 -> stock 24, costo de referencia 36.00/24 = 1.50
    confirm_purchase(pool, &c1).await.unwrap();
    assert_eq!(stock(()).await, 24);
    let p = list_products(pool, &ProductFilter::default()).await.unwrap().remove(0);
    assert_eq!(p.reference_cost_cents, 150);
    let k = kardex(pool, &pid, None, None, 10).await.unwrap();
    assert_eq!((k[0].mtype.as_str(), k[0].qty_base_signed, k[0].note.as_deref()), ("COMPRA", 24, Some("C-000001")));

    // Doble confirmación y edición de confirmada: rechazadas sin efectos
    assert!(matches!(confirm_purchase(pool, &c1).await, Err(AppError::Validation(_))));
    assert_eq!(stock(()).await, 24);
    let edit = PurchaseInput { id: Some(c1.clone()), supplier_id: None, purchased_at: None, items: vec![item(&pid, &unit, 1, 100)] };
    assert!(save_purchase(pool, &edit).await.is_err());
    assert!(cancel_draft(pool, &c1).await.is_err());

    // Segunda compra: último costo manda (redondeo half-up) y numeración consecutiva
    let c2 = save_purchase(pool, &PurchaseInput { id: None, supplier_id: None, purchased_at: None, items: vec![item(&pid, &caja, 1, 3700)] })
        .await
        .unwrap();
    assert_eq!(get_purchase(pool, &c2).await.unwrap().summary.number, "C-000002");
    confirm_purchase(pool, &c2).await.unwrap();
    assert_eq!(stock(()).await, 48);
    let p = list_products(pool, &ProductFilter::default()).await.unwrap().remove(0);
    assert_eq!(p.reference_cost_cents, 154); // 3700 / 24 = 154.17 -> 154

    // Descartar borrador
    let c3 = save_purchase(pool, &PurchaseInput { id: None, supplier_id: None, purchased_at: None, items: vec![item(&pid, &unit, 5, 100)] })
        .await
        .unwrap();
    cancel_draft(pool, &c3).await.unwrap();
    assert!(confirm_purchase(pool, &c3).await.is_err());
    assert_eq!(stock(()).await, 48);

    // Listado por estado
    assert_eq!(list_purchases(pool, Some("CONFIRMED")).await.unwrap().len(), 2);
    assert_eq!(list_purchases(pool, None).await.unwrap().len(), 3);

    // Proveedor con compras: solo desactivar
    assert!(delete_supplier(pool, &sup).await.is_err());
    let free = save_supplier(pool, &SupplierInput { id: None, name: "Temporal".into(), tax_id: None, phone: None, notes: None, active: true }).await.unwrap();
    delete_supplier(pool, &free).await.unwrap();

    assert!(stock_divergences(pool).await.unwrap().is_empty());
}

#[test]
fn cost_rounding_half_up() {
    assert_eq!(cost_per_base_unit(3600, 24), 150);
    assert_eq!(cost_per_base_unit(3700, 24), 154);
    assert_eq!(cost_per_base_unit(3710, 24), 155); // 154.58 -> 155
    assert_eq!(cost_per_base_unit(100, 1), 100);
}

#[tokio::test]
async fn phase3_sqlite() {
    run_all(&sqlite_mem().await).await;
}

#[tokio::test]
async fn phase3_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    match mysql_env().await {
        Some(db) => run_all(&db).await,
        None => eprintln!("MYSQL_TEST_URL no definida: se omite"),
    }
}
