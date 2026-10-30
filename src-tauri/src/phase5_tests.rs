//! Caja (movimientos, cierre) y reportes; SQLite y, con `MYSQL_TEST_URL`, MySQL.
use crate::cash::*;
use crate::catalog::*;
use crate::db::tests::{mysql_env, sqlite_mem, MYSQL_LOCK};
use crate::db::Db;
use crate::error::AppError;
use crate::inventory::*;
use crate::reports::*;
use crate::sales::*;

const FROM: &str = "2000-01-01T00:00:00.000Z";
const TO: &str = "2100-01-01T00:00:00.000Z";

fn cash_pay(a: i64) -> SalePaymentInput {
    SalePaymentInput { method: "EFECTIVO".into(), amount_cents: a, received_cents: None, reference: None }
}
fn yape(a: i64) -> SalePaymentInput {
    SalePaymentInput { method: "YAPE".into(), amount_cents: a, received_cents: None, reference: None }
}
fn line(p: &str, pres: &str, qty: i64) -> SaleItemInput {
    SaleItemInput { product_id: p.into(), presentation_id: pres.into(), qty_presentations: qty, discount_cents: 0 }
}
fn sale(req: &str, items: Vec<SaleItemInput>, payments: Vec<SalePaymentInput>) -> SaleInput {
    SaleInput { client_request_id: req.into(), discount_cents: 0, items, payments }
}

async fn make_product(pool: &sqlx::AnyPool, name: &str, cost: i64, price: i64, stock: i64) -> (String, String) {
    let id = save_product(
        pool,
        &ProductInput {
            id: None,
            category_id: None,
            sku: None,
            barcode: None,
            name: name.into(),
            base_unit: "unidad".into(),
            reference_cost_cents: cost,
            min_stock: Some(5),
            presentations: vec![PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: price }],
        },
    )
    .await
    .unwrap();
    adjust_stock(pool, &id, AdjustKind::Entrada, stock, "STOCK_INICIAL").await.unwrap();
    let pres = list_products(pool, &ProductFilter { search: Some(name.into()), ..Default::default() })
        .await
        .unwrap()
        .remove(0)
        .presentations[0]
        .id
        .clone();
    (id, pres)
}

