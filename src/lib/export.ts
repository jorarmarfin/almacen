import { save } from "@tauri-apps/plugin-dialog";
import { catalogApi, reportsApi } from "./api";

/** Genera un CSV en Rust y lo guarda donde el usuario elija. Devuelve la ruta o null si canceló. */
export async function exportCsv(kind: "products" | "kardex" | "sales", from: string, to: string, filename: string) {
  const path = await save({ defaultPath: filename, filters: [{ name: "CSV", extensions: ["csv"] }] });
  if (!path) return null;
  await catalogApi.writeTextFile(path, await reportsApi.exportCsv(kind, from, to));
  return path;
}
