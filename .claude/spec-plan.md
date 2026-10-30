# Plan de Fases: Mi Bodega V1

> Generado: 2026-10-30. Última actualización: 2026-10-30.
> Este archivo es la fuente de verdad del progreso. Actualízalo al
> cerrar o avanzar cada fase.
> Spec funcional: `spec/mi_bodega_v1_spec.md` (RF/RN/HU se referencian abajo).

## Información General
- **Stack**: Tauri 2 + React + TypeScript (UI) · Rust (dominio/casos de uso) · sqlx · SQLite (default) · MySQL (opcional, seleccionable en configuración). **Override del spec**: reemplaza Flutter/Drift; sin Android en V1.
- **Usuarios**: dueño/operador único de bodega pequeña en Perú, 1 dispositivo.
- **Restricciones**: offline-first, dinero en céntimos (i64), UUID, transacciones atómicas, build Linux (.deb/.AppImage) y Windows (.msi/.exe nsis).
- **Timeline estimado**: ~5 semanas (6 fases de 3-5 días).

### Decisiones técnicas (supuestos — confirmar)
1. **MySQL = backend alternativo de datos**, no sync. La app se conecta a SQLite local (default) o a un servidor MySQL configurado (host/user/db). Sigue siendo 1 caja activa; sin multiusuario en V1.
2. **sqlx** con dos carpetas de migraciones (`migrations/sqlite`, `migrations/mysql`) y repositorios detrás de traits; SQL escrito para ambos dialectos (sin features exclusivos). CI corre la suite contra ambos.
3. **Respaldo/restauración para ambos motores**: formato propio agnóstico (`.mibodega-backup` = zip con `manifest.json` {schema_version, fecha, installation_id, motor_origen, SHA-256 por tabla} + NDJSON por tabla). Se exporta desde SQLite o MySQL y se restaura en cualquiera (sirve también para migrar entre motores). Extra SQLite: copia nativa `VACUUM INTO` (.db). No depende de `mysqldump`.
4. **Lógica de negocio solo en Rust**; React no calcula dinero. Comandos Tauri tipados (tauri-specta o ts-rs para generar tipos).
5. **UI**: React + Vite + Tailwind + TanStack Query + Zustand/Router. Atajos de teclado y lector de código de barras (emula teclado).
6. **Windows**: se compila en runner `windows-latest` de GitHub Actions (no cross-compile desde Linux). Linux local para desarrollo.
7. **Stock**: `stock_balances` materializado + kardex, actualizado en la misma transacción; test de invariante.

## Estado Actual
- **Fase activa**: Fase 6 — Respaldo y empaquetado (EN PROGRESO). Fases 1-5 implementadas y probadas automáticamente; falta recorrido manual en pantalla
- **Progreso**: 1 de 6 fases completadas (Fase 1); Fases 2-6 implementadas y con pruebas automáticas, pendientes de validación manual
- **Bloqueadores**: ninguno (alcance MySQL y backup para ambos motores confirmados)
- **Próximo paso concreto**: recorrido manual completo en pantalla (crear producto → comprar → abrir caja → vender → cerrar → reportes → exportar/restaurar respaldo) y corregir lo que falle; luego build de Windows y smoke en Windows real. Antes de distribuir: cifrado de respaldos y clave MySQL en llavero.

---

## Fase 1: Base técnica y arquitectura dual-DB — COMPLETADA

**Duración estimada**: 4-5 días
**Objetivo**: Esqueleto Tauri+React que arranca en Linux y Windows, con capa de datos abstracta SQLite/MySQL, migraciones y configuración inicial.
**Complejidad**: Alto

### Requisitos previos
- Rust 1.99 + Node 24 + pnpm (ya instalados). Dependencias de sistema Tauri en Linux (webkit2gtk-4.1, libsoup3, etc.).
- Docker o MySQL local para pruebas.

### Entregables (Definición de Done)

#### Backend (Rust)
- [x] Proyecto Tauri 2 + estructura `domain/` (entidades, errores), `services/` (casos de uso), `repo/` (traits), `db/sqlite`, `db/mysql`
- [x] Pool sqlx con `PRAGMA foreign_keys=ON`, WAL en SQLite; selección de motor desde `config.json` (app data dir)
- [x] Migraciones completas del modelo §6 en ambos dialectos (UUID como TEXT/CHAR(36), montos BIGINT, timestamps UTC)
- [x] Tipo `Money(i64)` y helper de transacciones (`with_tx`)
- [x] Comandos: `get_settings`, `init_business`, `test_db_connection`, `set_db_config`
- [x] Errores de dominio serializables hacia la UI