async fn run_all(db: &Db) {
    let pool = &db.pool;
    let (a, a_pres) = make_product(pool, "Arroz", 300, 400, 100).await; // margen 100 por unidad
    let (b, b_pres) = make_product(pool, "Azúcar", 200, 350, 100).await; // margen 150 por unidad

    // Sin caja abierta no hay movimientos ni cierre
    assert!(add_manual_movement(pool, ManualKind::Ingreso, "EFECTIVO", 100, "x").await.is_err());
    assert!(close_cash(pool, 0, None).await.is_err());

    open_cash(pool, 10_000).await.unwrap();
    let exp = || async { current_session(pool).await.unwrap().unwrap().expected_cash_cents };

    // Movimientos manuales: validaciones y efecto solo del efectivo
    assert!(add_manual_movement(pool, ManualKind::Ingreso, "EFECTIVO", 0, "x").await.is_err());
    assert!(add_manual_movement(pool, ManualKind::Ingreso, "EFECTIVO", 100, "  ").await.is_err());
    add_manual_movement(pool, ManualKind::Ingreso, "EFECTIVO", 500, "Cambio inicial extra").await.unwrap();
    add_manual_movement(pool, ManualKind::Egreso, "EFECTIVO", 200, "Pago de bolsas").await.unwrap();
    add_manual_movement(pool, ManualKind::Ingreso, "YAPE", 9_999, "Transferencia").await.unwrap();
    assert_eq!(exp().await, 10_000 + 500 - 200, "Yape no altera el efectivo físico");
    let r = add_manual_movement(pool, ManualKind::Retiro, "EFECTIVO", 999_999, "retiro").await;
    assert!(matches!(r, Err(AppError::Validation(_))), "no puede dejar el efectivo negativo");
    add_manual_movement(pool, ManualKind::Retiro, "EFECTIVO", 300, "Retiro del dueño").await.unwrap();
    assert_eq!(exp().await, 10_000);

    // Ventas: 3 de Arroz (1200) en efectivo; 2 de Azúcar (700) mixto 200 efectivo + 500 Yape
    let s1 = confirm_sale(pool, &sale("a", vec![line(&a, &a_pres, 3)], vec![cash_pay(1200)])).await.unwrap();
    let s2 = confirm_sale(pool, &sale("b", vec![line(&b, &b_pres, 2)], vec![cash_pay(200), yape(500)])).await.unwrap();
    let s3 = confirm_sale(pool, &sale("c", vec![line(&a, &a_pres, 1)], vec![yape(400)])).await.unwrap();
    assert_eq!(exp().await, 10_000 + 1200 + 200);

    // Anular s3 (Yape): no toca la caja; se informa aparte
    cancel_sale(pool, &s3.summary.id, "cliente se arrepintió").await.unwrap();
    assert_eq!(exp().await, 11_400);

    // ---- Reportes (HU-08: totales == ventas válidas netas de anuladas) ----
    let rep = sales_report(pool, FROM, TO).await.unwrap();
    assert_eq!((rep.count, rep.total_cents), (2, 1200 + 700));
    assert_eq!((rep.canceled_count, rep.canceled_total_cents), (1, 400));
    let m = |k: &str| rep.by_method.iter().find(|x| x.key == k).map(|x| x.total_cents).unwrap_or(0);
    assert_eq!((m("EFECTIVO"), m("YAPE")), (1400, 500));
    assert_eq!(rep.by_day.len(), 1);
    assert_eq!((rep.by_day[0].count, rep.by_day[0].total_cents), (2, 1900));
    assert_eq!(rep.by_day[0].date, local_date(&s1.summary.sold_at));
    // margen: Arroz 3*(400-300)=300 ; Azúcar 2*(350-200)=300
    assert_eq!((rep.revenue_cents, rep.cost_cents, rep.margin_cents), (1200 + 700, 900 + 400, 600));
    // rango vacío
    let empty = sales_report(pool, "2001-01-01T00:00:00.000Z", "2001-01-02T00:00:00.000Z").await.unwrap();
    assert_eq!((empty.count, empty.total_cents, empty.margin_cents), (0, 0, 0));

    let top_u = top_products(pool, FROM, TO, TopMetric::Units, 10).await.unwrap();
    assert_eq!((top_u[0].name.as_str(), top_u[0].units_base), ("Arroz", 3));
    let top_a = top_products(pool, FROM, TO, TopMetric::Amount, 10).await.unwrap();
    assert_eq!((top_a[0].name.as_str(), top_a[0].amount_cents), ("Arroz", 1200));
    assert_eq!(top_a.len(), 2, "las anuladas no cuentan");

    let dash = dashboard(pool, FROM, TO).await.unwrap();
    assert_eq!((dash.sales_count, dash.sales_total_cents, dash.active_products, dash.cash_open), (2, 1900, 2, true));
    assert_eq!(dash.low_stock_count, 0);

    // ---- Detalle de sesión ----
    let id = current_session(pool).await.unwrap().unwrap().id;
    let d = session_detail(pool, &id).await.unwrap();
    assert_eq!(d.expected_cash_cents, 11_400);
    let by = |k: &str| d.cash_by_type.iter().find(|x| x.key == k).map(|x| x.total_cents);
    assert_eq!((by("INGRESO"), by("EGRESO"), by("RETIRO"), by("VENTA")), (Some(500), Some(-200), Some(-300), Some(1400)));
    assert!(d.cash_by_type.iter().all(|x| x.key != "ANULACION_VENTA"), "s3 fue solo Yape");

    // ---- Exportaciones ----
    let csv = export_sales_csv(pool, FROM, TO).await.unwrap();
    assert!(csv.starts_with('\u{feff}'));
    assert_eq!(csv.lines().count(), 1 + 3, "cabecera + 3 ventas (incluye anulada)");
    assert!(csv.contains("ANULADA") && csv.contains("12.00"));
    let pcsv = export_products_csv(pool).await.unwrap();
    assert_eq!(pcsv.lines().count(), 3);
    assert!(pcsv.contains("Arroz") && pcsv.contains("3.00") && pcsv.contains("4.00"));
    let kcsv = export_kardex_csv(pool, FROM, TO).await.unwrap();
    // 2 stock inicial + 3 ventas + 1 anulación = 6 movimientos
    assert_eq!(kcsv.lines().count(), 1 + 6);

    // ---- #6 Cierre con diferencia: conserva esperado, contado y diferencia ----
    let closed = close_cash(pool, 11_000, Some("faltan S/ 4.00")).await.unwrap();
    assert_eq!(closed, id);
    let sessions = list_sessions(pool).await.unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(
        (s.status.as_str(), s.expected_cash_cents, s.counted_cash_cents, s.difference_cents),
        ("CLOSED", Some(11_400), Some(11_000), Some(-400))
    );
    assert!(s.closed_at.is_some() && s.note.as_deref() == Some("faltan S/ 4.00"));
    assert!(current_session(pool).await.unwrap().is_none());

    // Sesión cerrada inmutable: no cierra de nuevo, no admite movimientos ni ventas
    assert!(close_cash(pool, 0, None).await.is_err());
    assert!(add_manual_movement(pool, ManualKind::Ingreso, "EFECTIVO", 100, "x").await.is_err());
    assert!(confirm_sale(pool, &sale("d", vec![line(&a, &a_pres, 1)], vec![cash_pay(400)])).await.is_err());

    // ---- RN-11: anular una venta en efectivo de un periodo cerrado ----
    assert!(cancel_sale(pool, &s1.summary.id, "devolución").await.is_err(), "exige caja abierta");
    assert_eq!(list_products(pool, &ProductFilter { search: Some("arroz".into()), ..Default::default() }).await.unwrap()[0].stock, 97);
    open_cash(pool, 2_000).await.unwrap(); // segunda sesión permitida tras cerrar
    cancel_sale(pool, &s1.summary.id, "devolución").await.unwrap();
    assert_eq!(exp().await, 2_000 - 1200, "el contra-movimiento va a la caja abierta");
    let old = session_detail(pool, &id).await.unwrap();
    assert_eq!(old.expected_cash_cents, 11_400, "el cierre anterior no cambia");
    assert_eq!(old.session.difference_cents, Some(-400));
    let _ = s2;

    assert!(stock_divergences(pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn phase5_sqlite() {
    run_all(&sqlite_mem().await).await;
}

#[tokio::test]
async fn phase5_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    match mysql_env().await {
        Some(db) => run_all(&db).await,
        None => eprintln!("MYSQL_TEST_URL no definida: se omite"),
    }
}
