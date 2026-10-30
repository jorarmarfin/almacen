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
- **Fase activa**: Fase 2 — Catálogo e inventario (EN PROGRESO); Fase 1 casi cerrada
- **Progreso**: 0 de 6 fases completadas
- **Bloqueadores**: ninguno (alcance MySQL y backup para ambos motores confirmados)
- **Próximo paso concreto (Fase 2)**: probar Productos/Inventario en la app (`./scripts/run.sh`, importar `examples/productos.csv`), pulir UX y cerrar fase. Pendientes Fase 1 (no bloquean): tipos TS autogenerados, ADR, seed, keyring. _Anterior:_ lanzar `pnpm tauri dev` y probar Setup (SQLite y MySQL); luego generación de tipos TS (ts-rs/specta), trait de repos y seed; validar CI en GitHub (build Windows).

---

## Fase 1: Base técnica y arquitectura dual-DB — EN PROGRESO

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
- [ ] Generación de tipos TS desde Rust

#### Tests
- [x] Migraciones aplican limpio en SQLite y MySQL
- [x] Test de rollback de transacción en ambos motores
- [ ] Seed de desarrollo

#### CI / Build
- [ ] GitHub Actions: matriz ubuntu + windows (`tauri build`), servicio MySQL para tests

#### Documentación
- [x] README: requisitos, `pnpm tauri dev`, cómo cambiar de motor
- [ ] ADR corto: decisión sqlx + dual-dialect

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

## Fase 3: Proveedores y compras — PENDIENTE

**Duración estimada**: 3 días
**Objetivo**: RF-03. Recepción de mercadería que incrementa stock una sola vez y actualiza costo de referencia.
**Complejidad**: Medio

### Requisitos previos
- Fase 2 completada.

### Entregables (Definición de Done)

#### Backend
- [ ] CRUD proveedores
- [ ] Compra: borrador → confirmada; numeración transaccional `C-000001`
- [ ] Confirmación idempotente: movimientos COMPRA, `factor_snapshot`, último costo → `reference_cost_cents`
- [ ] Compra confirmada inmutable (sin edición)

#### Frontend
- [ ] Proveedores (lista/form)
- [ ] Compra nueva (líneas con presentación, costo, total), historial y detalle

