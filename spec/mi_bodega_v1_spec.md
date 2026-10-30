# Mi Bodega — Especificación funcional y técnica V1

**Versión del documento:** 1.0  
**Fecha:** 30 de octubre de 2026  
**Estado:** Propuesta de MVP lista para planificación e implementación  
**Producto:** Aplicación portable *offline-first* de inventario, ventas y caja para una bodega o tienda pequeña.

---

## 1. Resumen ejecutivo

Mi Bodega V1 permite a un comerciante manejar productos, entradas de mercadería, ventas, caja e indicadores básicos desde **un solo dispositivo**, sin requerir conexión a internet. Está pensada para bodegas, bazares y minimarkets pequeños de Perú.

**Premisa:** instalar, registrar productos y empezar a vender en el mismo día. La V1 **no** pretende ser un ERP, un sistema contable ni una solución de facturación electrónica.

### Objetivos
1. Conocer el stock real y su historial de variaciones.
2. Registrar compras e ingresos de mercadería.
3. Vender rápidamente, con cálculo del vuelto.
4. Controlar entradas/salidas de dinero y cierres de caja.
5. Consultar resultados diarios y productos por reponer.
6. Conservar los datos localmente y permitir respaldos y restauración.

### Público objetivo
- Propietario que atiende su propia bodega.
- Tienda con un operador por dispositivo.
- Comercio de una sola sucursal con catálogo pequeño o mediano.

## 2. Alcance de la V1

| Módulo | Funcionalidad incluida |
|---|---|
| Configuración | Nombre comercial, moneda PEN, datos básicos, stock mínimo predeterminado |
| Productos | Alta, edición, desactivación, categoría, SKU/código de barras opcional, unidad base, costo y precio de venta |
| Presentaciones | Venta por unidad y paquetes/cajas con factor de conversión entero a la unidad base |
| Inventario | Stock actual derivado de movimientos; kardex; ajustes con motivo; alertas por stock mínimo |
| Proveedores | Registro básico para compras |
| Compras | Recepción de mercadería, costo por línea, actualización de stock y costo de referencia |
| Ventas | Búsqueda y carrito, cantidad, descuento por línea o total, efectivo/Yape/Plin/otros, cálculo de vuelto, comprobante interno |
| Caja | Apertura, ingresos, egresos, retiros, cierre y diferencias |
| Reportes | Ventas por día y rango, top productos, stock bajo, movimientos, margen bruto estimado |
| Respaldo | Exportación, importación/restauración con validación y confirmación |

### Fuera de alcance (V1)
- SUNAT: boletas/facturas electrónicas, guías y obligaciones fiscales.
- Fiados, cuotas, cobranza, crédito a clientes.
- Multiusuario concurrente, múltiples cajas/sucursales y sincronización entre dispositivos.
- Pedidos online, ecommerce, programa de fidelización, contabilidad completa.
- Control por lotes y vencimientos; balanzas, impresoras fiscales o integraciones especializadas.
- Operación simultánea desde PC y celular sobre una misma base de datos.

## 3. Plataforma y arquitectura

**Tecnología propuesta:** Flutter (Android y escritorio Windows/Linux) + SQLite local, usando una capa de persistencia estructurada (p. ej., Drift) y migraciones versionadas. Elegir y validar las bibliotecas concretas contra versiones estables al iniciar desarrollo.

- **Aplicación local:** una instalación, una base de datos, un negocio, una caja activa a la vez.
- **Capas:** interfaz → casos de uso/servicios de dominio → repositorios → SQLite.
- **Sin backend obligatorio:** ninguna operación crítica necesita internet.
- **Transacciones atómicas:** una venta registra su detalle, movimientos de stock, pagos y movimiento de caja en una sola transacción de DB.
- **Enteros para dinero:** guardar montos en céntimos (`amount_cents`), sin `float` binario para cálculos monetarios.
- **Cantidades:** usar cantidades decimales exactas o enteros de unidad mínima según la naturaleza del producto; en V1 las presentaciones utilizan factores enteros, y se definen reglas explícitas para productos fraccionables si llegan a habilitarse.
- **Fechas:** timestamps UTC en BD; mostrarlas en zona local del dispositivo, con zona de negocio configurable en futuras versiones.
- **ID:** UUID para entidades y movimientos; permite evolucionar hacia sincronización sin colisiones triviales.
- **Persistencia:** aplicar claves foráneas, índices y restricciones; activar `PRAGMA foreign_keys=ON`.
- **Portabilidad:** el instalador/aplicación y los datos locales son independientes; no asumir que copiar el ejecutable mueve los datos.

