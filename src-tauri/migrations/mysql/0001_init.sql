CREATE TABLE business_settings (
  id BIGINT NOT NULL PRIMARY KEY CHECK (id = 1),
  business_name VARCHAR(255) NOT NULL,
  currency VARCHAR(8) NOT NULL DEFAULT 'PEN',
  default_min_stock BIGINT NOT NULL DEFAULT 0,
  schema_version BIGINT NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE app_metadata (
  id BIGINT NOT NULL PRIMARY KEY CHECK (id = 1),
  installation_id VARCHAR(36) NOT NULL,
  schema_version BIGINT NOT NULL,
  backup_info TEXT,
  updated_at VARCHAR(32) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE counters (
  name VARCHAR(64) NOT NULL PRIMARY KEY,
  value BIGINT NOT NULL DEFAULT 0
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE categories (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  name VARCHAR(191) NOT NULL UNIQUE,
  active BIGINT NOT NULL DEFAULT 1
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE products (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  category_id VARCHAR(36),
  sku VARCHAR(64) NOT NULL UNIQUE,
  barcode VARCHAR(64) UNIQUE,
  name VARCHAR(255) NOT NULL,
  base_unit VARCHAR(32) NOT NULL DEFAULT 'unidad',
  reference_cost_cents BIGINT NOT NULL DEFAULT 0 CHECK (reference_cost_cents >= 0),
  min_stock BIGINT NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
  active BIGINT NOT NULL DEFAULT 1,
  created_at VARCHAR(32) NOT NULL,
  updated_at VARCHAR(32) NOT NULL,
  FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_products_name ON products(name);

CREATE TABLE product_presentations (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  product_id VARCHAR(36) NOT NULL,
  name VARCHAR(255) NOT NULL,
  factor_to_base BIGINT NOT NULL CHECK (factor_to_base > 0),
  sale_price_cents BIGINT NOT NULL CHECK (sale_price_cents >= 0),
  active BIGINT NOT NULL DEFAULT 1,
  FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_presentations_product ON product_presentations(product_id);

CREATE TABLE suppliers (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  tax_id VARCHAR(36),
  phone VARCHAR(32),
  notes TEXT,
  active BIGINT NOT NULL DEFAULT 1
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE purchases (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  number VARCHAR(32) NOT NULL UNIQUE,
  supplier_id VARCHAR(36),
  status VARCHAR(16) NOT NULL CHECK (status IN ('DRAFT','CONFIRMED','CANCELED')),
  purchased_at VARCHAR(32) NOT NULL,
  total_cents BIGINT NOT NULL CHECK (total_cents >= 0),
  created_at VARCHAR(32) NOT NULL,
  FOREIGN KEY (supplier_id) REFERENCES suppliers(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE purchase_items (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  purchase_id VARCHAR(36) NOT NULL,
  product_id VARCHAR(36) NOT NULL,
  presentation_id VARCHAR(36) NOT NULL,
  qty_presentations BIGINT NOT NULL CHECK (qty_presentations > 0),
  factor_snapshot BIGINT NOT NULL CHECK (factor_snapshot > 0),
  unit_cost_cents BIGINT NOT NULL CHECK (unit_cost_cents >= 0),
  total_cents BIGINT NOT NULL CHECK (total_cents >= 0),
  FOREIGN KEY (purchase_id) REFERENCES purchases(id) ON DELETE RESTRICT,
  FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT,
  FOREIGN KEY (presentation_id) REFERENCES product_presentations(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_purchase_items_purchase ON purchase_items(purchase_id);

CREATE TABLE sales (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  number VARCHAR(32) NOT NULL UNIQUE,
  client_request_id VARCHAR(36) NOT NULL UNIQUE,
  status VARCHAR(16) NOT NULL CHECK (status IN ('CONFIRMED','CANCELED')),
  sold_at VARCHAR(32) NOT NULL,
  subtotal_cents BIGINT NOT NULL CHECK (subtotal_cents >= 0),
  discount_cents BIGINT NOT NULL DEFAULT 0 CHECK (discount_cents >= 0),
  total_cents BIGINT NOT NULL CHECK (total_cents >= 0),
  canceled_at VARCHAR(32),
  cancel_reason VARCHAR(255)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_sales_sold_at ON sales(sold_at, status);

CREATE TABLE sale_items (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  sale_id VARCHAR(36) NOT NULL,
  product_id VARCHAR(36) NOT NULL,
  presentation_id VARCHAR(36) NOT NULL,
  qty_presentations BIGINT NOT NULL CHECK (qty_presentations > 0),
  factor_snapshot BIGINT NOT NULL CHECK (factor_snapshot > 0),
  unit_price_cents BIGINT NOT NULL CHECK (unit_price_cents >= 0),
  discount_cents BIGINT NOT NULL DEFAULT 0 CHECK (discount_cents >= 0),
  line_total_cents BIGINT NOT NULL CHECK (line_total_cents >= 0),
  cost_snapshot_cents_per_base_unit BIGINT NOT NULL CHECK (cost_snapshot_cents_per_base_unit >= 0),
  FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
  FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT,
  FOREIGN KEY (presentation_id) REFERENCES product_presentations(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_sale_items_sale ON sale_items(sale_id);

CREATE TABLE sale_payments (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  sale_id VARCHAR(36) NOT NULL,
  method VARCHAR(16) NOT NULL CHECK (method IN ('EFECTIVO','YAPE','PLIN','OTRO')),
  amount_cents BIGINT NOT NULL CHECK (amount_cents > 0),
  received_cents BIGINT,
  change_cents BIGINT,
  reference VARCHAR(255),
  FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_sale_payments_sale ON sale_payments(sale_id);

CREATE TABLE stock_movements (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  product_id VARCHAR(36) NOT NULL,
  type VARCHAR(32) NOT NULL CHECK (type IN ('COMPRA','VENTA','AJUSTE_ENTRADA','AJUSTE_SALIDA','ANULACION_VENTA','ANULACION_COMPRA')),
  qty_base_signed BIGINT NOT NULL CHECK (qty_base_signed <> 0),
  origin_type VARCHAR(32) NOT NULL,
  origin_id VARCHAR(36),
  note VARCHAR(500),
  occurred_at VARCHAR(32) NOT NULL,
  FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_stock_mov_product ON stock_movements(product_id, occurred_at);

CREATE TABLE stock_balances (
  product_id VARCHAR(36) NOT NULL PRIMARY KEY,
  qty_base BIGINT NOT NULL DEFAULT 0,
  FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

-- open_flag = 1 mientras la sesión está abierta, NULL al cerrar: UNIQUE garantiza una sola OPEN (RN-07)
CREATE TABLE cash_sessions (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  opened_at VARCHAR(32) NOT NULL,
  opening_cash_cents BIGINT NOT NULL CHECK (opening_cash_cents >= 0),
  closed_at VARCHAR(32),
  counted_cash_cents BIGINT,
  expected_cash_cents BIGINT,
  difference_cents BIGINT,
  note VARCHAR(500),
  status VARCHAR(16) NOT NULL CHECK (status IN ('OPEN','CLOSED')),
  open_flag BIGINT NULL UNIQUE CHECK (open_flag IS NULL OR open_flag = 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE cash_movements (
  id VARCHAR(36) NOT NULL PRIMARY KEY,
  cash_session_id VARCHAR(36) NOT NULL,
  type VARCHAR(32) NOT NULL,
  method VARCHAR(16) NOT NULL,
  amount_cents_signed BIGINT NOT NULL,
  source_type VARCHAR(32) NOT NULL,
  source_id VARCHAR(36),
  concept VARCHAR(255) NOT NULL,
  occurred_at VARCHAR(32) NOT NULL,
  FOREIGN KEY (cash_session_id) REFERENCES cash_sessions(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
CREATE INDEX idx_cash_mov_session ON cash_movements(cash_session_id, occurred_at);
