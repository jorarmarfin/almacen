# Mi Bodega

Aplicación de escritorio **offline** de inventario, compras, ventas y caja para bodegas y tiendas pequeñas de Perú. Funciona sin internet, en un solo equipo, con base de datos **SQLite** (por defecto) o **MySQL**. Compila para **Linux** y **Windows**.

**Stack:** Tauri 2 · React 19 + TypeScript + Tailwind 4 · Rust · sqlx (`Any`) · SQLite / MySQL

- Especificación funcional: [`spec/mi_bodega_v1_spec.md`](spec/mi_bodega_v1_spec.md)
- Plan de fases y estado del avance: [`.claude/spec-plan.md`](.claude/spec-plan.md)

## Funciones

| Módulo | Qué incluye |
|---|---|
| Productos y categorías | CRUD, SKU automático, código de barras, presentaciones (unidad, caja x24…), importar CSV con plantilla descargable |
| Inventario | Stock derivado de movimientos, kardex, ajustes con motivo, alerta de stock bajo, sin stock negativo |
| Compras y proveedores | Borrador → confirmada, costo de referencia, una sola entrada de stock por compra |
| Ventas | Búsqueda o lector de códigos, descuentos, pagos mixtos (efectivo, Yape, Plin, otro), vuelto, ticket interno, anulación con motivo |
| Caja | Apertura, ingresos/egresos/retiros, cierre con diferencia, historial |
| Reportes | Ventas por rango, método y día, más vendidos, por reponer, margen bruto estimado, exportación CSV |
| Respaldo | Exportar/restaurar `.mibodega-backup` (con contraseña opcional), respaldo automático al cerrar caja, válido entre SQLite y MySQL |

## Estructura del proyecto

```
.
├── src/                    Frontend React (pages/, components/, lib/)
├── src-tauri/              Backend Rust + configuración de Tauri
│   ├── src/                Código Rust (ver tabla abajo)
│   ├── migrations/         Esquema: sqlite/ y mysql/ (mismo número en ambos)
│   ├── capabilities/       Permisos de Tauri
│   └── tauri.conf.json     Ventana y empaquetado
├── scripts/                run.sh, build-linux.sh, build-windows.sh
├── docs/                   Manual, guía de actualización, ADR
├── spec/                   Especificaciones
├── examples/               CSV de ejemplo para importar
├── branding/               Logo original e icono de la app
└── .github/workflows/      CI (solo manual por ahora)
```

### Backend (`src-tauri/src/`)

| Archivo | Responsabilidad |
|---|---|
| `lib.rs` / `main.rs` | Arranque, plugins y registro de comandos |
| `commands.rs` | Comandos Tauri llamados por el frontend (capa fina) |
| `db.rs` | Pool, migraciones y selección de motor |
| `config.rs` | `config.json`, motor elegido, clave de MySQL en el llavero |
| `catalog.rs` | Categorías, productos, presentaciones, importación CSV |
| `inventory.rs` | Stock y kardex; único punto que modifica existencias |
| `purchases.rs` | Proveedores y compras |
| `sales.rs` | Ventas, pagos y anulación |
| `cash.rs` | Sesiones de caja y movimientos |
| `reports.rs` | Reportes y exportaciones CSV |
| `backup.rs` | Respaldo y restauración, cifrado, respaldo automático |
| `domain/money.rs`, `util.rs`, `error.rs` | Dinero en céntimos, utilidades, errores |
| `phase*_tests.rs` | Pruebas de cada fase |

## Requisitos