### Requisitos de calidad
- Velocidad objetivo: añadir un producto al carrito en menos de 1 s en un dispositivo de referencia, tras carga inicial de catálogo.
- Interfaz táctil con botones grandes, buena legibilidad y flujo de venta por teclado o toque.
- Confirmación antes de anulaciones, ajustes importantes y restauración.
- Sin conexión: todas las funciones del MVP deben mantenerse operativas.
- Manejo de interrupciones: si falla una transacción, no dejar stock, venta y caja parcialmente registrados.

## 4. Roles y navegación

**V1:** rol único `propietario/operador` local, sin gestión de usuarios. Puede configurarse PIN de apertura de aplicación como mejora opcional; no se asume que un PIN sustituya el cifrado de datos.

Navegación principal:
1. Inicio (indicadores y accesos rápidos).
2. Vender.
3. Productos.
4. Inventario.
5. Compras / Proveedores.
6. Caja.
7. Reportes.
8. Configuración / Respaldo.

## 5. Requisitos funcionales detallados

### RF-01 Productos y categorías
- Crear/editar/desactivar productos; no borrado físico si existen movimientos.
- Campos: nombre, categoría, SKU autogenerado o manual, código de barras opcional, unidad base, costo de referencia, precio base, stock mínimo, estado.
- SKU único en el negocio; código de barras único si se informa, según regla configurable para artículos sin código.
- Buscar por nombre, SKU o código de barras; filtrar por categoría.
- Presentaciones por producto: nombre (`unidad`, `paquete x6`, `caja x24`), factor entero positivo hacia la unidad base, precio por presentación.
- Productos con historial se desactivan, no se eliminan.

### RF-02 Inventario
- No mantener el stock como un valor editable sin trazabilidad: derivarlo de movimientos o mantener saldo materializado actualizado transaccionalmente junto con el kardex.
- Tipos de movimiento: `COMPRA`, `VENTA`, `AJUSTE_ENTRADA`, `AJUSTE_SALIDA`, `ANULACION_VENTA`, `ANULACION_COMPRA` (esta última puede posponerse si se bloquea anulación de compras y se corrige mediante ajuste autorizado).
- Cada movimiento guarda producto, fecha, cantidad con signo en unidades base, tipo, documento de origen y motivo cuando aplique.
- Stock negativo **bloqueado por defecto**. Si no existe disponibilidad suficiente, la venta no se confirma.
- Avisar cuando stock disponible ≤ stock mínimo.
- Ajuste manual siempre requiere motivo; registrar antes/después para auditoría.
- Conteo físico completo por planilla queda fuera de V1; permitir ajuste puntual.

### RF-03 Proveedores y compras
- Proveedor: nombre comercial, identificador tributario opcional, teléfono, nota.
- Compra: número interno, proveedor opcional, fecha, líneas de producto/presentación, cantidad, costo unitario y costo total.
- Confirmar compra aumenta existencias exactamente una vez; no permitir una segunda contabilización del mismo documento.
- Al confirmar, actualizar costo de referencia con **último costo de compra**, sin recalcular de manera retroactiva el costo histórico de ventas.
- Documento confirmado no se edita libremente. Correcciones mediante reversión validada o ajuste documentado.

### RF-04 Ventas y cobros
- Agregar productos por búsqueda, SKU o código escaneado (teclado/lector que emula teclado); uso de cámara es fase posterior si no se valida su integración en todas las plataformas.
- Seleccionar presentación, cantidad y descuento; subtotal, descuento y total visibles.
- Admitir una o más formas de pago por venta; en efectivo calcular vuelto; registrar referencia opcional para Yape/Plin.
- Métodos: `EFECTIVO`, `YAPE`, `PLIN`, `OTRO`; solo el efectivo afecta el saldo físico de efectivo en caja.
- En venta confirmada: persistir cabecera, líneas, pagos, movimiento de salida por inventario y movimiento de efectivo si corresponde.
- Imprimir o compartir comprobante **interno no tributario** opcional en formatos soportados. No llamarlo boleta electrónica.
- Anular venta confirmada: generar movimientos inversos de stock y caja, con motivo, evitando doble anulación. Restringir por regla si caja está cerrada; cualquier ajuste a periodos cerrados debe registrarse en periodo abierto y trazable.