#### Frontend
- [x] Vite + React + TS + Tailwind + Router + TanStack Query
- [x] Layout con navegación principal (8 secciones §4), tema táctil (botones grandes)
- [x] Pantalla Configuración inicial (negocio, stock mínimo, motor de BD: SQLite/MySQL con prueba de conexión)
- [x] ~~Generación de tipos TS desde Rust~~ (descartado: tipos escritos a mano en `src/lib/api.ts`; el costo de ts-rs/specta no compensa en este tamaño)

#### Tests
- [x] Migraciones aplican limpio en SQLite y MySQL
- [x] Test de rollback de transacción en ambos motores
- [x] Seed de desarrollo (`examples/productos.csv` + importación)

#### CI / Build
- [x] GitHub Actions: matriz ubuntu + windows (`tauri build`), servicio MySQL para tests (workflow listo, solo manual hasta el final; aún sin ejecutar)

#### Documentación
- [x] README: requisitos, `pnpm tauri dev`, cómo cambiar de motor
- [x] ADR corto: `docs/adr-0001-sqlx-any-dual-db.md`

### Posibles obstáculos
- Diferencias SQL SQLite/MySQL (UPSERT, tipos, `RETURNING`) → limitar a SQL común; encapsular lo distinto en el repo.
- Deps de webkit en Linux → script de setup documentado.
- sqlx offline mode (`sqlx prepare`) con dos motores → usar queries no-macro o `Any` con cuidado.

### Notas de avance
- 2026-10-30: scaffold Tauri 2 + React 19 + Tailwind 4, sqlx 0.9 (`Any`), migraciones SQLite/MySQL idénticas en esquema, tests de FK/rollback/única caja abierta pasan en SQLite y en MySQL 8.4 (Docker). CI en `.github/workflows/ci.yml` (sin ejecutar aún).
- Pendiente de la fase: tipos TS autogenerados, traits de repos (`repo/`), seed de desarrollo, `pnpm tauri dev` manual, ADR separado, `set_db_config` ya persiste config (clave MySQL en JSON plano: mejorar con keyring).
- Cambió el comando `get_settings` por `get_boot_state` (incluye settings).
- sqlx 0.9 exige `AssertSqlSafe` para SQL dinámico.

---

## Fase 2: Catálogo e inventario — EN PROGRESO

**Duración estimada**: 4-5 días
**Objetivo**: RF-01 y RF-02. Productos, categorías, presentaciones, stock derivado, kardex, ajustes y alertas.
**Complejidad**: Medio

### Requisitos previos
- Fase 1 completada.

### Entregables (Definición de Done)

#### Backend
- [x] Repos/servicios: categorías, productos, presentaciones (factor entero > 0)
- [x] Unicidad SKU/barcode, desactivación sin borrado si hay movimientos
- [x] `stock_movements` + `stock_balances` actualizados en misma tx; tipos AJUSTE_ENTRADA/SALIDA
- [x] Ajuste manual con motivo obligatorio y antes/después
- [x] Búsqueda por nombre/SKU/barcode + filtro categoría; lista de stock bajo
- [x] Importación CSV de productos con validación por fila (reporte de errores)
- [x] Comandos Tauri correspondientes

#### Frontend
- [x] Productos: lista con búsqueda, formulario, editor de presentaciones
- [x] Inventario: stock, kardex por producto/rango, diálogo de ajuste
- [x] Importar CSV con vista previa de errores

#### Tests
- [x] Unit: validaciones, SKU duplicado, factor inválido
- [x] Integración (ambos motores): invariante `stock_balances == SUM(kardex)`
- [x] Stock inicial vía `AJUSTE_ENTRADA / STOCK_INICIAL`

#### Documentación
- [x] Formato CSV de importación (ejemplo en `examples/productos.csv`)

### Posibles obstáculos
- Cantidades fraccionarias fuera de V1 → solo enteros en unidades base.

### Notas de avance
- 2026-10-30: backend en `catalog.rs` / `inventory.rs`; único punto de cambio de stock = `inventory::apply_movement` (UPDATE condicional atómico, bloquea negativo). Migración 0002 agrega `balance_before/after` y `seq` (orden estable del kardex).
- Tests `phase2_tests` pasan en SQLite y MySQL 8.4. Hallazgos: sqlx `Any` no decodifica `TINYINT` (MySQL usa BIGINT en flags); `SUM` en MySQL es DECIMAL → `CAST(... AS SIGNED)`; el dry-run de CSV usa una sola tx para detectar duplicados entre filas.
- UI: Productos (CRUD, presentaciones, importar CSV con vista previa) e Inventario (stock, solo por reponer, kardex, ajuste con motivo). Falta validación manual en pantalla y exportar CSV (va en Fase 5).

