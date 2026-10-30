# Mi Bodega — Manual corto del comerciante

Mi Bodega funciona **sin internet**. Tus datos se guardan en tu equipo.

## 1. Primer uso
1. Abre la app y escribe el **nombre de tu negocio**.
2. Elige la base de datos: **SQLite** (recomendado, todo local) o MySQL (si ya tienes un servidor).
3. Pulsa **Comenzar**.

## 2. Cargar tus productos
- **Categorías**: crea las que uses (Bebidas, Abarrotes…).
- **Productos → + Nuevo producto**: nombre, categoría, costo y precio. Cada producto tiene **presentaciones**: *Unidad* (factor 1) y, si vendes por caja, *Caja x24* (factor 24) con su precio.
- Muchos productos a la vez: **Productos → Importar CSV → Descargar plantilla**. Llénala en Excel, guárdala como CSV y súbela. Verás una vista previa con los errores antes de importar.
- **Stock inicial**: en *Inventario → Ajustar* (entrada, motivo `STOCK_INICIAL`) o en la columna `stock_inicial` de la plantilla.

## 3. Recibir mercadería
**Compras / Proveedores → + Nueva compra**: elige proveedor (opcional), busca los productos, indica cantidad y costo y pulsa **Confirmar compra**. El stock sube **una sola vez**. Una compra confirmada no se edita; puedes guardarla como **borrador** si aún no llega.

## 4. Vender
1. Abre la **caja** con el efectivo con el que empiezas (la app te lo pide).
2. En **Vender** escribe o escanea el producto y pulsa **Enter**. Ajusta cantidad, presentación o descuento.
3. **Cobrar (F9)**: efectivo, Yape, Plin u otro; puedes combinar. En efectivo escribe lo recibido y verás el **vuelto**.
4. Confirma una sola vez. Se muestra el **ticket interno** (no es boleta ni factura electrónica).
- ¿Te equivocaste? **Historial de ventas → Ver → Anular venta** (pide motivo, repone el stock y revierte el efectivo).

## 5. Cerrar la caja
**Caja → Cerrar caja**: cuenta el efectivo, escríbelo y verás si **cuadra**, sobra o falta. El cierre queda registrado y no se puede cambiar. Yape y Plin no cuentan como efectivo.

## 6. Reportes
**Reportes**: ventas del periodo, por método y por día, más vendidos, por reponer y **margen estimado** (ingreso − costo de referencia; no es utilidad contable). Puedes exportar ventas, kardex y productos a CSV.

## 7. Respaldos — hazlos seguido
**Configuración → Exportar respaldo**. Guarda el archivo `.mibodega-backup` en un USB o en la nube.
- Al exportar puedes **proteger el respaldo con contraseña** (recomendado). **Si la olvidas, no se puede recuperar.**
- **Respaldo automático**: en Configuración elige una carpeta (mejor un USB o una carpeta de la nube) y la app guardará un respaldo cada vez que cierres la caja (conserva los 14 últimos; sin contraseña, usa una carpeta privada).
- Si el equipo falla y no hay respaldo, **los datos se pierden**.
- **Restaurar respaldo** reemplaza todos los datos actuales; antes la app guarda un respaldo de seguridad. Pide escribir `RESTAURAR` para confirmar.
- El respaldo sirve para pasar de SQLite a MySQL y al revés.

## Atajos
`F2` buscar · `F9` cobrar · `Enter` agregar producto · `Esc` cerrar ventana.