### RF-05 Caja
- Solo una sesión abierta a la vez.
- Apertura con saldo inicial declarado.
- Registrar ingresos/egresos manuales con concepto, monto y método; los movimientos de efectivo modifican el saldo físico esperado.
- El esperado = apertura + ventas cobradas en efectivo − devoluciones en efectivo + ingresos manuales en efectivo − egresos/retiros en efectivo.
- Cierre con efectivo contado, diferencia y observación opcional; no alterar ni borrar cierres anteriores.
- Ventas Yape/Plin se reflejan en reportes de cobros, no se suman al efectivo físico esperado.
- Para simplificar, la V1 requiere caja abierta para confirmar ventas.

### RF-06 Reportes
- Inicio: ventas de hoy, número de operaciones, SKU activos, alertas de stock bajo.
- Ventas: rango de fechas, total vendido, desglose de métodos, devoluciones/anulaciones separadas.
- Productos más vendidos: unidades base y monto, indicando la métrica usada para ordenar.
- Inventario: disponible, por reponer y movimientos por producto/rango.
- **Margen bruto estimado:** ingreso neto de la línea menos costo de referencia **capturado al vender** multiplicado por unidades base. No equivale a utilidad neta ni a contabilidad fiscal.
- Exportar CSV al menos para productos, kardex y ventas.

### RF-07 Respaldo y recuperación
- Generar respaldo completo de SQLite **consistente** (API de backup o snapshot correcto; nunca copiar a ciegas una BD activa con WAL).
- Incluir manifiesto con versión de esquema, fecha, identificador del negocio y suma de verificación.
- La restauración valida compatibilidad e integridad, confirma sustitución de datos y crea, si es posible, un respaldo previo.
- En Android utilizar el selector de archivos del sistema; en escritorio permitir elegir destino.
- La aplicación debe explicar que el almacenamiento local puede perderse si el dispositivo falla o se desinstala la app sin respaldo.
- Las copias contienen información comercial; recomendar archivo cifrado con contraseña y definirlo como requisito antes de lanzamiento comercial si hay datos sensibles.

## 6. Modelo de datos inicial

| Tabla | Campos clave / propósito |
|---|---|
| `business_settings` | id, business_name, currency, default_min_stock, schema_version |
| `categories` | id, name, active |
| `products` | id, category_id, sku, barcode?, name, base_unit, reference_cost_cents, min_stock, active, created_at, updated_at |
| `product_presentations` | id, product_id, name, factor_to_base, sale_price_cents, active |
| `suppliers` | id, name, tax_id?, phone?, notes?, active |
| `purchases` | id, number, supplier_id?, status, purchased_at, total_cents, created_at |
| `purchase_items` | id, purchase_id, product_id, presentation_id, qty_presentations, factor_snapshot, unit_cost_cents, total_cents |
| `sales` | id, number, status, sold_at, subtotal_cents, discount_cents, total_cents, canceled_at?, cancel_reason? |
| `sale_items` | id, sale_id, product_id, presentation_id, qty_presentations, factor_snapshot, unit_price_cents, discount_cents, line_total_cents, cost_snapshot_cents_per_base_unit |
| `sale_payments` | id, sale_id, method, amount_cents, received_cents?, change_cents?, reference? |
| `stock_movements` | id, product_id, type, qty_base_signed, origin_type, origin_id, note?, occurred_at |
| `stock_balances` | product_id (PK), qty_base; saldo cache transaccional verificado contra kardex |
| `cash_sessions` | id, opened_at, opening_cash_cents, closed_at?, counted_cash_cents?, expected_cash_cents?, difference_cents?, status |
| `cash_movements` | id, cash_session_id, type, method, amount_cents_signed, source_type, source_id?, concept, occurred_at |
| `app_metadata` | installation_id, schema_version, backup_info, updated_at |

### Integridad y diseño
- Claves foráneas para cada referencia y `ON DELETE RESTRICT` en históricos.
- Índices por `products.sku`, `products.barcode`, movimientos `(product_id, occurred_at)`, ventas `(sold_at, status)`.
- Restricciones de valores positivos para cantidades y precios donde corresponda; descuentos no pueden producir totales negativos.
- Guardar snapshots de precio, factor de presentación y costo en cada detalle: los cambios futuros no modifican ventas históricas.
- Numeración humana (`V-000001`, `C-000001`) generada de modo transaccional; el UUID permanece como identificador técnico.
- Ningún campo monetario debe calcularse con números flotantes.

## 7. Reglas de negocio críticas

