/** Céntimos -> "S/ 12.50" (sin float). */
export function formatCents(cents: number): string {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  return `${sign}S/ ${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

/** "12", "12.5", "12,50" -> céntimos; null si es inválido. Sin float. */
export function parseCents(input: string): number | null {
  const t = input.trim().replace(",", ".");
  const m = /^(\d*)(?:\.(\d{0,2}))?$/.exec(t);
  if (!m || (m[1] === "" && !m[2])) return null;
  return Number(m[1] || "0") * 100 + Number((m[2] ?? "").padEnd(2, "0"));
}

/** Céntimos -> "12.50" para inputs. */
export function centsToInput(cents: number): string {
  return `${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}

/** Timestamp UTC RFC3339 -> fecha/hora local legible. */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString("es-PE", { dateStyle: "short", timeStyle: "short" });
}

/** "2026-10-30" (día local) -> inicio/fin del día en UTC RFC3339, para filtrar rangos. */
export const dayStartIso = (d: string) => new Date(`${d}T00:00:00`).toISOString();
export const dayEndIso = (d: string) => new Date(`${d}T23:59:59.999`).toISOString();
export const todayLocal = () => {
  const n = new Date();
  return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, "0")}-${String(n.getDate()).padStart(2, "0")}`;
};
