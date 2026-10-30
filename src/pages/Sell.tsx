import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import Ticket from "../components/Ticket";
import {
  catalogApi,
  errorMessage,
  salesApi,
  type PayMethod,
  type Product,
  type SaleDetail,
  type SaleInput,
} from "../lib/api";
import { centsToInput, formatCents, formatDateTime, parseCents } from "../lib/money";

interface CartLine {
  product: Product;
  presentationId: string;
  qty: number;
  discount: string; // S/ de la línea
}

const METHODS: { id: PayMethod; label: string }[] = [
  { id: "EFECTIVO", label: "Efectivo" },
  { id: "YAPE", label: "Yape" },
  { id: "PLIN", label: "Plin" },
  { id: "OTRO", label: "Otro" },
];

function OpenCashCard() {
  const qc = useQueryClient();
  const [amount, setAmount] = useState("0.00");
  const [error, setError] = useState<string | null>(null);
  const open = useMutation({
    mutationFn: async () => {
      const cents = parseCents(amount);
      if (cents === null) throw { message: "Monto inválido" };
      return salesApi.openCash(cents);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["cash"] }),
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <div className="mx-auto mt-16 max-w-md rounded-2xl bg-white p-8 shadow">
      <h1 className="text-2xl font-bold text-brand">Abre la caja para vender</h1>
      <p className="mt-1 text-slate-600">Indica el efectivo con el que inicias el turno.</p>
      <label className="label mt-6">Saldo inicial (S/)</label>
      <input className="input" inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus />
      {error && <p className="mt-3 text-red-700">{error}</p>}
      <button className="btn-accent mt-6 w-full" disabled={open.isPending} onClick={() => open.mutate()}>
        Abrir caja
      </button>
    </div>
  );
}

interface PayRow {
  method: PayMethod;
  amount: string;
  received: string;
  reference: string;
}