**RN-01** Una venta confirmada es inmutable; su corrección se hace mediante anulación con trazabilidad.  
**RN-02** Las ventas reducen stock en unidades base: `cantidad_vendida × factor_presentación`.  
**RN-03** Las compras incrementan stock usando el factor congelado en cada línea.  
**RN-04** No se puede vender más que el stock disponible.  
**RN-05** Ninguna operación debe registrarse dos veces por doble toque: bloquear confirmación en UI e implementar idempotencia a nivel de caso de uso/BD.  
**RN-06** Los pagos deben cuadrar con el total de venta; el efectivo recibido incluye el vuelto que se entrega, y el valor neto cobrado en efectivo es el importe asignado a ese método.  
**RN-07** Solo puede existir una caja `OPEN` por instalación.  
**RN-08** Cada cambio de existencias genera un movimiento auditable.  
**RN-09** Los cambios de costo/precio no reescriben registros históricos.  
**RN-10** No hay sincronización ni uso compartido multi-dispositivo en V1.  
**RN-11** Una anulación debe revertir las operaciones exactamente una vez y respetar periodos de caja cerrados.  
**RN-12** La restauración verifica integridad y versión del archivo antes de reemplazar datos.

## 8. Flujos principales

### Flujo A — Primera instalación
1. Abrir app y crear negocio local.
2. Definir nombre y stock mínimo por defecto.
3. Crear categorías y productos, manualmente o importar CSV con validación.
4. Registrar ingreso inicial como movimiento `AJUSTE_ENTRADA` con motivo `STOCK_INICIAL` (o compra real si corresponde).
5. Abrir caja y comenzar ventas.

### Flujo B — Vender
1. Abrir la vista Vender, con caja abierta.
2. Buscar o escanear producto, seleccionar presentación y cantidad.
3. Validar existencias, mostrar subtotales, descuentos y total.
4. Elegir método(s), ingresar efectivo recibido si aplica, mostrar vuelto.
5. Confirmar una sola vez en transacción de DB.
6. Mostrar ticket interno y actualizar reportes, stock y caja.

### Flujo C — Reponer mercadería
1. Abrir Compras y elegir/crear proveedor.
2. Agregar productos y cantidades con costo.
3. Revisar monto y confirmar.
4. Generar movimientos `COMPRA`, actualizar stock y costo de referencia.
5. Ver las unidades disponibles en inventario.

### Flujo D — Cerrar caja
1. Revisar ventas, ingresos y egresos de la sesión.
2. Contar el efectivo físico.
3. Registrar importe contado y observaciones si existe diferencia.
4. Confirmar cierre; el registro queda inmutable.

### Flujo E — Respaldar y restaurar
1. Elegir Exportar respaldo; guardar archivo consistente con manifiesto.
2. Restaurar: seleccionar archivo, validar versión/checksum, mostrar fecha y negocio de origen.
3. Advertir sustitución y solicitar confirmación explícita.
4. Crear respaldo previo cuando sea posible y ejecutar restauración segura.
5. Reiniciar conexiones y validar BD restaurada.

## 9. Pantallas mínimas

| Pantalla | Elementos clave |
|---|---|
| Configuración inicial | Negocio, moneda, comenzar |
| Inicio | KPIs, stock bajo, accesos rápidos |
| Productos | Lista/búsqueda, formulario y presentaciones |
| Inventario | Stock, kardex, ajuste manual |
| Compras | Proveedores, compra nueva, historial |
| Vender | Buscador, carrito, total, pago, vuelto |
| Venta realizada | Resumen, ticket interno, opción de nueva venta |
| Historial de ventas | Filtros, detalle, anulación con motivo |
| Caja | Abrir, movimientos, cierre, historial |
| Reportes | Fechas, indicadores, exportación |
| Configuración | Datos del negocio, respaldo, restauración, acerca de |

## 10. Historias de usuario y aceptación

| ID | Historia | Criterio de aceptación |
|---|---|---|
| HU-01 | Como dueño, registro productos y precios. | SKU único, nombre obligatorio y precio no negativo; se puede editar y desactivar. |
| HU-02 | Como dueño, vendo una unidad o una caja. | Se descuenta exactamente el factor en unidades base. |
| HU-03 | Como operador, veo cuánto vuelto entregar. | El vuelto de efectivo se calcula y no se permiten cobros insuficientes. |
| HU-04 | Como dueño, recibo mercadería. | La compra confirmada genera una sola entrada de stock. |
| HU-05 | Como dueño, corrijo un error de inventario. | Ajuste requiere motivo, queda en kardex y cambia el saldo. |
| HU-06 | Como operador, cierro caja. | Muestra efectivo esperado, contado y diferencia; cierre queda bloqueado para edición. |
| HU-07 | Como dueño, sé qué reponer. | Lista de productos cuyo stock está en o bajo el mínimo. |
| HU-08 | Como dueño, consulto ventas del día. | Totales coinciden con ventas válidas netas de anulaciones. |
| HU-09 | Como dueño, recupero datos ante falla. | Respaldo/restauración valida archivo y mantiene integridad. |
| HU-10 | Como dueño, trabajo sin internet. | Compra, venta, caja e inventario operan en modo avión. |