- Rust estable y Node 24 con pnpm
- **Linux:** `libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- **Windows:** MSVC y WebView2, o compilar con el workflow de GitHub (`windows-latest`)
- Opcional, para probar MySQL: Docker

## Desarrollo

```sh
./scripts/run.sh             # modo desarrollo con recarga en caliente
./scripts/build-linux.sh     # .deb, .rpm y .AppImage en src-tauri/target/release/bundle
./scripts/build-windows.sh   # cross-compila el instalador NSIS desde Linux (requiere nsis y cargo-xwin)
```

En Linux, `run.sh` y la app fijan `WEBKIT_DISABLE_DMABUF_RENDERER=1` (WebKitGTK cierra la ventana con algunos drivers de GPU sin esto).

### Pruebas

```sh
cargo test --manifest-path src-tauri/Cargo.toml      # SQLite

# Con MySQL real (contenedor desechable):
docker run -d --rm --name mb-mysql -e MYSQL_ROOT_PASSWORD=test -e MYSQL_DATABASE=mibodega_test -p 127.0.0.1:3399:3306 mysql:8.4
MYSQL_TEST_URL=mysql://root:test@127.0.0.1:3399/mibodega_test cargo test --manifest-path src-tauri/Cargo.toml
```

Sin `MYSQL_TEST_URL`, las pruebas de MySQL se omiten en silencio: antes de dar una fase por buena, córrelas con MySQL.

### Migraciones

Cada cambio de esquema va en **ambos** dialectos con el mismo número, se sube `SCHEMA_VERSION` en `db.rs` y se actualiza `backup::TABLES` (un test comprueba que cubra todas las columnas). Nunca se edita una migración ya publicada. Más en [`docs/actualizacion.md`](docs/actualizacion.md).

## Base de datos

En la pantalla inicial se elige el motor:

- **SQLite:** archivo `mibodega.db` en la carpeta de datos de la app (Configuración → Acerca de muestra la ruta).
- **MySQL:** host, puerto, usuario, clave y base. La base debe existir; las tablas se migran solas.

La elección se guarda en `config.json`. La clave de MySQL va al **llavero del sistema** (Secret Service en Linux, Credential Manager en Windows), no al archivo.

## Respaldo y seguridad

- Configuración → **Exportar respaldo**: zip `.mibodega-backup` con manifiesto y suma SHA-256 por tabla; se puede cifrar con contraseña (AES-256). Una contraseña olvidada no se puede recuperar.
- **Restaurar** valida todo antes de tocar la BD, guarda un respaldo previo y reemplaza los datos en una sola transacción. También sirve para migrar entre SQLite y MySQL.
- **Respaldo automático** al cerrar caja en una carpeta elegida (se conservan los 14 últimos; sin contraseña).
- Los datos viven solo en el equipo: sin respaldo, una falla del equipo los pierde.

## Decisiones de arquitectura

Detalle en [`docs/adr-0001-sqlx-any-dual-db.md`](docs/adr-0001-sqlx-any-dual-db.md).

- `sqlx::Any` con SQL común a ambos motores, sin repositorios por motor.
- Dinero en céntimos (`i64`), nunca con coma flotante. Timestamps UTC en texto RFC3339. IDs UUID.
- La lógica de negocio vive solo en Rust; React solo presenta.
- Una única caja abierta: `cash_sessions.open_flag` UNIQUE (1 abierta, NULL cerrada), portable a MySQL.
- Toda venta, compra o ajuste se registra en una sola transacción; el stock solo cambia por `inventory::apply_movement`.
- El comprobante de venta es un **ticket interno no tributario**: no es boleta ni factura electrónica.

## Documentación

- [`docs/manual-comerciante.md`](docs/manual-comerciante.md): manual de uso para el comerciante
- [`docs/actualizacion.md`](docs/actualizacion.md): actualizar versión y reglas de migraciones
- [`docs/adr-0001-sqlx-any-dual-db.md`](docs/adr-0001-sqlx-any-dual-db.md): decisión de la capa de datos

## Estado

Las 6 fases del plan están implementadas y con pruebas automáticas en SQLite y MySQL. Pendiente: recorrido manual en pantalla, compilación y prueba en Windows, PIN de apertura (opcional) y el workflow de release de GitHub, que queda sin ejecutar hasta el final del proyecto.
