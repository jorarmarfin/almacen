import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { catalogApi, errorMessage, reportsApi } from "../lib/api";
import { exportCsv } from "../lib/export";
import { dayEndIso, dayStartIso, formatCents, todayLocal } from "../lib/money";

const METHOD_LABEL: Record<string, string> = { EFECTIVO: "Efectivo", YAPE: "Yape", PLIN: "Plin", OTRO: "Otro" };

function monthStart() {
  const n = new Date();
  return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, "0")}-01`;
}

function Kpi({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-xl border border-slate-200 bg-white p-4">
      <div className="text-sm text-slate-500">{label}</div>
      <div className="text-2xl font-bold text-brand">{value}</div>
      {hint && <div className="text-xs text-slate-500">{hint}</div>}
    </div>
  );
}

export default function Reports() {
  const [from, setFrom] = useState(monthStart());
  const [to, setTo] = useState(todayLocal());
  const [metric, setMetric] = useState<"units" | "amount">("units");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const f = from ? dayStartIso(from) : "2000-01-01T00:00:00.000Z";
  const t = to ? dayEndIso(to) : "2100-01-01T00:00:00.000Z";

  const sales = useQuery({ queryKey: ["report-sales", f, t], queryFn: () => reportsApi.sales(f, t) });
  const top = useQuery({ queryKey: ["report-top", f, t, metric], queryFn: () => reportsApi.top(f, t, metric) });
  const low = useQuery({ queryKey: ["products", "low"], queryFn: () => catalogApi.products({ low_stock_only: true }) });

  async function doExport(kind: "products" | "kardex" | "sales", name: string) {
    setMsg(null);
    try {
      const path = await exportCsv(kind, f, t, `${name}_${from}_${to}.csv`);
      if (path) setMsg({ ok: true, text: `Guardado en ${path}` });
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) });
    }
  }

  const r = sales.data;
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-brand">Reportes</h1>
      <div className="flex flex-wrap items-end gap-3">
        <div>
          <label className="label">Desde</label>
          <input className="input" type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </div>
        <div>
          <label className="label">Hasta</label>
          <input className="input" type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </div>
        <div className="ml-auto flex flex-wrap gap-2">
          <button className="btn-ghost" onClick={() => doExport("sales", "ventas")}>
            ⬇ Ventas CSV
          </button>
          <button className="btn-ghost" onClick={() => doExport("kardex", "kardex")}>
            ⬇ Kardex CSV
          </button>
          <button className="btn-ghost" onClick={() => doExport("products", "productos")}>
            ⬇ Productos CSV
          </button>
        </div>
      </div>
      {msg && <p className={msg.ok ? "text-emerald-700" : "text-red-700"}>{msg.text}</p>}
      {sales.isError && <p className="text-red-700">{errorMessage(sales.error)}</p>}

      {r && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Kpi label="Total vendido" value={formatCents(r.total_cents)} hint="Ventas confirmadas (sin anuladas)" />
            <Kpi label="Operaciones" value={String(r.count)} hint={r.count ? `Ticket promedio ${formatCents(Math.round(r.total_cents / r.count))}` : undefined} />
            <Kpi label="Margen bruto estimado" value={formatCents(r.margin_cents)} hint="Ingreso − costo de referencia al vender. No es utilidad neta." />
            <Kpi label="Anuladas" value={`${r.canceled_count} · ${formatCents(r.canceled_total_cents)}`} hint="Informadas aparte" />
          </div>

          <div className="grid gap-6 lg:grid-cols-2">
            <section className="rounded-xl border border-slate-200 bg-white p-4">
              <h2 className="mb-2 font-bold text-brand">Cobros por método</h2>
              {r.by_method.length === 0 && <p className="text-slate-500">Sin ventas.</p>}
              {r.by_method.map((m) => (
                <div key={m.key} className="flex justify-between border-b border-slate-100 py-2 last:border-0">
                  <span>{METHOD_LABEL[m.key] ?? m.key}</span>
                  <b>{formatCents(m.total_cents)}</b>
                </div>
              ))}
            </section>
            <section className="rounded-xl border border-slate-200 bg-white p-4">
              <h2 className="mb-2 font-bold text-brand">Ventas por día</h2>
              <div className="max-h-56 overflow-y-auto">
                {r.by_day.length === 0 && <p className="text-slate-500">Sin ventas.</p>}
                {r.by_day.map((d) => (
                  <div key={d.date} className="flex justify-between border-b border-slate-100 py-2 last:border-0">
                    <span>
                      {d.date} <span className="text-sm text-slate-500">({d.count} ops)</span>
                    </span>
                    <b>{formatCents(d.total_cents)}</b>
                  </div>
                ))}
              </div>
            </section>
          </div>
        </>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="rounded-xl border border-slate-200 bg-white p-4">
          <div className="mb-2 flex items-center justify-between">
            <h2 className="font-bold text-brand">Más vendidos</h2>
            <div className="flex gap-1">
              <button className={metric === "units" ? "btn-primary min-h-9 px-3 text-sm" : "btn-ghost min-h-9 px-3 text-sm"} onClick={() => setMetric("units")}>
                Por unidades
              </button>
              <button className={metric === "amount" ? "btn-primary min-h-9 px-3 text-sm" : "btn-ghost min-h-9 px-3 text-sm"} onClick={() => setMetric("amount")}>
                Por monto
              </button>
            </div>
          </div>
          <p className="mb-2 text-xs text-slate-500">Ordenado por {metric === "units" ? "unidades base vendidas" : "monto vendido"}.</p>
          {top.data?.length === 0 && <p className="text-slate-500">Sin ventas.</p>}
          {top.data?.map((p, i) => (
            <div key={p.product_id} className="flex justify-between border-b border-slate-100 py-2 last:border-0">
              <span>
                {i + 1}. {p.name}
              </span>
              <span className="text-sm">
                <b>{p.units_base}</b> u · <b>{formatCents(p.amount_cents)}</b>
              </span>
            </div>
          ))}
        </section>

        <section className="rounded-xl border border-slate-200 bg-white p-4">
          <h2 className="mb-2 font-bold text-brand">Por reponer</h2>
          {low.data?.length === 0 && <p className="text-slate-500">Todo el stock está sobre el mínimo.</p>}
          <div className="max-h-72 overflow-y-auto">
            {low.data?.map((p) => (
              <div key={p.id} className="flex justify-between border-b border-slate-100 py-2 last:border-0">
                <span>{p.name}</span>
                <span className="text-sm text-accent-dark">
                  <b>{p.stock}</b> / mín. {p.min_stock}
                </span>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