## 11. Plan de implementación por hitos

**Hito 0 — Base técnica**
- Proyecto Flutter, navegación, tema visual, SQLite/Drift y migraciones.
- Entidades, repositorios, errores de dominio y pruebas transaccionales.
- Configuración inicial y datos semilla para desarrollo.

**Hito 1 — Catálogo + inventario**
- Categorías, productos, presentaciones.
- Saldos, kardex, ajustes y validaciones.
- Importación CSV inicial si su complejidad lo permite.

**Hito 2 — Compras + ventas**
- Proveedores, recepción de compras.
- Carrito, pagos y comprobante interno.
- Pruebas de stock, redondeo, doble confirmación y reversión.

**Hito 3 — Caja + reportes**
- Sesiones de caja, movimientos, cierre.
- Reportes y exportación CSV.

**Hito 4 — Respaldo + entrega**
- Backup/restauración, pruebas de fallos, optimización y empaquetado.
- Instalador de escritorio y APK de distribución según plataforma objetivo.
- Manual corto del comerciante.

## 12. Pruebas mínimas obligatorias

1. Comprar una caja de 24 → vender 3 unidades → saldo 21.
2. Intentar vender 22 unidades con saldo 21 → rechazo sin cambios.
3. Confirmar venta dos veces rápidamente → una sola venta, una sola salida y un solo cobro.
4. Error simulado durante venta → rollback completo de venta, stock y caja.
5. Pago mixto (efectivo + Yape) → caja física aumenta solo por efectivo neto.
6. Cierre con diferencia → se conserva esperado, contado y diferencia.
7. Anular venta una sola vez → stock se repone; segundo intento rechazado.
8. Editar precio tras vender → histórico conserva precio original.
9. Restaurar respaldo corrupto → rechazo sin dañar BD activa.
10. Probar sin internet desde instalación ya configurada → operaciones disponibles.
11. Reiniciar dispositivo en operación → datos confirmados persistentes, operaciones incompletas revertidas.
12. Cambiar presentación de caja de 24 a 12 después de vender → ventas históricas conservan factor 24.

## 13. Definición de terminado (DoD)

El MVP está listo cuando:
- Todas las HU-01 a HU-10 pasan las pruebas de aceptación.
- No hay divergencia entre `stock_balances` y la suma del kardex en pruebas automatizadas.
- No hay ventas confirmadas sin líneas/pagos/stock/caja congruentes.
- Los cálculos de dinero y vuelto son deterministas, sin errores binarios de coma flotante.
- Respaldo restaurado reproduce datos de ventas, inventario y caja.
- La app funciona sin internet y tiene recorrido básico probado en un Android real y un escritorio objetivo.
- Existen migraciones probadas y procedimiento documentado para actualizar de versión sin perder datos.
- Se entrega código fuente, instrucciones de build, instalable y guía corta de uso.

## 14. Riesgos y decisiones abiertas

| Decisión | Recomendación V1 |
|---|---|
| ¿Android o escritorio primero? | Validar ambos, priorizar el que utilice el cliente piloto. |
| ¿Cantidades fraccionadas (kg, litro)? | No incluir hasta definir unidades mínimas, redondeos y balanza. |
| ¿Impresoras térmicas? | Ticket PDF/visual primero; impresión específica en V1.1. |
| ¿Costeo FIFO/promedio? | V1: costo de referencia capturado al vender; no prometer utilidad contable exacta. |
| ¿Seguridad? | Bloqueo opcional local; evaluar cifrado de datos y respaldos antes de distribución. |
| ¿Fiados? | V1.1, una vez estabilizados ventas y caja. |
| ¿SUNAT? | Proyecto separado con validación normativa y soporte operativo. |
| ¿Sincronización PC-móvil? | V2 con estrategia de conflictos, IDs y eventos; no improvisar copia de SQLite entre dispositivos. |

## 15. Posible evolución

**V1.1:** clientes/fiados, lector de cámara, vencimientos por lotes, tickets térmicos, mejora de importación.  
**V2:** multiusuario, sincronización opcional, nube de respaldo, sucursales, métricas remotas.  
**Integraciones comerciales:** SUNAT e impresión avanzada como productos o módulos evaluados por separado.

---

> **Principio de producto:** el bodeguero debe poder vender en segundos y cuadrar su caja al final del día. Si una función añade complejidad sin ayudar a eso, no entra en V1.