#### Tests
- [ ] Caja de 24 comprada → stock +24 (prueba spec #1 parte 1)
- [ ] Doble confirmación → una sola entrada
- [ ] Cambio de costo no reescribe histórico

#### Documentación
- [ ] Flujo C en guía de usuario (borrador)

### Posibles obstáculos
- Numeración concurrente en MySQL → contador en tabla con `SELECT ... FOR UPDATE`; en SQLite tx inmediata.

### Notas de avance
-

---

## Fase 4: Ventas y cobros — PENDIENTE

**Duración estimada**: 5 días
**Objetivo**: RF-04. Vender en segundos: carrito, descuentos, pagos mixtos, vuelto, anulación, comprobante interno.
**Complejidad**: Alto

### Requisitos previos
- Fase 3 completada. Stub de "caja abierta" (la sesión real llega en Fase 5; aquí se crea apertura mínima para poder vender).

### Entregables (Definición de Done)

#### Backend
- [ ] `confirm_sale` atómico: cabecera, líneas (snapshots precio/factor/costo), pagos, stock, movimiento de efectivo
- [ ] Validaciones: stock suficiente (RN-04), pagos cuadran (RN-06), descuento no negativo, caja abierta
- [ ] Idempotencia por `client_request_id` único (RN-05)
- [ ] Métodos EFECTIVO/YAPE/PLIN/OTRO; vuelto solo en efectivo
- [ ] Anulación: movimientos inversos exactamente una vez, motivo, respeta caja cerrada (RN-11)
- [ ] Numeración `V-000001`

#### Frontend
- [ ] Vender: buscador con foco permanente, captura de lector de código, carrito, selector de presentación, atajos (F-keys/Enter)
- [ ] Modal de pago (mixto + vuelto), botón de confirmar bloqueado tras clic
- [ ] Venta realizada + ticket interno imprimible (HTML/PDF vía print)
- [ ] Historial de ventas con filtros, detalle y anulación

#### Tests
- [ ] Spec #1-#5, #7, #8, #12 automatizados
- [ ] Error simulado en medio de venta → rollback total
- [ ] Rendimiento: agregar al carrito < 1 s con 5k productos

#### Documentación
- [ ] Atajos de teclado y flujo B

### Posibles obstáculos
- Doble toque → bloqueo UI + idempotencia en BD.
- Redondeo de descuentos por línea → reglas explícitas en enteros, tests de borde.

### Notas de avance
-

---

## Fase 5: Caja y reportes — PENDIENTE

**Duración estimada**: 4 días
**Objetivo**: RF-05 y RF-06. Sesión de caja completa, cierre inmutable, dashboard y reportes con export CSV.
**Complejidad**: Medio

### Requisitos previos
- Fase 4 completada.

### Entregables (Definición de Done)

#### Backend
- [ ] Apertura/cierre, única sesión `OPEN` (índice único parcial / validación por tx en MySQL)
- [ ] Ingresos/egresos/retiros manuales; esperado = fórmula RF-05
- [ ] Cierre con contado, diferencia, observación; inmutable
- [ ] Consultas de reportes: ventas por día/rango, métodos, anulaciones, top productos, stock bajo, margen bruto estimado
- [ ] Exportación CSV: productos, kardex, ventas

#### Frontend
- [ ] Inicio con KPIs y stock bajo
- [ ] Caja: abrir, movimientos, cierre, historial
- [ ] Reportes con rango de fechas y botón exportar (diálogo nativo de guardado)

#### Tests
- [ ] Spec #5, #6 (pago mixto, diferencia en cierre)
- [ ] Totales del día == ventas válidas netas de anulaciones (HU-08)
- [ ] Zona horaria: UTC en BD, local en UI

#### Documentación
- [ ] Flujo D, definición de margen estimado

### Posibles obstáculos
- Agregaciones por día con zona local distinta entre SQLite/MySQL → convertir rangos en Rust a UTC antes de consultar.

### Notas de avance
-

---

## Fase 6: Respaldo, endurecimiento y empaquetado — PENDIENTE

**Duración estimada**: 4-5 días
**Objetivo**: RF-07 y entrega: export/restore de BD (SQLite y MySQL), pruebas de fallos, instaladores Linux y Windows, manual.
**Complejidad**: Alto

### Requisitos previos
- Fases 1-5 completadas.

### Entregables (Definición de Done)

#### Backend
- [ ] Export agnóstico de motor: snapshot consistente (tx de lectura / `REPEATABLE READ` en MySQL), zip con manifiesto + NDJSON por tabla + SHA-256
- [ ] Export extra SQLite nativo (`VACUUM INTO`)
- [ ] Restore: valida checksum, versión de esquema, FKs e invariante stock vs kardex; backup previo automático; carga en tx única sobre BD vacía/nueva y reapertura del pool; permite restaurar en el otro motor
- [ ] Backup automático opcional al cerrar caja
- [ ] Migraciones versionadas y prueba de upgrade con datos
- [ ] (Opcional) PIN de apertura

#### Frontend
- [ ] Configuración → Exportar respaldo (diálogo nativo de guardado) / Restaurar con advertencias y confirmación explícita
- [ ] Aviso de pérdida de datos sin respaldo

#### Tests
- [ ] Spec #9 (respaldo corrupto no daña BD), #10, #11
- [ ] Respaldo restaurado reproduce ventas, inventario y caja
- [ ] Round-trip SQLite→MySQL y MySQL→SQLite sin pérdida
- [ ] Smoke manual en Linux y Windows reales
- [ ] Invariantes DoD: stock vs kardex, ventas congruentes

#### Build / Entrega
- [ ] Workflow de release: Linux (.deb, .AppImage) y Windows (.msi/NSIS) como artefactos
- [ ] Icono, nombre, firma opcional; ubicación de datos por plataforma documentada
- [ ] Manual corto del comerciante

#### Documentación
- [ ] Build instructions, procedimiento de actualización de versión

### Posibles obstáculos
- WebView2 en Windows antiguos → bootstrapper en instalador.
- Reemplazar archivo SQLite con pool abierto → cerrar pool antes y reabrir.
- Tablas grandes en export → streaming NDJSON, sin cargar todo en memoria.

### Notas de avance
-

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
- 2026-10-30: Fase 2 implementada (backend probado en ambos motores + UI); pendiente validación manual.
- 2026-10-30: Fase 1 en progreso (base técnica, migraciones duales, setup UI, CI).
- 2026-10-30: Confirmado alcance MySQL; backup in-app ahora cubre SQLite y MySQL (formato agnóstico).
- 2026-10-30: Plan creado con 6 fases (adaptado de Flutter a Tauri+React+Rust, MySQL opcional).
