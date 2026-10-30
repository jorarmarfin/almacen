import { invoke } from "@tauri-apps/api/core";

export type Engine = "sqlite" | "mysql";

export interface MysqlConfig {
  host: string;
  port: number;
  user: string;
  password: string;
  database: string;
}

export interface DbConfig {
  engine: Engine;
  mysql: MysqlConfig | null;
}

export interface Settings {
  business_name: string;
  currency: string;
  default_min_stock: number;
  schema_version: number;
}

export interface BootState {
  engine: Engine;
  db_ready: boolean;
  initialized: boolean;
  settings: Settings | null;
}

export interface AppError {
  code: string;
  message: string;
}

export const errorMessage = (e: unknown): string =>
  typeof e === "object" && e && "message" in e ? String((e as AppError).message) : String(e);

export const api = {
  bootState: () => invoke<BootState>("get_boot_state"),
  testDbConnection: (config: DbConfig) => invoke<void>("test_db_connection", { config }),
  setDbConfig: (config: DbConfig) => invoke<void>("set_db_config", { config }),
  initBusiness: (input: { business_name: string; default_min_stock: number }) =>
    invoke<Settings>("init_business", { input }),
};

// ---------- Catálogo e inventario ----------

export interface Category {
  id: string;
  name: string;
  active: boolean;
  product_count: number;
}

export interface Presentation {
  id: string;
  name: string;
  factor_to_base: number;
  sale_price_cents: number;
  active: boolean;
}

export interface Product {
  id: string;
  category_id: string | null;
  category_name: string | null;
  sku: string;
  barcode: string | null;
  name: string;
  base_unit: string;
  reference_cost_cents: number;
  min_stock: number;
  active: boolean;
  stock: number;
  presentations: Presentation[];
}

export interface PresentationInput {
  id?: string | null;
  name: string;
  factor_to_base: number;
  sale_price_cents: number;
}

export interface ProductInput {
  id?: string | null;
  category_id: string | null;
  sku: string | null;
  barcode: string | null;
  name: string;
  base_unit: string;
  reference_cost_cents: number;
  min_stock: number | null;
  presentations: PresentationInput[];
}

export interface ProductFilter {
  search?: string;
  category_id?: string | null;
  include_inactive?: boolean;
  low_stock_only?: boolean;
}

export interface Movement {
  id: string;
  type: string;
  qty_base_signed: number;
  origin_type: string;
  note: string | null;
  balance_before: number | null;
  balance_after: number | null;
  occurred_at: string;
}

export interface ImportReport {
  rows: { line: number; ok: boolean; message: string | null }[];
  imported: number;
  dry_run: boolean;
}

export const catalogApi = {
  categories: () => invoke<Category[]>("list_categories"),
  saveCategory: (name: string, id: string | null = null, active = true) =>
    invoke<string>("save_category", { id, name, active }),
  deleteCategory: (id: string) => invoke<void>("delete_category", { id }),
  products: (filter: ProductFilter) => invoke<Product[]>("list_products", { filter }),
  saveProduct: (input: ProductInput) => invoke<string>("save_product", { input }),
  setProductActive: (id: string, active: boolean) => invoke<void>("set_product_active", { id, active }),
  importCsv: (content: string, dryRun: boolean) =>
    invoke<ImportReport>("import_products_csv", { content, dryRun }),
  productsTemplate: () => invoke<string>("get_products_template"),
  writeTextFile: (path: string, content: string) => invoke<void>("write_text_file", { path, content }),
  adjustStock: (productId: string, kind: "ENTRADA" | "SALIDA", qty: number, reason: string) =>
    invoke<number>("adjust_stock", { productId, kind, qty, reason }),
  kardex: (productId: string, from?: string, to?: string) =>
    invoke<Movement[]>("get_kardex", { productId, from: from ?? null, to: to ?? null }),
  stockIntegrity: () => invoke<string[]>("check_stock_integrity"),
};

// ---------- Proveedores y compras ----------

export interface Supplier {
  id: string;
  name: string;
  tax_id: string | null;
  phone: string | null;
  notes: string | null;
  active: boolean;
}

export type PurchaseStatus = "DRAFT" | "CONFIRMED" | "CANCELED";

export interface PurchaseSummary {
  id: string;
  number: string;
  supplier_id: string | null;
  supplier_name: string | null;
  status: PurchaseStatus;
  purchased_at: string;
  total_cents: number;
}

export interface PurchaseItemView {
  id: string;
  product_id: string;
  product_name: string;
  presentation_id: string;
  presentation_name: string;
  qty_presentations: number;
  factor_snapshot: number;
  unit_cost_cents: number;
  total_cents: number;
}

export interface PurchaseDetail extends PurchaseSummary {
  items: PurchaseItemView[];
}

export interface PurchaseInput {
  id?: string | null;
  supplier_id: string | null;
  purchased_at?: string | null;
  items: { product_id: string; presentation_id: string; qty_presentations: number; unit_cost_cents: number }[];
}

export const purchasesApi = {
  suppliers: () => invoke<Supplier[]>("list_suppliers"),
  saveSupplier: (input: Omit<Supplier, "id"> & { id?: string | null }) => invoke<string>("save_supplier", { input }),
  deleteSupplier: (id: string) => invoke<void>("delete_supplier", { id }),
  list: (status?: PurchaseStatus) => invoke<PurchaseSummary[]>("list_purchases", { status: status ?? null }),
  get: (id: string) => invoke<PurchaseDetail>("get_purchase", { id }),
  save: (input: PurchaseInput) => invoke<string>("save_purchase", { input }),
  confirm: (id: string) => invoke<void>("confirm_purchase", { id }),
  cancelDraft: (id: string) => invoke<void>("cancel_purchase_draft", { id }),
};

