# Actualizar Mi Bodega a una versión nueva

1. **Haz un respaldo** (Configuración → Exportar respaldo) y guárdalo fuera del equipo.
2. Instala la versión nueva encima de la anterior (el instalador conserva los datos: viven en la carpeta de datos de la app, no junto al programa).
3. Al abrir, la app aplica sola las **migraciones** pendientes (SQLite o MySQL). Los datos existentes se conservan.
4. Si algo falla, reinstala la versión anterior y **restaura** el respaldo del paso 1.

Un respaldo de una versión **más nueva** que la app instalada se rechaza con un aviso; actualiza la app primero. Un respaldo de una versión **más antigua** se restaura sin problema.

## Para desarrolladores
- Cada cambio de esquema = migración `000N_*.sql` en `src-tauri/migrations/sqlite` **y** `mysql`, y subir `SCHEMA_VERSION` en `db.rs`.
- Actualizar `backup::TABLES` si cambian tablas/columnas (lo comprueba un test).
- Nunca editar una migración ya publicada: sqlx valida su checksum.
- Test de actualización con datos: `phase6_tests::upgrade_*`.
