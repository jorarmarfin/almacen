import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import Ticket from "../components/Ticket";
import { errorMessage, salesApi, type SaleSummary } from "../lib/api";
import { dayEndIso, dayStartIso, formatCents, formatDateTime, todayLocal } from "../lib/money";

function SaleDialog({ sale, businessName, onClose }: { sale: SaleSummary; businessName: string; onClose: () => void }) {
  const qc = useQueryClient();
  const detail = useQuery({ queryKey: ["sale", sale.id], queryFn: () => salesApi.get(sale.id) });
  const [reason, setReason] = useState("");
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const cancel = useMutation({
    mutationFn: () => salesApi.cancel(sale.id, reason),
    onSuccess: () => {
      for (const k of ["sales", "sale", "products", "cash"]) qc.invalidateQueries({ queryKey: [k] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  const d = detail.data;
  return (
    <Modal title={`Venta ${sale.number}`} onClose={onClose}>
      {d && (
        <div className="space-y-4">
          <Ticket sale={d} businessName={businessName} />
          {d.status === "CANCELED" && (
            <p className="rounded-lg bg-slate-100 p-3 text-sm">
              Anulada {d.canceled_at && formatDateTime(d.canceled_at)} — {d.cancel_reason}
            </p>
          )}
          {asking && (
            <div className="space-y-2 rounded-lg border border-accent p-3">
              <label className="label">Motivo de la anulación *</label>
              <input className="input" value={reason} onChange={(e) => setReason(e.target.value)} autoFocus />
              <p className="text-xs text-slate-600">Repone el stock y revierte el efectivo en la caja abierta actual.</p>
            </div>
          )}
          {error && <p className="text-red-700">{error}</p>}
          <div className="flex justify-end gap-3">
            <button className="btn-ghost" onClick={() => window.print()}>
              Imprimir
            </button>
            {d.status === "CONFIRMED" &&
              (asking ? (
                <button className="btn-accent" disabled={!reason.trim() || cancel.isPending} onClick={() => cancel.mutate()}>
                  Confirmar anulación
                </button>
              ) : (
                <button className="btn-ghost text-red-700" onClick={() => setAsking(true)}>
                  Anular venta
                </button>
              ))}
          </div>
        </div>
      )}
    </Modal>
  );
}

export default function SalesHistory({ businessName }: { businessName: string }) {
  const [from, setFrom] = useState(todayLocal());
  const [to, setTo] = useState(todayLocal());
  const [status, setStatus] = useState("");
  const [selected, setSelected] = useState<SaleSummary | null>(null);
  const list = useQuery({
    queryKey: ["sales", from, to, status],
    queryFn: () => salesApi.list(from ? dayStartIso(from) : undefined, to ? dayEndIso(to) : undefined, status || undefined),
  });
  const net = list.data?.filter((s) => s.status === "CONFIRMED").reduce((a, s) => a + s.total_cents, 0) ?? 0;

  return (
    <div>
      <h1 className="mb-4 text-2xl font-bold text-brand">Historial de ventas</h1>
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <div>
          <label className="label">Desde</label>
          <input className="input" type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </div>
        <div>
          <label className="label">Hasta</label>
          <input className="input" type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </div>
        <div>
          <label className="label">Estado</label>
          <select className="input" value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">Todas</option>
            <option value="CONFIRMED">Confirmadas</option>
            <option value="CANCELED">Anuladas</option>
          </select>
        </div>
        <div className="ml-auto text-right">
          <div className="text-sm text-slate-500">Total vendido (sin anuladas)</div>
          <div className="text-2xl font-bold text-brand">{formatCents(net)}</div>
        </div>
      </div>
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">N°</th>
              <th className="px-4 py-3">Fecha</th>
              <th className="px-4 py-3">Estado</th>
              <th className="px-4 py-3 text-right">Total</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {list.data?.map((s) => (
              <tr key={s.id} className={`border-t border-slate-100 ${s.status === "CANCELED" ? "text-slate-400 line-through" : ""}`}>
                <td className="px-4 py-3 font-mono text-sm">{s.number}</td>
                <td className="px-4 py-3">{formatDateTime(s.sold_at)}</td>
                <td className="px-4 py-3 no-underline">{s.status === "CONFIRMED" ? "Confirmada" : "Anulada"}</td>
                <td className="px-4 py-3 text-right font-semibold">{formatCents(s.total_cents)}</td>
                <td className="px-4 py-3 text-right">
                  <button className="btn-ghost min-h-10 px-3" onClick={() => setSelected(s)}>
                    Ver
                  </button>
                </td>
              </tr>
            ))}
            {list.data?.length === 0 && (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-slate-500">
                  Sin ventas en el rango.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {selected && <SaleDialog sale={selected} businessName={businessName} onClose={() => setSelected(null)} />}
    </div>
  );
}
