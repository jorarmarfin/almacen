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
