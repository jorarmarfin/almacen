# Mi Bodega

App de escritorio offline-first de inventario, ventas y caja. Tauri 2 + React + Rust + sqlx. Motor de datos: SQLite (default) o MySQL. Linux y Windows.

Spec: `spec/mi_bodega_v1_spec.md` · Plan de fases: `.claude/spec-plan.md`

## Requisitos
- Rust estable, Node 24, pnpm
- Linux: `libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- Windows: se compila en CI (`windows-latest`) o localmente con MSVC + WebView2

## Scripts
```sh
./scripts/run.sh             # desarrollo (hot reload)
./scripts/build-linux.sh     # .deb + .AppImage
./scripts/build-windows.sh   # cross-compila .exe NSIS desde Linux (cargo-xwin + nsis)
```
CI de GitHub: solo manual (`workflow_dispatch`) hasta el final.

## Tests
```sh
cargo test --manifest-path src-tauri/Cargo.toml          # SQLite
# Con MySQL (ej. contenedor desechable):
docker run -d --rm --name mb-mysql -e MYSQL_ROOT_PASSWORD=test -e MYSQL_DATABASE=mibodega_test -p 127.0.0.1:3399:3306 mysql:8.4
MYSQL_TEST_URL=mysql://root:test@127.0.0.1:3399/mibodega_test cargo test --manifest-path src-tauri/Cargo.toml
```

## Elegir motor de BD
En la pantalla inicial: SQLite (archivo `mibodega.db` en el directorio de datos de la app) o MySQL (host, puerto, usuario, clave, base). La elección se guarda en `config.json` del directorio de datos. La base MySQL debe existir; las tablas se migran solas.

## Respaldo y restauración
Configuración → Exportar/Restaurar (`.mibodega-backup`, zip con manifiesto y SHA-256). Funciona en SQLite y MySQL y entre ambos. Detalle en `docs/`.

## Documentación
- `docs/manual-comerciante.md` — manual de uso
- `docs/actualizacion.md` — actualizar versión y reglas de migraciones
- `docs/adr-0001-sqlx-any-dual-db.md` — decisión de arquitectura de datos

## Migraciones
Dos dialectos en paralelo: `src-tauri/migrations/sqlite` y `src-tauri/migrations/mysql`. Todo cambio de esquema va en ambos con el mismo número.

## Arquitectura (ADR breve)
- `sqlx::Any` con SQL común a ambos motores; sin features exclusivos de uno.
- Dinero en céntimos (`i64`); timestamps UTC en texto RFC3339 (orden lexicográfico = cronológico); IDs UUID.
- Lógica de negocio solo en Rust; React solo presenta.
- Única caja abierta: `cash_sessions.open_flag` UNIQUE (1 abierta, NULL cerrada), portable a MySQL.
