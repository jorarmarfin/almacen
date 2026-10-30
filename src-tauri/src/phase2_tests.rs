//! Pruebas de catálogo e inventario; corren en SQLite y, si hay `MYSQL_TEST_URL`, en MySQL.
use crate::catalog::*;
use crate::db::tests::{mysql_env, sqlite_mem, MYSQL_LOCK};
use crate::db::Db;
use crate::error::AppError;
use crate::inventory::*;

fn input(name: &str) -> ProductInput {
    ProductInput {
        id: None,
        category_id: None,
        sku: None,
        barcode: None,
        name: name.into(),
        base_unit: "unidad".into(),
        reference_cost_cents: 100,
        min_stock: Some(5),
        presentations: vec![
            PresentationInput { id: None, name: "Unidad".into(), factor_to_base: 1, sale_price_cents: 150 },
            PresentationInput { id: None, name: "Caja x24".into(), factor_to_base: 24, sale_price_cents: 3000 },
        ],
    }
}

async fn run_all(db: &Db) {
    let pool = &db.pool;

    // Alta con SKU autogenerado + presentaciones + saldo en 0
    let id = save_product(pool, &input("Gaseosa 500ml")).await.unwrap();
    let list = list_products(pool, &ProductFilter::default()).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].sku, "P-000001");
    assert_eq!(list[0].stock, 0);
    assert_eq!(list[0].presentations.len(), 2);

    // Validaciones
    let mut bad = input("X");
    bad.presentations[0].factor_to_base = 0;
    assert!(matches!(save_product(pool, &bad).await, Err(AppError::Validation(_))));
    assert!(matches!(save_product(pool, &input("  ")).await, Err(AppError::Validation(_))));
    let mut neg = input("Y");
    neg.presentations[0].sale_price_cents = -1;
    assert!(save_product(pool, &neg).await.is_err());

    // SKU y código de barras únicos
    let mut dup = input("Otro");
    dup.sku = Some("P-000001".into());
    assert!(matches!(save_product(pool, &dup).await, Err(AppError::Validation(_))));
    let mut a = input("Con código");
    a.barcode = Some("7750001".into());
    save_product(pool, &a).await.unwrap();
    let mut b = input("Mismo código");
    b.barcode = Some("7750001".into());
    assert!(save_product(pool, &b).await.is_err());

    // Ajustes: entrada, salida, negativo bloqueado, motivo obligatorio
    assert_eq!(adjust_stock(pool, &id, AdjustKind::Entrada, 24, "STOCK_INICIAL").await.unwrap(), 24);
    assert_eq!(adjust_stock(pool, &id, AdjustKind::Salida, 3, "merma").await.unwrap(), 21);
    let r = adjust_stock(pool, &id, AdjustKind::Salida, 22, "x").await;
    assert!(matches!(r, Err(AppError::InsufficientStock { available: 21 })));
    assert!(adjust_stock(pool, &id, AdjustKind::Entrada, 1, "  ").await.is_err());
    assert!(adjust_stock(pool, &id, AdjustKind::Entrada, 0, "x").await.is_err());
    let k = kardex(pool, &id, None, None, 50).await.unwrap();
    assert_eq!(k.len(), 2, "el rechazo no deja movimientos");
    assert_eq!(k[0].balance_before, Some(24));
    assert_eq!(k[0].balance_after, Some(21));
    assert!(stock_divergences(pool).await.unwrap().is_empty());

    // Edición: factor 24 -> 12 y presentación ausente se desactiva, no se borra
    let p = list_products(pool, &ProductFilter { search: Some("gaseosa".into()), ..Default::default() }).await.unwrap().remove(0);
    let caja = p.presentations.iter().find(|x| x.factor_to_base == 24).unwrap();
    let mut edit = input("Gaseosa 500ml");
    edit.id = Some(id.clone());
    edit.presentations = vec![PresentationInput { id: Some(caja.id.clone()), name: "Caja x12".into(), factor_to_base: 12, sale_price_cents: 1600 }];
    save_product(pool, &edit).await.unwrap();
    let p = list_products(pool, &ProductFilter { search: Some("P-000001".into()), ..Default::default() }).await.unwrap().remove(0);
    assert_eq!(p.presentations.len(), 2);
    assert_eq!(p.presentations.iter().filter(|x| x.active).count(), 1);
    assert_eq!(p.stock, 21, "editar no toca stock");

    // Stock bajo y desactivación
    let low = list_products(pool, &ProductFilter { low_stock_only: true, ..Default::default() }).await.unwrap();
    assert_eq!(low.len(), 1, "solo el producto sin stock (0 <= 5)");
    set_product_active(pool, &id, false).await.unwrap();
    assert_eq!(list_products(pool, &ProductFilter::default()).await.unwrap().len(), 1);
    assert_eq!(list_products(pool, &ProductFilter { include_inactive: true, ..Default::default() }).await.unwrap().len(), 2);

    // Categorías
    let c = save_category(pool, None, "Bebidas", true).await.unwrap();
    assert!(save_category(pool, None, "Bebidas", true).await.is_err());
    save_category(pool, Some(&c), "Bebidas frías", true).await.unwrap();
    assert_eq!(list_categories(pool).await.unwrap()[0].name, "Bebidas frías");
    // eliminar: libre sí, con productos no
    delete_category(pool, &c).await.unwrap();
    let ab = save_category(pool, None, "Abarrotes2", true).await.unwrap();
    let mut with_cat = input("Con categoría");
    with_cat.category_id = Some(ab.clone());
    save_product(pool, &with_cat).await.unwrap();
    assert_eq!(list_categories(pool).await.unwrap().iter().find(|x| x.id == ab).unwrap().product_count, 1);
    assert!(delete_category(pool, &ab).await.is_err());

    // Importación CSV: dry-run no persiste; errores por fila
    let csv = "nombre,sku,categoria,costo,precio,stock_minimo,stock_inicial\n\
               Arroz 1kg,A-1,Abarrotes,3.50,4.20,10,50\n\
               ,A-2,Abarrotes,1,2,,\n\
               Azúcar,A-1,Abarrotes,2,3,,\n\
               Sal,A-3,Abarrotes,x,3,,\n";
    let before = list_products(pool, &ProductFilter { include_inactive: true, ..Default::default() }).await.unwrap().len();
    let dry = import_csv(pool, csv, true).await.unwrap();
    assert_eq!(dry.imported, 1);
    assert_eq!(list_products(pool, &ProductFilter { include_inactive: true, ..Default::default() }).await.unwrap().len(), before);
    let rep = import_csv(pool, csv, false).await.unwrap();
    assert_eq!(rep.imported, 1);
    assert!(!rep.rows[1].ok && !rep.rows[2].ok && !rep.rows[3].ok);
    let arroz = list_products(pool, &ProductFilter { search: Some("arroz".into()), ..Default::default() }).await.unwrap().remove(0);
    assert_eq!(arroz.stock, 50);
    assert_eq!(arroz.reference_cost_cents, 350);
    assert_eq!(arroz.presentations[0].sale_price_cents, 420);
    assert_eq!(arroz.category_name.as_deref(), Some("Abarrotes"));

    // Invariante global saldo == kardex
    assert!(stock_divergences(pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn phase2_sqlite() {
    run_all(&sqlite_mem().await).await;
}

#[tokio::test]
async fn phase2_mysql() {
    let _g = MYSQL_LOCK.lock().await;
    match mysql_env().await {
        Some(db) => run_all(&db).await,
        None => eprintln!("MYSQL_TEST_URL no definida: se omite"),
    }
}
