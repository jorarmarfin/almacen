import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { catalogApi, reportsApi } from "../lib/api";
import { dayEndIso, dayStartIso, formatCents, todayLocal } from "../lib/money";

function Kpi({ label, value, accent = false }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className={`rounded-xl border p-4 ${accent ? "border-accent/40 bg-accent/5" : "border-slate-200 bg-white"}`}>
      <div className="text-sm text-slate-500">{label}</div>
      <div className="text-3xl font-bold text-brand">{value}</div>
    </div>
  );
}

export default function Home({ businessName }: { businessName: string }) {
  const today = todayLocal();
  const dash = useQuery({ queryKey: ["dashboard", today], queryFn: () => reportsApi.dashboard(dayStartIso(today), dayEndIso(today)) });
  const low = useQuery({ queryKey: ["products", "low"], queryFn: () => catalogApi.products({ low_stock_only: true }) });
  const d = dash.data;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-brand">Hola, {businessName}</h1>
        <p className="text-slate-600">Resumen de hoy</p>
      </div>
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Kpi label="Ventas de hoy" value={formatCents(d?.sales_total_cents ?? 0)} />
        <Kpi label="Operaciones" value={String(d?.sales_count ?? 0)} />
        <Kpi label="Productos activos" value={String(d?.active_products ?? 0)} />
        <Kpi label="Por reponer" value={String(d?.low_stock_count ?? 0)} accent={(d?.low_stock_count ?? 0) > 0} />
      </div>
      <div className="flex flex-wrap gap-3">
        <Link to="/vender" className="btn-accent">
          Vender
        </Link>
        <Link to="/compras" className="btn-primary">
          Registrar compra
        </Link>
        <Link to="/caja" className="btn-ghost">
          {d?.cash_open ? "Ir a caja (abierta)" : "Abrir caja"}
        </Link>
        <Link to="/reportes" className="btn-ghost">
          Reportes
        </Link>
      </div>
      <section className="rounded-xl border border-slate-200 bg-white p-4">
        <h2 className="mb-2 font-bold text-brand">Stock bajo</h2>
        {low.data?.length === 0 && <p className="text-slate-500">Todo el stock está sobre el mínimo.</p>}
        {low.data?.slice(0, 10).map((p) => (
          <div key={p.id} className="flex justify-between border-b border-slate-100 py-2 last:border-0">
            <span>{p.name}</span>
            <span className="text-sm text-accent-dark">
              <b>{p.stock}</b> / mín. {p.min_stock}
            </span>
          </div>
        ))}
        {(low.data?.length ?? 0) > 10 && (
          <Link to="/inventario" className="mt-2 inline-block text-sm text-brand underline">
            Ver los {low.data!.length} productos
          </Link>
        )}
      </section>
    </div>
  );
}
