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
  adjustStock: (productId: string, kind: "ENTRADA" | "SALIDA", qty: number, reason: string) =>
    invoke<number>("adjust_stock", { productId, kind, qty, reason }),
  kardex: (productId: string, from?: string, to?: string) =>
    invoke<Movement[]>("get_kardex", { productId, from: from ?? null, to: to ?? null }),
  stockIntegrity: () => invoke<string[]>("check_stock_integrity"),
};