// ---------- Caja y ventas ----------

export interface CashSession {
  id: string;
  opened_at: string;
  opening_cash_cents: number;
  status: string;
  expected_cash_cents: number;
}

export type PayMethod = "EFECTIVO" | "YAPE" | "PLIN" | "OTRO";

export interface SaleInput {
  client_request_id: string;
  discount_cents: number;
  items: { product_id: string; presentation_id: string; qty_presentations: number; discount_cents: number }[];
  payments: { method: PayMethod; amount_cents: number; received_cents: number | null; reference: string | null }[];
}

export interface SaleSummary {
  id: string;
  number: string;
  status: "CONFIRMED" | "CANCELED";
  sold_at: string;
  subtotal_cents: number;
  discount_cents: number;
  total_cents: number;
  canceled_at: string | null;
  cancel_reason: string | null;
}

export interface SaleDetail extends SaleSummary {
  items: {
    product_name: string;
    presentation_name: string;
    qty_presentations: number;
    factor_snapshot: number;
    unit_price_cents: number;
    discount_cents: number;
    line_total_cents: number;
  }[];
  payments: {
    method: PayMethod;
    amount_cents: number;
    received_cents: number | null;
    change_cents: number | null;
    reference: string | null;
  }[];
}

export const salesApi = {
  currentCash: () => invoke<CashSession | null>("current_cash"),
  openCash: (openingCashCents: number) => invoke<string>("open_cash", { openingCashCents }),
  confirm: (input: SaleInput) => invoke<SaleDetail>("confirm_sale", { input }),
  cancel: (id: string, reason: string) => invoke<SaleDetail>("cancel_sale", { id, reason }),
  list: (from?: string, to?: string, status?: string) =>
    invoke<SaleSummary[]>("list_sales", { from: from ?? null, to: to ?? null, status: status ?? null }),
  get: (id: string) => invoke<SaleDetail>("get_sale", { id }),
};

// ---------- Caja (cierre, movimientos) y reportes ----------

export interface SessionRow {
  id: string;
  opened_at: string;
  opening_cash_cents: number;
  closed_at: string | null;
  counted_cash_cents: number | null;
  expected_cash_cents: number | null;
  difference_cents: number | null;
  note: string | null;
  status: "OPEN" | "CLOSED";
}

export interface TypeTotal {
  key: string;
  total_cents: number;
}

export interface SessionDetail {
  session: SessionRow;
  expected_cash_cents: number;
  movements: {
    id: string;
    type: string;
    method: string;
    amount_cents_signed: number;
    concept: string;
    occurred_at: string;
  }[];
  cash_by_type: TypeTotal[];
  sales_by_method: TypeTotal[];
}

export interface Dashboard {
  sales_total_cents: number;
  sales_count: number;
  active_products: number;
  low_stock_count: number;
  cash_open: boolean;
}

export interface SalesReport {
  total_cents: number;
  count: number;
  canceled_count: number;
  canceled_total_cents: number;
  by_method: TypeTotal[];
  by_day: { date: string; count: number; total_cents: number }[];
  revenue_cents: number;
  cost_cents: number;
  margin_cents: number;
}

export interface TopProduct {
  product_id: string;
  name: string;
  units_base: number;
  amount_cents: number;
}

export const cashApi = {
  addMovement: (kind: "INGRESO" | "EGRESO" | "RETIRO", method: PayMethod, amountCents: number, concept: string) =>
    invoke<void>("add_cash_movement", { kind, method, amountCents, concept }),
  close: (countedCents: number, note: string | null) => invoke<string>("close_cash", { countedCents, note }),
  sessions: () => invoke<SessionRow[]>("list_cash_sessions"),
  session: (id: string) => invoke<SessionDetail>("get_cash_session", { id }),
};

export const reportsApi = {
  dashboard: (from: string, to: string) => invoke<Dashboard>("dashboard", { from, to }),
  sales: (from: string, to: string) => invoke<SalesReport>("sales_report", { from, to }),
  top: (from: string, to: string, metric: "units" | "amount", limit = 10) =>
    invoke<TopProduct[]>("top_products", { from, to, metric, limit }),
  exportCsv: (kind: "products" | "kardex" | "sales", from: string, to: string) =>
    invoke<string>("export_csv", { kind, from, to }),
};

// ---------- Configuración y respaldo ----------

export interface AppInfo {
  version: string;
  data_dir: string;
  engine: Engine;
  last_backup_at: string | null;
}

export interface BackupManifest {
  format: string;
  schema_version: number;
  created_at: string;
  installation_id: string | null;
  business_name: string | null;
  engine: string;
  app_version: string;
  tables: Record<string, { rows: number; sha256: string }>;
}

export interface RestoreReport {
  pre_restore_backup: string | null;
  tables: Record<string, number>;
}

export const backupApi = {
  info: () => invoke<AppInfo>("get_app_info"),
  exportBackup: (path: string) => invoke<BackupManifest>("export_backup", { path }),
  exportSqliteNative: (path: string) => invoke<void>("export_sqlite_native", { path }),
  inspect: (path: string) => invoke<BackupManifest>("inspect_backup", { path }),
  restore: (path: string) => invoke<RestoreReport>("restore_backup", { path }),
};
