//! Ventas y cobros (pruebas 1-5, 7, 8 y 12 del spec §12); SQLite y, con `MYSQL_TEST_URL`, MySQL.
use crate::cash::*;
use crate::catalog::*;
use crate::db::tests::{mysql_env, sqlite_mem, MYSQL_LOCK};
use crate::db::Db;
use crate::error::AppError;
use crate::inventory::*;
use crate::purchases::*;
use crate::sales::*;

fn cash_pay(amount: i64, received: Option<i64>) -> SalePaymentInput {
    SalePaymentInput { method: "EFECTIVO".into(), amount_cents: amount, received_cents: received, reference: None }
}
fn yape(amount: i64) -> SalePaymentInput {
    SalePaymentInput { method: "YAPE".into(), amount_cents: amount, received_cents: None, reference: Some("op-123".into()) }
}
fn line(p: &str, pres: &str, qty: i64, discount: i64) -> SaleItemInput {
    SaleItemInput { product_id: p.into(), presentation_id: pres.into(), qty_presentations: qty, discount_cents: discount }
}
fn sale(req: &str, discount: i64, items: Vec<SaleItemInput>, payments: Vec<SalePaymentInput>) -> SaleInput {
    SaleInput { client_request_id: req.into(), discount_cents: discount, items, payments }
}