---

## Fase 3: Proveedores y compras — EN PROGRESO

**Duración estimada**: 3 días
**Objetivo**: RF-03. Recepción de mercadería que incrementa stock una sola vez y actualiza costo de referencia.
**Complejidad**: Medio

### Requisitos previos
- Fase 2 completada.

### Entregables (Definición de Done)

#### Backend
- [x] CRUD proveedores
- [x] Compra: borrador → confirmada; numeración transaccional `C-000001`
- [x] Confirmación idempotente: movimientos COMPRA, `factor_snapshot`, último costo → `reference_cost_cents`
- [x] Compra confirmada inmutable (sin edición)

#### Frontend
- [x] Proveedores (lista/form)
- [x] Compra nueva (líneas con presentación, costo, total), historial y detalle

#### Tests
- [x] Caja de 24 comprada → stock +24 (prueba spec #1 parte 1)
- [x] Doble confirmación → una sola entrada
- [x] Cambio de costo no reescribe histórico

#### Documentación
- [ ] Flujo C en guía de usuario (borrador)

### Posibles obstáculos
- Numeración concurrente en MySQL → contador en tabla con `SELECT ... FOR UPDATE`; en SQLite tx inmediata.

### Notas de avance
- 2026-10-30: `purchases.rs` (proveedores + compras). Numeración `C-000001` por contador al crear el borrador (los descartados dejan hueco). Confirmación con `UPDATE ... WHERE status='DRAFT'` (una sola gana) + `apply_movement` COMPRA por línea + costo de referencia = costo de línea / factor, redondeo half-up en enteros (`cost_per_base_unit`).
- Decisiones: borrador → confirmada; descartar borrador = CANCELED; anulación de compra confirmada pospuesta (el spec lo permite; corregir con ajuste). Proveedor con compras solo se desactiva.
- Tests `phase3_tests` pasan en SQLite y MySQL 8.4. UI en Compras/Proveedores (tabs). Falta validación manual en pantalla.

---

## Fase 4: Ventas y cobros — EN PROGRESO

**Duración estimada**: 5 días
**Objetivo**: RF-04. Vender en segundos: carrito, descuentos, pagos mixtos, vuelto, anulación, comprobante interno.
**Complejidad**: Alto

### Requisitos previos
- Fase 3 completada. Stub de "caja abierta" (la sesión real llega en Fase 5; aquí se crea apertura mínima para poder vender).

### Entregables (Definición de Done)

#### Backend
- [x] `confirm_sale` atómico: cabecera, líneas (snapshots precio/factor/costo), pagos, stock, movimiento de efectivo
- [x] Validaciones: stock suficiente (RN-04), pagos cuadran (RN-06), descuento no negativo, caja abierta
- [x] Idempotencia por `client_request_id` único (RN-05)
- [x] Métodos EFECTIVO/YAPE/PLIN/OTRO; vuelto solo en efectivo
- [x] Anulación: movimientos inversos exactamente una vez, motivo, respeta caja cerrada (RN-11)
- [x] Numeración `V-000001`

#### Frontend
- [x] Vender: buscador con foco permanente, captura de lector de código, carrito, selector de presentación, atajos (F-keys/Enter)
- [x] Modal de pago (mixto + vuelto), botón de confirmar bloqueado tras clic
- [x] Venta realizada + ticket interno imprimible (HTML/PDF vía print)
- [x] Historial de ventas con filtros, detalle y anulación

#### Tests
- [x] Spec #1-#5, #7, #8, #12 automatizados
- [x] Error simulado en medio de venta → rollback total
- [x] Rendimiento: agregar al carrito < 1 s con 5k productos

#### Documentación
- [ ] Atajos de teclado y flujo B (guía de usuario)

### Posibles obstáculos
- Doble toque → bloqueo UI + idempotencia en BD.
- Redondeo de descuentos por línea → reglas explícitas en enteros, tests de borde.

### Notas de avance
- 2026-10-30: `sales.rs` + `cash.rs` (caja mínima: abrir, saldo esperado, movimientos; cierre y movimientos manuales quedan para Fase 5). Precio/factor/costo se leen de la BD al confirmar (la UI no dicta precios).
- Idempotencia: `client_request_id` UNIQUE + chequeo previo; repetir devuelve la misma venta. La UI conserva la clave si falla (reintento seguro) y genera otra al completar.
- Pagos: `amount_cents` = neto asignado; `received/change` solo efectivo. Solo EFECTIVO crea `cash_movements`; Yape/Plin quedan en `sale_payments` para reportes. Anulación revierte solo efectivo en la caja abierta actual (RN-11) y exige caja abierta si hubo efectivo.
- Descuentos: por línea (`sale_items.discount_cents`) y total (`sales.discount_cents`); subtotal = suma de líneas netas. Margen (Fase 5) usará `line_total` sin prorratear el descuento total.
- Rendimiento: búsqueda con 5.000 productos ≈ 17 ms (se acotó la carga de presentaciones a los productos devueltos).
- Tests `phase4_tests` en SQLite y MySQL 8.4: spec #1-#5, #7, #8, #12 + descuentos, producto inactivo, rollback multilínea.
- UI: Vender (búsqueda/lector + Enter, carrito, F2/F9, cobro mixto con vuelto y billetes rápidos, ticket interno imprimible) e Historial de ventas con anulación.
- Pendiente: validación manual en pantalla; impresión usa `window.print()` (revisar en Windows).

---

## Fase 5: Caja y reportes — EN PROGRESO

**Duración estimada**: 4 días
**Objetivo**: RF-05 y RF-06. Sesión de caja completa, cierre inmutable, dashboard y reportes con export CSV.
**Complejidad**: Medio

### Requisitos previos
- Fase 4 completada.

### Entregables (Definición de Done)

#### Backend
- [x] Apertura/cierre, única sesión `OPEN` (índice único parcial / validación por tx en MySQL)
- [x] Ingresos/egresos/retiros manuales; esperado = fórmula RF-05
- [x] Cierre con contado, diferencia, observación; inmutable
- [x] Consultas de reportes: ventas por día/rango, métodos, anulaciones, top productos, stock bajo, margen bruto estimado
- [x] Exportación CSV: productos, kardex, ventas

#### Frontend
- [x] Inicio con KPIs y stock bajo
- [x] Caja: abrir, movimientos, cierre, historial
- [x] Reportes con rango de fechas y botón exportar (diálogo nativo de guardado)

#### Tests
- [x] Spec #5, #6 (pago mixto, diferencia en cierre)
- [x] Totales del día == ventas válidas netas de anulaciones (HU-08)
- [x] Zona horaria: UTC en BD, local en UI

#### Documentación
- [ ] Flujo D, definición de margen estimado (guía de usuario)

### Posibles obstáculos
- Agregaciones por día con zona local distinta entre SQLite/MySQL → convertir rangos en Rust a UTC antes de consultar.

### Notas de avance
- 2026-10-30: `cash.rs` (movimientos manuales, cierre, historial, detalle) y `reports.rs` (inicio, ventas, más vendidos, exportaciones). Cierre = UPDATE condicional (`status='OPEN'`) que congela esperado/contado/diferencia y libera `open_flag`; sesión cerrada no admite movimientos, ventas ni segundo cierre. Egreso/retiro en efectivo no puede dejar el esperado en negativo.
- RN-11 verificado: anular una venta en efectivo de un periodo cerrado exige caja abierta y el contra-movimiento va a la sesión abierta; el cierre anterior no cambia.
- Reportes: ventas por día se agrupan en Rust con la hora LOCAL (independiente del motor); anuladas se informan aparte; margen estimado = Σ line_total − Σ costo_snapshot×unidades base (sin prorratear descuento total). Los más vendidos indican la métrica de orden.
- CSV: BOM UTF-8, separador coma, punto decimal (Excel es-PE); la plantilla de importación también pasó a coma. Guardado con diálogo nativo.
- Hallazgo MySQL: `before` es palabra reservada como alias (renombrado a `bal_before`).
- Tests `phase5_tests` en SQLite y MySQL 8.4 (14 pruebas en total). Falta validación manual en pantalla.

---

## Fase 6: Respaldo, endurecimiento y empaquetado — EN PROGRESO

**Duración estimada**: 4-5 días
**Objetivo**: RF-07 y entrega: export/restore de BD (SQLite y MySQL), pruebas de fallos, instaladores Linux y Windows, manual.
**Complejidad**: Alto

### Requisitos previos
- Fases 1-5 completadas.

### Entregables (Definición de Done)

#### Backend
- [x] Export agnóstico de motor: snapshot consistente (tx de lectura / `REPEATABLE READ` en MySQL), zip con manifiesto + NDJSON por tabla + SHA-256
- [x] Export extra SQLite nativo (`VACUUM INTO`)
- [x] Restore: valida checksum, versión de esquema, FKs e invariante stock vs kardex; backup previo automático; carga en tx única sobre BD vacía/nueva y reapertura del pool; permite restaurar en el otro motor
- [ ] Backup automático opcional al cerrar caja
- [x] Migraciones versionadas y prueba de upgrade con datos
- [ ] (Opcional) PIN de apertura

#### Frontend
- [x] Configuración → Exportar respaldo (diálogo nativo de guardado) / Restaurar con advertencias y confirmación explícita
- [x] Aviso de pérdida de datos sin respaldo

#### Tests
- [x] Spec #9 (respaldo corrupto no daña BD), #10, #11
- [x] Respaldo restaurado reproduce ventas, inventario y caja
- [x] Round-trip SQLite→MySQL y MySQL→SQLite sin pérdida
- [ ] Smoke manual en Linux y Windows reales
- [x] Invariantes DoD: stock vs kardex, ventas congruentes

#### Build / Entrega
- [ ] Workflow de release: Linux (.deb, .AppImage) y Windows (.msi/NSIS) como artefactos
- [x] Icono, nombre, firma opcional; ubicación de datos por plataforma documentada
- [x] Manual corto del comerciante

#### Documentación
- [x] Build instructions, procedimiento de actualización de versión

### Posibles obstáculos
- WebView2 en Windows antiguos → bootstrapper en instalador.
- Reemplazar archivo SQLite con pool abierto → cerrar pool antes y reabrir.
- Tablas grandes en export → streaming NDJSON, sin cargar todo en memoria.

### Notas de avance
- 2026-10-30: `backup.rs` — formato `.mibodega-backup` (zip: `manifest.json` + `data/<tabla>.ndjson` + SHA-256 por tabla), snapshot en una transacción de lectura, escritura a temporal + rename. Restauración: valida TODO antes de tocar la BD (formato, esquema ≤ actual, checksums, filas, columnas), respaldo previo en `<datos>/backups/pre-restore-*.mibodega-backup`, reemplazo en una sola tx (FK + saldo == kardex o rollback). No hace falta reabrir el pool (no se reemplaza el archivo, se reescriben filas).
- Tests (23 en total, SQLite y MySQL 8.4): ida y vuelta, corrupto/basura/truncado/FK rota sin cambios en la BD, esquema más nuevo, SQLite→MySQL→SQLite idéntico, `TABLES` == columnas reales del esquema, actualización de migración 1→2 con datos.
- Pendiente: backup automático al cerrar caja y PIN (ambos opcionales, diferidos); streaming del export (hoy una tabla entera en memoria: suficiente para catálogos pequeños/medianos); smoke manual en Linux y Windows; build de Windows (necesita `nsis` instalado o runner de GitHub); cifrado del respaldo con contraseña (spec lo exige **antes de lanzamiento comercial**); la clave de MySQL sigue en `config.json` en texto plano (mover al llavero).

---

## Timeline Total
- Fase 1: 4-5 días
- Fase 2: 4-5 días
- Fase 3: 3 días
- Fase 4: 5 días
- Fase 5: 4 días
- Fase 6: 4-5 días
**TOTAL: ~5 semanas**

## Riesgos Identificados
1. Mantener paridad SQLite/MySQL → SQL común, tests en CI contra ambos desde Fase 1.
2. Concurrencia/numeración en MySQL → locks explícitos y tests de doble confirmación.
3. Alcance de "soporte MySQL" ambiguo → supuesto 1; confirmar antes de Fase 1.
4. Build Windows sin máquina local → GitHub Actions; validar temprano (final de Fase 1).
5. Dependencias webkit/WebView2 → documentar y probar instaladores en limpio.
6. Rendimiento del flujo de venta → índices y búsqueda en Rust, no filtrar en React.

## Historial de Cambios
- 2026-10-30: Fase 6 implementada (respaldo/restauración dual-motor, docs, manual); Fase 1 cerrada.
- 2026-10-30: Fase 5 implementada (cierre de caja, movimientos, reportes, exportación CSV, inicio).
- 2026-10-30: Fase 4 implementada (ventas, pagos mixtos, anulación, ticket interno, historial).
- 2026-10-30: Fase 3 implementada (proveedores, compras, confirmación idempotente); plantilla CSV de productos y CRUD de categorías añadidos a Fase 2.
- 2026-10-30: Fase 2 implementada (backend probado en ambos motores + UI); pendiente validación manual.
- 2026-10-30: Fase 1 en progreso (base técnica, migraciones duales, setup UI, CI).
- 2026-10-30: Confirmado alcance MySQL; backup in-app ahora cubre SQLite y MySQL (formato agnóstico).
- 2026-10-30: Plan creado con 6 fases (adaptado de Flutter a Tauri+React+Rust, MySQL opcional).
