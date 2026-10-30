import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import OpenCashCard from "../components/OpenCashCard";
import { cashApi, errorMessage, salesApi, type CloseResult, type PayMethod, type SessionRow } from "../lib/api";
import { formatCents, formatDateTime, parseCents } from "../lib/money";

const TYPE_LABEL: Record<string, string> = {
  VENTA: "Ventas en efectivo",
  ANULACION_VENTA: "Anulaciones",
  INGRESO: "Ingresos",
  EGRESO: "Egresos",
  RETIRO: "Retiros",
};

function ManualDialog({ kind, onClose }: { kind: "INGRESO" | "EGRESO" | "RETIRO"; onClose: () => void }) {
  const qc = useQueryClient();
  const [method, setMethod] = useState<PayMethod>("EFECTIVO");
  const [amount, setAmount] = useState("");
  const [concept, setConcept] = useState("");
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: async () => {
      const cents = parseCents(amount);
      if (cents === null || cents <= 0) throw { message: "Monto inválido" };
      return cashApi.addMovement(kind, method, cents, concept);
    },
    onSuccess: () => {
      for (const k of ["cash", "cash-session", "cash-sessions"]) qc.invalidateQueries({ queryKey: [k] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  const title = { INGRESO: "Ingreso de dinero", EGRESO: "Egreso (gasto)", RETIRO: "Retiro de efectivo" }[kind];
  return (
    <Modal title={title} onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <div>
          <label className="label">Método</label>
          <select className="input" value={method} onChange={(e) => setMethod(e.target.value as PayMethod)}>
            <option value="EFECTIVO">Efectivo (afecta la caja)</option>
            <option value="YAPE">Yape</option>
            <option value="PLIN">Plin</option>
            <option value="OTRO">Otro</option>
          </select>
        </div>
        <div>
          <label className="label">Monto (S/)</label>
          <input className="input" inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">Concepto *</label>
          <input className="input" value={concept} onChange={(e) => setConcept(e.target.value)} />
        </div>
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button type="button" className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button type="submit" className="btn-accent" disabled={save.isPending}>
            Registrar
          </button>
        </div>
      </form>
    </Modal>
  );
}

function CloseDialog({ expected, onClose }: { expected: number; onClose: () => void }) {
  const qc = useQueryClient();
  const [counted, setCounted] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CloseResult | null>(null);
  const cents = parseCents(counted);
  const diff = cents === null ? null : cents - expected;
  const close = useMutation({
    mutationFn: () => cashApi.close(cents!, note.trim() || null),
    onSuccess: (r) => {
      for (const k of ["cash", "cash-session", "cash-sessions"]) qc.invalidateQueries({ queryKey: [k] });
      // Sin respaldo automático configurado se cierra directo; si no, se muestra su resultado.
      if (r.auto_backup || r.auto_backup_error) setResult(r);
      else onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  if (result) {
    return (
      <Modal title="Caja cerrada" onClose={onClose}>
        <p className="text-emerald-700">El cierre quedó registrado.</p>
        {result.auto_backup && (
          <p className="mt-3 break-all text-sm text-slate-700">
            Respaldo automático guardado en <code>{result.auto_backup}</code>
          </p>
        )}
        {result.auto_backup_error && (
          <p className="mt-3 rounded-lg bg-amber-50 p-3 text-sm text-amber-900">
            El cierre se registró, pero el respaldo automático falló: {result.auto_backup_error}. Haz uno manual en Configuración.
          </p>
        )}
        <div className="mt-6 flex justify-end">
          <button className="btn-accent" onClick={onClose} autoFocus>
            Aceptar
          </button>
        </div>
      </Modal>
    );
  }
  return (
    <Modal title="Cerrar caja" onClose={onClose}>
      <div className="space-y-4">
        <div className="rounded-lg bg-brand-soft p-4 text-center">
          <div className="text-sm text-brand">Efectivo esperado</div>
          <div className="text-3xl font-bold text-brand">{formatCents(expected)}</div>
        </div>
        <div>
          <label className="label">Efectivo contado (S/) *</label>
          <input className="input text-lg" inputMode="decimal" value={counted} onChange={(e) => setCounted(e.target.value)} autoFocus />
        </div>
        {diff !== null && (
          <p className={`text-center text-xl font-bold ${diff === 0 ? "text-emerald-700" : "text-accent-dark"}`}>
            {diff === 0 ? "Caja cuadrada" : diff > 0 ? `Sobrante ${formatCents(diff)}` : `Faltante ${formatCents(-diff)}`}
          </p>
        )}
        <div>
          <label className="label">Observación {diff !== null && diff !== 0 ? "(recomendada)" : "(opcional)"}</label>
          <input className="input" value={note} onChange={(e) => setNote(e.target.value)} />
        </div>
        <p className="text-xs text-slate-600">El cierre queda registrado y no se puede modificar ni borrar.</p>
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-accent" disabled={cents === null || close.isPending} onClick={() => close.mutate()}>
            Confirmar cierre
          </button>
        </div>
      </div>
    </Modal>
  );
}

function SessionView({ id }: { id: string }) {
  const detail = useQuery({ queryKey: ["cash-session", id], queryFn: () => cashApi.session(id) });
  const d = detail.data;
  if (!d) return <p>Cargando…</p>;
  const by = (k: string) => d.cash_by_type.find((x) => x.key === k)?.total_cents ?? 0;
  const rows: [string, number][] = [
    ["Apertura", d.session.opening_cash_cents],
    ...(["VENTA", "ANULACION_VENTA", "INGRESO", "EGRESO", "RETIRO"] as const).map((k): [string, number] => [TYPE_LABEL[k], by(k)]),
  ];
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
        {rows.map(([label, v]) => (
          <div key={label} className="rounded-xl border border-slate-200 bg-white p-3">
            <div className="text-xs text-slate-500">{label}</div>
            <div className={`text-lg font-bold ${v < 0 ? "text-accent-dark" : ""}`}>{formatCents(v)}</div>
          </div>
        ))}
        <div className="rounded-xl bg-brand p-3 text-white">
          <div className="text-xs opacity-80">{d.session.status === "OPEN" ? "Efectivo esperado" : "Esperado al cierre"}</div>
          <div className="text-lg font-bold">{formatCents(d.expected_cash_cents)}</div>
        </div>
      </div>
      {d.session.status === "CLOSED" && (
        <div className="rounded-xl border border-slate-200 bg-white p-3 text-sm">
          Contado <b>{formatCents(d.session.counted_cash_cents ?? 0)}</b> · Diferencia{" "}
          <b className={(d.session.difference_cents ?? 0) === 0 ? "text-emerald-700" : "text-accent-dark"}>
            {formatCents(d.session.difference_cents ?? 0)}
          </b>
          {d.session.note && <> · {d.session.note}</>}
        </div>
      )}
      {d.sales_by_method.length > 0 && (
        <div className="text-sm text-slate-700">
          Cobros de ventas:{" "}
          {d.sales_by_method.map((m) => (
            <span key={m.key} className="mr-3">
              {m.key} <b>{formatCents(m.total_cents)}</b>
            </span>
          ))}
          <span className="text-slate-500">(Yape/Plin no entran al efectivo)</span>
        </div>
      )}
      <div className="max-h-72 overflow-y-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 bg-brand-soft text-brand">
            <tr>
              <th className="px-3 py-2">Hora</th>
              <th className="px-3 py-2">Tipo</th>
              <th className="px-3 py-2">Método</th>
              <th className="px-3 py-2">Concepto</th>
              <th className="px-3 py-2 text-right">Monto</th>
            </tr>
          </thead>
          <tbody>
            {d.movements.map((m) => (
              <tr key={m.id} className="border-t border-slate-100">
                <td className="px-3 py-2">{formatDateTime(m.occurred_at)}</td>
                <td className="px-3 py-2">{TYPE_LABEL[m.type] ?? m.type}</td>
                <td className="px-3 py-2">{m.method}</td>
                <td className="px-3 py-2">{m.concept}</td>
                <td className={`px-3 py-2 text-right font-semibold ${m.amount_cents_signed < 0 ? "text-accent-dark" : "text-emerald-700"}`}>
                  {formatCents(m.amount_cents_signed)}
                </td>
              </tr>
            ))}
            {d.movements.length === 0 && (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-center text-slate-500">
                  Sin movimientos.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export default function Cash() {
  const cash = useQuery({ queryKey: ["cash"], queryFn: salesApi.currentCash });
  const sessions = useQuery({ queryKey: ["cash-sessions"], queryFn: cashApi.sessions });
  const [manual, setManual] = useState<"INGRESO" | "EGRESO" | "RETIRO" | null>(null);
  const [closing, setClosing] = useState(false);
  const [viewing, setViewing] = useState<SessionRow | null>(null);

  const closed = sessions.data?.filter((s) => s.status === "CLOSED") ?? [];

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-brand">Caja</h1>

      {cash.data ? (
        <section className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-slate-600">Abierta desde {formatDateTime(cash.data.opened_at)}</p>
            <div className="flex flex-wrap gap-2">
              <button className="btn-ghost" onClick={() => setManual("INGRESO")}>
                + Ingreso
              </button>
              <button className="btn-ghost" onClick={() => setManual("EGRESO")}>
                − Egreso
              </button>
              <button className="btn-ghost" onClick={() => setManual("RETIRO")}>
                Retiro
              </button>
              <button className="btn-accent" onClick={() => setClosing(true)}>
                Cerrar caja
              </button>
            </div>
          </div>
          <SessionView id={cash.data.id} />
        </section>
      ) : (
        !cash.isPending && <OpenCashCard />
      )}

      <section>
        <h2 className="mb-2 text-lg font-bold text-brand">Historial de cierres</h2>
        <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
          <table className="w-full text-left text-sm">
            <thead className="bg-brand-soft text-brand">
              <tr>
                <th className="px-3 py-2">Apertura</th>
                <th className="px-3 py-2">Cierre</th>
                <th className="px-3 py-2 text-right">Esperado</th>
                <th className="px-3 py-2 text-right">Contado</th>
                <th className="px-3 py-2 text-right">Diferencia</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {closed.map((s) => (
                <tr key={s.id} className="border-t border-slate-100">
                  <td className="px-3 py-2">{formatDateTime(s.opened_at)}</td>
                  <td className="px-3 py-2">{s.closed_at ? formatDateTime(s.closed_at) : "—"}</td>
                  <td className="px-3 py-2 text-right">{formatCents(s.expected_cash_cents ?? 0)}</td>
                  <td className="px-3 py-2 text-right">{formatCents(s.counted_cash_cents ?? 0)}</td>
                  <td className={`px-3 py-2 text-right font-semibold ${(s.difference_cents ?? 0) === 0 ? "text-emerald-700" : "text-accent-dark"}`}>
                    {formatCents(s.difference_cents ?? 0)}
                  </td>
                  <td className="px-3 py-2 text-right">
                    <button className="btn-ghost min-h-9 px-3" onClick={() => setViewing(s)}>
                      Ver
                    </button>
                  </td>
                </tr>
              ))}
              {closed.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-3 py-6 text-center text-slate-500">
                    Aún no hay cierres.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>

      {manual && <ManualDialog kind={manual} onClose={() => setManual(null)} />}
      {closing && cash.data && <CloseDialog expected={cash.data.expected_cash_cents} onClose={() => setClosing(false)} />}
      {viewing && (
        <Modal title={`Caja del ${formatDateTime(viewing.opened_at)}`} onClose={() => setViewing(null)} wide>
          <SessionView id={viewing.id} />
        </Modal>
      )}
    </div>
  );
}
