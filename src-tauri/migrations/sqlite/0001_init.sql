CREATE TABLE business_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  business_name TEXT NOT NULL,
  currency TEXT NOT NULL DEFAULT 'PEN',
  default_min_stock INTEGER NOT NULL DEFAULT 0,
  schema_version INTEGER NOT NULL
);

CREATE TABLE app_metadata (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  installation_id TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  backup_info TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE counters (
  name TEXT PRIMARY KEY,
  value INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE categories (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE products (
  id TEXT PRIMARY KEY,
  category_id TEXT REFERENCES categories(id) ON DELETE RESTRICT,
  sku TEXT NOT NULL UNIQUE,
  barcode TEXT UNIQUE,
  name TEXT NOT NULL,
  base_unit TEXT NOT NULL DEFAULT 'unidad',
  reference_cost_cents INTEGER NOT NULL DEFAULT 0 CHECK (reference_cost_cents >= 0),
  min_stock INTEGER NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
  active INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_products_name ON products(name);

CREATE TABLE product_presentations (
  id TEXT PRIMARY KEY,
  product_id TEXT NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  factor_to_base INTEGER NOT NULL CHECK (factor_to_base > 0),
  sale_price_cents INTEGER NOT NULL CHECK (sale_price_cents >= 0),
  active INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_presentations_product ON product_presentations(product_id);

CREATE TABLE suppliers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  tax_id TEXT,
  phone TEXT,
  notes TEXT,
  active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE purchases (
  id TEXT PRIMARY KEY,
  number TEXT NOT NULL UNIQUE,
  supplier_id TEXT REFERENCES suppliers(id) ON DELETE RESTRICT,
  status TEXT NOT NULL CHECK (status IN ('DRAFT','CONFIRMED','CANCELED')),
  purchased_at TEXT NOT NULL,
  total_cents INTEGER NOT NULL CHECK (total_cents >= 0),
  created_at TEXT NOT NULL
);

CREATE TABLE purchase_items (
  id TEXT PRIMARY KEY,
  purchase_id TEXT NOT NULL REFERENCES purchases(id) ON DELETE RESTRICT,
  product_id TEXT NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
  presentation_id TEXT NOT NULL REFERENCES product_presentations(id) ON DELETE RESTRICT,
  qty_presentations INTEGER NOT NULL CHECK (qty_presentations > 0),
  factor_snapshot INTEGER NOT NULL CHECK (factor_snapshot > 0),
  unit_cost_cents INTEGER NOT NULL CHECK (unit_cost_cents >= 0),
  total_cents INTEGER NOT NULL CHECK (total_cents >= 0)
);
CREATE INDEX idx_purchase_items_purchase ON purchase_items(purchase_id);

CREATE TABLE sales (
  id TEXT PRIMARY KEY,
  number TEXT NOT NULL UNIQUE,
  client_request_id TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('CONFIRMED','CANCELED')),
  sold_at TEXT NOT NULL,
  subtotal_cents INTEGER NOT NULL CHECK (subtotal_cents >= 0),
  discount_cents INTEGER NOT NULL DEFAULT 0 CHECK (discount_cents >= 0),
  total_cents INTEGER NOT NULL CHECK (total_cents >= 0),
  canceled_at TEXT,
  cancel_reason TEXT
);
CREATE INDEX idx_sales_sold_at ON sales(sold_at, status);

CREATE TABLE sale_items (
  id TEXT PRIMARY KEY,
  sale_id TEXT NOT NULL REFERENCES sales(id) ON DELETE RESTRICT,
  product_id TEXT NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
  presentation_id TEXT NOT NULL REFERENCES product_presentations(id) ON DELETE RESTRICT,
  qty_presentations INTEGER NOT NULL CHECK (qty_presentations > 0),
  factor_snapshot INTEGER NOT NULL CHECK (factor_snapshot > 0),
  unit_price_cents INTEGER NOT NULL CHECK (unit_price_cents >= 0),
  discount_cents INTEGER NOT NULL DEFAULT 0 CHECK (discount_cents >= 0),
  line_total_cents INTEGER NOT NULL CHECK (line_total_cents >= 0),
  cost_snapshot_cents_per_base_unit INTEGER NOT NULL CHECK (cost_snapshot_cents_per_base_unit >= 0)
);
CREATE INDEX idx_sale_items_sale ON sale_items(sale_id);

CREATE TABLE sale_payments (
  id TEXT PRIMARY KEY,
  sale_id TEXT NOT NULL REFERENCES sales(id) ON DELETE RESTRICT,
  method TEXT NOT NULL CHECK (method IN ('EFECTIVO','YAPE','PLIN','OTRO')),
  amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
  received_cents INTEGER,
  change_cents INTEGER,
  reference TEXT
);
CREATE INDEX idx_sale_payments_sale ON sale_payments(sale_id);

CREATE TABLE stock_movements (
  id TEXT PRIMARY KEY,
  product_id TEXT NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
  type TEXT NOT NULL CHECK (type IN ('COMPRA','VENTA','AJUSTE_ENTRADA','AJUSTE_SALIDA','ANULACION_VENTA','ANULACION_COMPRA')),
  qty_base_signed INTEGER NOT NULL CHECK (qty_base_signed <> 0),
  origin_type TEXT NOT NULL,
  origin_id TEXT,
  note TEXT,
  occurred_at TEXT NOT NULL
);
CREATE INDEX idx_stock_mov_product ON stock_movements(product_id, occurred_at);

CREATE TABLE stock_balances (
  product_id TEXT PRIMARY KEY REFERENCES products(id) ON DELETE RESTRICT,
  qty_base INTEGER NOT NULL DEFAULT 0
);

-- open_flag = 1 mientras la sesión está abierta, NULL al cerrar: UNIQUE garantiza una sola OPEN (RN-07)
CREATE TABLE cash_sessions (
  id TEXT PRIMARY KEY,
  opened_at TEXT NOT NULL,
  opening_cash_cents INTEGER NOT NULL CHECK (opening_cash_cents >= 0),
  closed_at TEXT,
  counted_cash_cents INTEGER,
  expected_cash_cents INTEGER,
  difference_cents INTEGER,
  note TEXT,
  status TEXT NOT NULL CHECK (status IN ('OPEN','CLOSED')),
  open_flag INTEGER UNIQUE CHECK (open_flag IS NULL OR open_flag = 1)
);

CREATE TABLE cash_movements (
  id TEXT PRIMARY KEY,
  cash_session_id TEXT NOT NULL REFERENCES cash_sessions(id) ON DELETE RESTRICT,
  type TEXT NOT NULL,
  method TEXT NOT NULL,
  amount_cents_signed INTEGER NOT NULL,
  source_type TEXT NOT NULL,
  source_id TEXT,
  concept TEXT NOT NULL,
  occurred_at TEXT NOT NULL
);
CREATE INDEX idx_cash_mov_session ON cash_movements(cash_session_id, occurred_at);