function PaymentDialog({
  total,
  busy,
  error,
  onConfirm,
  onClose,
}: {
  total: number;
  busy: boolean;
  error: string | null;
  onConfirm: (rows: SaleInput["payments"]) => void;
  onClose: () => void;
}) {
  const [rows, setRows] = useState<PayRow[]>([{ method: "EFECTIVO", amount: centsToInput(total), received: "", reference: "" }]);
  const [localError, setLocalError] = useState<string | null>(null);

  const parsed = rows.map((r) => ({ ...r, cents: parseCents(r.amount) ?? 0, recv: r.received.trim() === "" ? null : parseCents(r.received) }));
  const paid = parsed.reduce((a, r) => a + r.cents, 0);
  const remaining = total - paid;

  const setRow = (i: number, patch: Partial<PayRow>) => setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  function submit() {
    setLocalError(null);
    if (remaining !== 0) return setLocalError("Los pagos deben sumar el total");
    const payments: SaleInput["payments"] = [];
    for (const r of parsed) {
      if (r.cents <= 0) return setLocalError("Cada pago debe ser mayor a cero");
      if (r.method === "EFECTIVO" && r.recv !== null && r.recv < r.cents) return setLocalError("El efectivo recibido no cubre el importe");
      payments.push({
        method: r.method,
        amount_cents: r.cents,
        received_cents: r.method === "EFECTIVO" ? (r.recv ?? r.cents) : null,
        reference: r.reference.trim() || null,
      });
    }
    onConfirm(payments);
  }

  return (
    <Modal title={`Cobrar ${formatCents(total)}`} onClose={onClose}>
      <div className="space-y-4">
        {rows.map((r, i) => {
          const p = parsed[i];
          const change = r.method === "EFECTIVO" && p.recv !== null && p.recv >= p.cents ? p.recv - p.cents : null;
          return (
            <div key={i} className="space-y-2 rounded-lg border border-slate-200 p-3">
              <div className="flex gap-2">
                {METHODS.map((m) => (
                  <button key={m.id} className={r.method === m.id ? "btn-primary min-h-10 flex-1 px-2" : "btn-ghost min-h-10 flex-1 px-2"} onClick={() => setRow(i, { method: m.id })}>
                    {m.label}
                  </button>
                ))}
                {rows.length > 1 && (
                  <button className="btn-ghost min-h-10 px-3" onClick={() => setRows(rows.filter((_, j) => j !== i))} aria-label="Quitar">
                    ✕
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="label">Importe (S/)</label>
                  <input className="input" inputMode="decimal" value={r.amount} onChange={(e) => setRow(i, { amount: e.target.value })} />
                </div>
                {r.method === "EFECTIVO" ? (
                  <div>
                    <label className="label">Recibido (S/)</label>
                    <input className="input" inputMode="decimal" value={r.received} placeholder={r.amount} onChange={(e) => setRow(i, { received: e.target.value })} autoFocus={i === 0} />
                  </div>
                ) : (
                  <div>
                    <label className="label">Referencia (opcional)</label>
                    <input className="input" value={r.reference} onChange={(e) => setRow(i, { reference: e.target.value })} />
                  </div>
                )}
              </div>
              {r.method === "EFECTIVO" && (
                <div className="flex flex-wrap items-center gap-2">
                  {[10, 20, 50, 100, 200].map((b) => (
                    <button key={b} className="btn-ghost min-h-9 px-3 text-sm" onClick={() => setRow(i, { received: centsToInput(b * 100) })}>
                      S/ {b}
                    </button>
                  ))}
                  <button className="btn-ghost min-h-9 px-3 text-sm" onClick={() => setRow(i, { received: r.amount })}>
                    Exacto
                  </button>
                  {change !== null && <span className="ml-auto text-lg font-bold text-emerald-700">Vuelto {formatCents(change)}</span>}
                </div>
              )}
            </div>
          );
        })}
        <div className="flex items-center justify-between">
          <button
            className="btn-ghost min-h-10"
            disabled={remaining <= 0}
            onClick={() => setRows([...rows, { method: "YAPE", amount: centsToInput(remaining), received: "", reference: "" }])}
          >
            + Otro método
          </button>
          <span className={remaining === 0 ? "text-emerald-700" : "font-semibold text-accent-dark"}>
            {remaining === 0 ? "Pago completo" : remaining > 0 ? `Falta ${formatCents(remaining)}` : `Sobra ${formatCents(-remaining)}`}
          </span>
        </div>
        {(localError || error) && <p className="text-red-700">{localError ?? error}</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Volver
          </button>
          <button className="btn-accent" disabled={busy} onClick={submit}>
            Confirmar venta
          </button>
        </div>
      </div>
    </Modal>
  );
}

export default function Sell({ businessName }: { businessName: string }) {
  const qc = useQueryClient();
  const cash = useQuery({ queryKey: ["cash"], queryFn: salesApi.currentCash });
  const [q, setQ] = useState("");
  const [lines, setLines] = useState<CartLine[]>([]);
  const [headerDiscount, setHeaderDiscount] = useState("");
  const [paying, setPaying] = useState(false);
  const [done, setDone] = useState<SaleDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(crypto.randomUUID());
  const searchRef = useRef<HTMLInputElement>(null);

  const results = useQuery({
    queryKey: ["products", "sell", q],
    queryFn: () => catalogApi.products({ search: q }),
    enabled: q.trim().length > 0,
  });

  const price = (l: CartLine) => l.product.presentations.find((p) => p.id === l.presentationId)?.sale_price_cents ?? 0;
  const factor = (l: CartLine) => l.product.presentations.find((p) => p.id === l.presentationId)?.factor_to_base ?? 1;
  const lineDiscount = (l: CartLine) => parseCents(l.discount || "0") ?? 0;
  const lineTotal = (l: CartLine) => l.qty * price(l) - lineDiscount(l);

  const subtotal = lines.reduce((a, l) => a + lineTotal(l), 0);
  const hDisc = parseCents(headerDiscount || "0") ?? 0;
  const total = subtotal - hDisc;
  const cartValid = lines.length > 0 && total > 0 && lines.every((l) => lineTotal(l) >= 0) && hDisc <= subtotal;

  // Unidades base pedidas por producto vs stock disponible (aviso; el servidor valida)
  const stockWarn = useMemo(() => {
    const need = new Map<string, number>();
    for (const l of lines) need.set(l.product.id, (need.get(l.product.id) ?? 0) + l.qty * factor(l));
    return new Set(lines.filter((l) => (need.get(l.product.id) ?? 0) > l.product.stock).map((l) => l.product.id));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lines]);

  function add(p: Product) {
    const pres = p.presentations.filter((x) => x.active);
    const first = pres.find((x) => x.factor_to_base === 1) ?? pres[0];
    if (!first) return;
    setLines((ls) => {
      const i = ls.findIndex((l) => l.product.id === p.id && l.presentationId === first.id);
      if (i >= 0) return ls.map((l, j) => (j === i ? { ...l, qty: l.qty + 1 } : l));
      return [...ls, { product: p, presentationId: first.id, qty: 1, discount: "" }];
    });
    setQ("");
    searchRef.current?.focus();
  }

  async function onEnter() {
    const term = q.trim();
    if (!term) return;
    const found = await catalogApi.products({ search: term });
    const lower = term.toLowerCase();
    const exact = found.find((p) => p.barcode === term || p.sku.toLowerCase() === lower);
    const pick = exact ?? found[0];
    if (pick) add(pick);
  }

  const patch = (i: number, l: Partial<CartLine>) => setLines((ls) => ls.map((x, j) => (j === i ? { ...x, ...l } : x)));

  const confirm = useMutation({
    mutationFn: (payments: SaleInput["payments"]) =>
      salesApi.confirm({
        client_request_id: requestId.current,
        discount_cents: hDisc,
        items: lines.map((l) => ({
          product_id: l.product.id,
          presentation_id: l.presentationId,
          qty_presentations: l.qty,
          discount_cents: lineDiscount(l),
        })),
        payments,
      }),
    onSuccess: (sale) => {
      setDone(sale);
      setPaying(false);
      setLines([]);
      setHeaderDiscount("");
      setError(null);
      requestId.current = crypto.randomUUID();
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["cash"] });
      qc.invalidateQueries({ queryKey: ["sales"] });
    },
    // Se conserva requestId: reintentar tras un error de red nunca duplica la venta (RN-05)
    onError: (e) => setError(errorMessage(e)),
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F2") searchRef.current?.focus();
      if (e.key === "F9" && cartValid && !done) setPaying(true);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cartValid, done]);

  if (cash.isPending) return <p>Cargando…</p>;
  if (!cash.data) return <OpenCashCard />;

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-2xl font-bold text-brand">Vender</h1>
        <div className="rounded-lg bg-brand-soft px-4 py-2 text-sm text-brand">
          Caja abierta {formatDateTime(cash.data.opened_at)} · Efectivo esperado <b>{formatCents(cash.data.expected_cash_cents)}</b>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-6 xl:grid-cols-[1fr_28rem]">
        <section>
          <input
            ref={searchRef}
            className="input text-lg"
            placeholder="Buscar o escanear (nombre, SKU, código)…  F2"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && onEnter()}
            autoFocus
          />
          <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-2">
            {q.trim() && results.data?.length === 0 && <p className="text-slate-500">Sin resultados.</p>}
            {results.data?.map((p) => {
              const base = p.presentations.find((x) => x.active && x.factor_to_base === 1) ?? p.presentations.find((x) => x.active);
              return (
                <button
                  key={p.id}
                  disabled={!base || p.stock <= 0}
                  className="rounded-xl border border-slate-200 bg-white p-4 text-left hover:border-accent disabled:opacity-50"
                  onClick={() => add(p)}
                >
                  <div className="font-semibold">{p.name}</div>
                  <div className="mt-1 flex justify-between text-sm">
                    <span className="text-slate-500">
                      Stock {p.stock} {p.base_unit}
                    </span>
                    <span className="font-bold text-brand">{base ? formatCents(base.sale_price_cents) : "—"}</span>
                  </div>
                </button>
              );
            })}
          </div>
          {!q.trim() && <p className="mt-6 text-slate-500">Escribe o escanea un producto y pulsa Enter para agregarlo al carrito.</p>}
        </section>

        <section className="rounded-2xl border border-slate-200 bg-white p-4">
          <h2 className="mb-3 font-bold text-brand">Carrito</h2>
          <div className="max-h-[50vh] space-y-3 overflow-y-auto">
            {lines.map((l, i) => (
              <div key={i} className={`rounded-lg border p-3 ${stockWarn.has(l.product.id) ? "border-accent bg-accent/5" : "border-slate-200"}`}>
                <div className="flex justify-between gap-2">
                  <span className="font-medium">{l.product.name}</span>
                  <button className="text-slate-400 hover:text-red-700" onClick={() => setLines(lines.filter((_, j) => j !== i))} aria-label="Quitar">
                    ✕
                  </button>
                </div>
                <div className="mt-2 flex items-center gap-2">
                  <button className="btn-ghost min-h-10 w-10 px-0" onClick={() => patch(i, { qty: Math.max(1, l.qty - 1) })}>
                    −
                  </button>
                  <input
                    className="input min-h-10 w-16 px-2 text-center"
                    type="number"
                    min={1}
                    value={l.qty}
                    onChange={(e) => patch(i, { qty: Math.max(1, Math.floor(Number(e.target.value)) || 1) })}
                  />
                  <button className="btn-ghost min-h-10 w-10 px-0" onClick={() => patch(i, { qty: l.qty + 1 })}>
                    +
                  </button>
                  <select className="input min-h-10 flex-1 px-2" value={l.presentationId} onChange={(e) => patch(i, { presentationId: e.target.value })}>
                    {l.product.presentations.filter((p) => p.active).map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name} · {formatCents(p.sale_price_cents)}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="mt-2 flex items-center justify-between gap-2 text-sm">
                  <label className="flex items-center gap-1 text-slate-600">
                    Desc. S/
                    <input className="input min-h-9 w-20 px-2" inputMode="decimal" value={l.discount} onChange={(e) => patch(i, { discount: e.target.value })} />
                  </label>
                  <span className="text-base font-bold">{formatCents(lineTotal(l))}</span>
                </div>
                {stockWarn.has(l.product.id) && <p className="mt-1 text-xs text-accent-dark">Stock insuficiente (disponible: {l.product.stock})</p>}
              </div>
            ))}
            {lines.length === 0 && <p className="py-8 text-center text-slate-500">Carrito vacío</p>}
          </div>

          <div className="mt-4 space-y-2 border-t border-slate-200 pt-3">
            <div className="flex justify-between text-sm">
              <span>Subtotal</span>
              <span>{formatCents(subtotal)}</span>
            </div>
            <label className="flex items-center justify-between text-sm">
              Descuento total S/
              <input className="input min-h-9 w-24 px-2" inputMode="decimal" value={headerDiscount} onChange={(e) => setHeaderDiscount(e.target.value)} />
            </label>
            <div className="flex justify-between text-2xl font-bold text-brand">
              <span>Total</span>
              <span>{formatCents(Math.max(total, 0))}</span>
            </div>
            <button className="btn-accent w-full text-lg" disabled={!cartValid} onClick={() => { setError(null); setPaying(true); }}>
              Cobrar (F9)
            </button>
            {lines.length > 0 && (
              <button className="btn-ghost w-full min-h-10" onClick={() => setLines([])}>
                Vaciar carrito
              </button>
            )}
          </div>
        </section>
      </div>

      {paying && (
        <PaymentDialog total={total} busy={confirm.isPending} error={error} onConfirm={(p) => confirm.mutate(p)} onClose={() => setPaying(false)} />
      )}

      {done && (
        <Modal
          title="Venta realizada"
          onClose={() => {
            setDone(null);
            searchRef.current?.focus();
          }}
        >
          <div className="space-y-4">
            {done.payments.some((p) => p.method === "EFECTIVO" && (p.change_cents ?? 0) > 0) && (
              <div className="rounded-xl bg-emerald-50 p-4 text-center text-emerald-800">
                <div className="text-sm">Entrega de vuelto</div>
                <div className="text-3xl font-bold">{formatCents(done.payments.reduce((a, p) => a + (p.change_cents ?? 0), 0))}</div>
              </div>
            )}
            <Ticket sale={done} businessName={businessName} />
            <div className="flex justify-end gap-3">
              <button className="btn-ghost" onClick={() => window.print()}>
                Imprimir ticket
              </button>
              <button
                className="btn-accent"
                autoFocus
                onClick={() => {
                  setDone(null);
                  searchRef.current?.focus();
                }}
              >
                Nueva venta
              </button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