async fn run_all(db: &Db) {
    let pool = &db.pool;
    let pid = save_product(
        pool,
        &ProductInput {
            id: None,
            category_id: None,
            sku: None,
            barcode: None,
            name: "Gaseosa".into(),
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
    let prod = || async { list_products(pool, &ProductFilter { include_inactive: true, ..Default::default() }).await.unwrap().remove(0) };
    let p = prod().await;
    let unit = p.presentations.iter().find(|x| x.factor_to_base == 1).unwrap().id.clone();
    let caja = p.presentations.iter().find(|x| x.factor_to_base == 24).unwrap().id.clone();
    let stock = || async { prod().await.stock };
    let sales_count = || async { list_sales(pool, None, None, None).await.unwrap().len() };
    let expected = || async { current_session(pool).await.unwrap().unwrap().expected_cash_cents };
    let buy_box = |cost: i64| {
        let (pid, caja) = (pid.clone(), caja.clone());
        async move {
            let c = save_purchase(
                pool,
                &PurchaseInput {
                    id: None,
                    supplier_id: None,
                    purchased_at: None,
                    items: vec![PurchaseItemInput { product_id: pid, presentation_id: caja, qty_presentations: 1, unit_cost_cents: cost }],
                },
            )
            .await
            .unwrap();
            confirm_purchase(pool, &c).await.unwrap();
        }
    };

    // #1 comprar una caja de 24
    buy_box(3600).await;
    assert_eq!(stock().await, 24);

    // Sin caja abierta no se vende; nada cambia
    let r = confirm_sale(pool, &sale("r0", 0, vec![line(&pid, &unit, 1, 0)], vec![cash_pay(250, None)])).await;
    assert!(matches!(r, Err(AppError::Validation(_))));
    assert_eq!((stock().await, sales_count().await), (24, 0));

    // Una sola caja abierta (RN-07)
    open_cash(pool, 1000).await.unwrap();
    assert!(open_cash(pool, 0).await.is_err());
    assert_eq!(expected().await, 1000);

    // #1 vender 3 unidades -> saldo 21; vuelto calculado; snapshot de costo
    let s1 = confirm_sale(pool, &sale("r1", 0, vec![line(&pid, &unit, 3, 0)], vec![cash_pay(750, Some(1000))])).await.unwrap();
    assert_eq!(s1.summary.number, "V-000001");
    assert_eq!((s1.summary.total_cents, s1.payments[0].change_cents), (750, Some(250)));
    assert_eq!(stock().await, 21);
    assert_eq!(expected().await, 1750);

    // #3 doble confirmación (mismo request): una venta, una salida, un cobro
    let again = confirm_sale(pool, &sale("r1", 0, vec![line(&pid, &unit, 3, 0)], vec![cash_pay(750, Some(1000))])).await.unwrap();
    assert_eq!(again.summary.id, s1.summary.id);
    assert_eq!((stock().await, sales_count().await, expected().await), (21, 1, 1750));

    // #2 vender 22 con saldo 21: rechazo sin cambios
    let r = confirm_sale(pool, &sale("r2", 0, vec![line(&pid, &unit, 22, 0)], vec![cash_pay(5500, None)])).await;
    assert!(matches!(r, Err(AppError::Validation(ref m)) if m.contains("stock insuficiente")), "{r:?}");
    assert_eq!((stock().await, sales_count().await, expected().await), (21, 1, 1750));
    let kardex_len = kardex(pool, &pid, None, None, 100).await.unwrap().len();

    // #4 error en la 2.ª línea revierte la 1.ª (venta, stock y caja intactos)
    let r = confirm_sale(pool, &sale("r3", 0, vec![line(&pid, &unit, 1, 0), line(&pid, &unit, 30, 0)], vec![cash_pay(7750, None)])).await;
    assert!(r.is_err());
    assert_eq!((stock().await, sales_count().await, expected().await), (21, 1, 1750));
    assert_eq!(kardex(pool, &pid, None, None, 100).await.unwrap().len(), kardex_len);

    // Pagos que no cuadran / efectivo insuficiente
    assert!(confirm_sale(pool, &sale("r4", 0, vec![line(&pid, &unit, 4, 0)], vec![cash_pay(900, None)])).await.is_err());
    assert!(confirm_sale(pool, &sale("r5", 0, vec![line(&pid, &unit, 4, 0)], vec![cash_pay(1000, Some(900))])).await.is_err());
    assert!(confirm_sale(pool, &sale("r6", 0, vec![line(&pid, &unit, 1, 0)], vec![])).await.is_err());
    assert_eq!((stock().await, sales_count().await), (21, 1));

    // #5 pago mixto: la caja física solo sube por el efectivo neto
    let s2 = confirm_sale(pool, &sale("r7", 0, vec![line(&pid, &unit, 4, 0)], vec![cash_pay(600, Some(600)), yape(400)])).await.unwrap();
    assert_eq!(s2.summary.number, "V-000002", "los rechazos no consumen numeración");
    assert_eq!((stock().await, expected().await), (17, 2350));

    // Descuentos: línea 100 sobre 500 + descuento total 50 => 350
    let s3 = confirm_sale(pool, &sale("r8", 50, vec![line(&pid, &unit, 2, 100)], vec![cash_pay(350, None)])).await.unwrap();
    assert_eq!((s3.summary.subtotal_cents, s3.summary.discount_cents, s3.summary.total_cents), (400, 50, 350));
    assert_eq!(s3.items[0].line_total_cents, 400);
    assert!(confirm_sale(pool, &sale("r9", 0, vec![line(&pid, &unit, 1, 251)], vec![cash_pay(1, None)])).await.is_err());
    assert_eq!((stock().await, expected().await), (15, 2700));

    // #8 cambiar el precio después de vender no toca la venta histórica
    let p = prod().await;
    let edit = |unit_price: i64, caja_factor: i64| ProductInput {
        id: Some(pid.clone()),
        category_id: None,
        sku: None,
        barcode: None,
        name: "Gaseosa".into(),
        base_unit: "unidad".into(),
        reference_cost_cents: p.reference_cost_cents,
        min_stock: None,
        presentations: vec![
            PresentationInput { id: Some(unit.clone()), name: "Unidad".into(), factor_to_base: 1, sale_price_cents: unit_price },
            PresentationInput { id: Some(caja.clone()), name: "Caja".into(), factor_to_base: caja_factor, sale_price_cents: 5000 },
        ],
    };
    save_product(pool, &edit(300, 24)).await.unwrap();
    let old = get_sale(pool, &s1.summary.id).await.unwrap();
    assert_eq!((old.items[0].unit_price_cents, old.summary.total_cents), (250, 750));

    // #12 vender una caja (24) y luego cambiar la caja a 12: la venta conserva factor 24
    buy_box(3700).await; // stock 39, costo de referencia 154
    let s4 = confirm_sale(pool, &sale("r10", 0, vec![line(&pid, &caja, 1, 0)], vec![cash_pay(5000, Some(5000))])).await.unwrap();
    assert_eq!(stock().await, 15);
    assert_eq!(expected().await, 7700);
    save_product(pool, &edit(300, 12)).await.unwrap();
    let hist = get_sale(pool, &s4.summary.id).await.unwrap();
    assert_eq!((hist.items[0].factor_snapshot, hist.items[0].unit_price_cents), (24, 5000));

    // #7 anular una sola vez: repone stock (24) y revierte efectivo; segundo intento rechazado
    assert!(cancel_sale(pool, &s4.summary.id, "  ").await.is_err());
    let c = cancel_sale(pool, &s4.summary.id, "error de digitación").await.unwrap();
    assert_eq!(c.summary.status, "CANCELED");
    assert_eq!((stock().await, expected().await), (39, 2700));
    assert!(matches!(cancel_sale(pool, &s4.summary.id, "otra vez").await, Err(AppError::Validation(_))));
    assert_eq!((stock().await, expected().await), (39, 2700));
    assert!(matches!(cancel_sale(pool, "no-existe", "x").await, Err(AppError::NotFound(_))));

    // Anular pago mixto: solo se revierte el efectivo (600), Yape no toca la caja
    cancel_sale(pool, &s2.summary.id, "devolución").await.unwrap();
    assert_eq!((stock().await, expected().await), (43, 2100));

    // Producto inactivo no se vende
    set_product_active(pool, &pid, false).await.unwrap();
    assert!(confirm_sale(pool, &sale("r11", 0, vec![line(&pid, &unit, 1, 0)], vec![cash_pay(300, None)])).await.is_err());
    set_product_active(pool, &pid, true).await.unwrap();

    // Listado por estado
    assert_eq!(list_sales(pool, None, None, Some("CANCELED")).await.unwrap().len(), 2);
    assert_eq!(list_sales(pool, None, None, Some("CONFIRMED")).await.unwrap().len(), 2);

    // Invariante saldo == kardex
    assert!(stock_divergences(pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn phase4_sqlite() {
    run_all(&sqlite_mem().await).await;
}

#[tokio::test]
async fn phase4_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    match mysql_env().await {
        Some(db) => run_all(&db).await,
        None => eprintln!("MYSQL_TEST_URL no definida: se omite"),
    }
}

/// Rendimiento (spec §3): buscar para agregar al carrito debe responder en < 1 s con catálogo grande.
#[tokio::test]
async fn search_is_fast_with_5k_products() {
    let db = sqlite_mem().await;
    let mut tx = db.pool.begin().await.unwrap();
    for i in 0..5000 {
        save_product_tx(
            &mut tx,
            &ProductInput {
                id: None,
                category_id: None,
                sku: Some(format!("SKU-{i:05}")),
                barcode: Some(format!("77{i:08}")),
                name: format!("Producto de prueba {i}"),
                base_unit: "unidad".into(),
                reference_cost_cents: 100,
                min_stock: Some(1),
                presentations: vec![
                    PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: 200 },
                    PresentationInput { id: None, name: "Caja x12".into(), factor_to_base: 12, sale_price_cents: 2000 },
                ],
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let t = std::time::Instant::now();
    let r = list_products(&db.pool, &ProductFilter { search: Some("prueba 4999".into()), ..Default::default() }).await.unwrap();
    let by_barcode = list_products(&db.pool, &ProductFilter { search: Some("770000123".into()), ..Default::default() }).await.unwrap();
    let elapsed = t.elapsed();
    assert_eq!(r.len(), 1);
    assert!(!by_barcode.is_empty());
    eprintln!("búsqueda con 5000 productos: {elapsed:?}");
    assert!(elapsed.as_millis() < 1000, "demasiado lento: {elapsed:?}");
}
