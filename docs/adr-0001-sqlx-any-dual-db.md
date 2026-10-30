# ADR 0001 — Una sola capa de datos para SQLite y MySQL

**Estado:** aceptada · 2026-10-30

## Contexto
La app debe funcionar offline con SQLite y, opcionalmente, contra un servidor MySQL, en Linux y Windows.

## Decisión
- `sqlx::Any` con SQL común a ambos motores (sin repositorios por motor). Migraciones duplicadas: `migrations/sqlite` y `migrations/mysql` con el mismo número y esquema.
- Dinero en céntimos `i64`, timestamps UTC como texto RFC3339, IDs UUID en texto.
- Lógica de negocio solo en Rust; React solo presenta.

## Reglas derivadas (aprendidas con pruebas en ambos motores)
- `Any` no decodifica `TINYINT`: los flags son `BIGINT` en MySQL.
- `SUM` en MySQL devuelve `DECIMAL`: usar `CAST(... AS SIGNED)`.
- MySQL ignora `REFERENCES` en línea: las FK van a nivel de tabla.
- Palabras reservadas de MySQL (`before`…) no sirven como alias.
- "Una sola caja abierta": columna `open_flag` UNIQUE (1 abierta, NULL cerrada), portable.
- Stock: único punto de cambio `inventory::apply_movement` (UPDATE condicional atómico).
- SQL dinámico solo con fragmentos estáticos y parámetros (`AssertSqlSafe` documentado).

## Consecuencias
- Toda prueba corre en SQLite y, con `MYSQL_TEST_URL`, en MySQL.
- Un test verifica que `backup::TABLES` cubra todas las columnas del esquema: al agregar columnas/tablas hay que actualizarlo.
