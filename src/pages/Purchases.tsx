import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import {
  catalogApi,
  errorMessage,
  purchasesApi,
  type Product,
  type PurchaseDetail,
  type PurchaseStatus,
  type PurchaseSummary,
  type Supplier,
} from "../lib/api";
import { centsToInput, formatCents, formatDateTime, parseCents } from "../lib/money";

const STATUS_LABEL: Record<PurchaseStatus, string> = {
  DRAFT: "Borrador",
  CONFIRMED: "Confirmada",
  CANCELED: "Descartada",
};
const STATUS_STYLE: Record<PurchaseStatus, string> = {
  DRAFT: "bg-amber-100 text-amber-800",
  CONFIRMED: "bg-emerald-100 text-emerald-800",
  CANCELED: "bg-slate-200 text-slate-600",
};

interface Line {
  product: Pick<Product, "id" | "name" | "presentations">;
  presentationId: string;
  qty: string;
  cost: string;
}

function ProductPicker({ onPick }: { onPick: (p: Product) => void }) {
  const [q, setQ] = useState("");
  const results = useQuery({
    queryKey: ["products", "picker", q],
    queryFn: () => catalogApi.products({ search: q }),
    enabled: q.trim().length > 0,
  });
  return (
    <div className="relative">
      <input className="input" placeholder="Buscar producto para agregar (nombre, SKU o código)…" value={q} onChange={(e) => setQ(e.target.value)} />
      {q.trim() && results.data && (
        <div className="absolute z-10 mt-1 max-h-60 w-full overflow-y-auto rounded-lg border border-slate-200 bg-white shadow-lg">
          {results.data.length === 0 && <div className="p-3 text-slate-500">Sin resultados</div>}
          {results.data.map((p) => (
            <button
              key={p.id}
              className="block w-full px-4 py-2 text-left hover:bg-brand-soft"
              onClick={() => {
                onPick(p);
                setQ("");
              }}
            >
              <span className="font-medium">{p.name}</span> <span className="font-mono text-xs text-slate-500">{p.sku}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function PurchaseForm({ purchaseId, onClose }: { purchaseId: string | null; onClose: () => void }) {
  const qc = useQueryClient();
  const suppliers = useQuery({ queryKey: ["suppliers"], queryFn: purchasesApi.suppliers });
  const [supplierId, setSupplierId] = useState("");
  const [lines, setLines] = useState<Line[]>([]);
  const [loaded, setLoaded] = useState(!purchaseId);
  const [error, setError] = useState<string | null>(null);

  // Carga del borrador existente
  useQuery({
    queryKey: ["purchase", purchaseId, "form"],
    enabled: !!purchaseId && !loaded,
    queryFn: async () => {
      const d = await purchasesApi.get(purchaseId!);
      const prods = await catalogApi.products({ include_inactive: true });
      setSupplierId(d.supplier_id ?? "");
      setLines(
        d.items.map((it) => ({
          product: prods.find((p) => p.id === it.product_id) ?? { id: it.product_id, name: it.product_name, presentations: [] },
          presentationId: it.presentation_id,
          qty: String(it.qty_presentations),
          cost: centsToInput(it.unit_cost_cents),
        })),
      );
      setLoaded(true);
      return d;
    },
  });

  const addProduct = (p: Product) => {
    const pres = p.presentations.filter((x) => x.active);
    const first = pres.find((x) => x.factor_to_base === 1) ?? pres[0];
    if (!first) return;
    setLines((ls) => [
      ...ls,
      { product: p, presentationId: first.id, qty: "1", cost: centsToInput(p.reference_cost_cents * first.factor_to_base) },
    ]);
  };

  const patch = (i: number, l: Partial<Line>) => setLines((ls) => ls.map((x, j) => (j === i ? { ...x, ...l } : x)));

  const total = lines.reduce((acc, l) => acc + (Number(l.qty) || 0) * (parseCents(l.cost) ?? 0), 0);

  const persist = async () => {
    const items = lines.map((l) => {
      const cost = parseCents(l.cost);
      const qty = Number(l.qty);
      if (cost === null) throw { message: `Costo inválido en «${l.product.name}»` };
      if (!Number.isInteger(qty) || qty <= 0) throw { message: `Cantidad inválida en «${l.product.name}»` };
      return { product_id: l.product.id, presentation_id: l.presentationId, qty_presentations: qty, unit_cost_cents: cost };
    });
    return purchasesApi.save({ id: purchaseId, supplier_id: supplierId || null, items });
  };

  const done = () => {
    qc.invalidateQueries({ queryKey: ["purchases"] });
    qc.invalidateQueries({ queryKey: ["products"] });
    onClose();
  };

  const saveDraft = useMutation({ mutationFn: persist, onSuccess: done, onError: (e) => setError(errorMessage(e)) });
  const confirm = useMutation({
    mutationFn: async () => purchasesApi.confirm(await persist()),
    onSuccess: done,
    onError: (e) => {
      setError(errorMessage(e));
      qc.invalidateQueries({ queryKey: ["purchases"] });
    },
  });
  const busy = saveDraft.isPending || confirm.isPending;

  return (
    <Modal title={purchaseId ? "Editar compra (borrador)" : "Nueva compra"} onClose={onClose} wide>
      <div className="space-y-4">
        <div>
          <label className="label">Proveedor (opcional)</label>
          <select className="input" value={supplierId} onChange={(e) => setSupplierId(e.target.value)}>
            <option value="">Sin proveedor</option>
            {suppliers.data?.filter((s) => s.active).map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
        <ProductPicker onPick={addProduct} />

        <div className="overflow-hidden rounded-lg border border-slate-200">
          <table className="w-full text-left text-sm">
            <thead className="bg-brand-soft text-brand">
              <tr>
                <th className="px-3 py-2">Producto</th>
                <th className="px-3 py-2">Presentación</th>
                <th className="w-24 px-3 py-2">Cant.</th>
                <th className="w-32 px-3 py-2">Costo c/u (S/)</th>
                <th className="px-3 py-2 text-right">Total</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {lines.map((l, i) => (
                <tr key={i} className="border-t border-slate-100">
                  <td className="px-3 py-2 font-medium">{l.product.name}</td>
                  <td className="px-3 py-2">
                    <select
                      className="input min-h-10"
                      value={l.presentationId}
                      onChange={(e) => {
                        const pr = l.product.presentations.find((x) => x.id === e.target.value);
                        const ref = (l.product as Product).reference_cost_cents;
                        // sugerir costo según el último costo de referencia
                        patch(i, pr && ref !== undefined ? { presentationId: pr.id, cost: centsToInput(ref * pr.factor_to_base) } : { presentationId: e.target.value });
                      }}
                    >
                      {l.product.presentations.filter((x) => x.active).map((x) => (
                        <option key={x.id} value={x.id}>
                          {x.name} (×{x.factor_to_base})
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="px-3 py-2">
                    <input className="input min-h-10" type="number" min={1} value={l.qty} onChange={(e) => patch(i, { qty: e.target.value })} />
                  </td>
                  <td className="px-3 py-2">
                    <input className="input min-h-10" inputMode="decimal" value={l.cost} onChange={(e) => patch(i, { cost: e.target.value })} />
                  </td>
                  <td className="px-3 py-2 text-right">{formatCents((Number(l.qty) || 0) * (parseCents(l.cost) ?? 0))}</td>
                  <td className="px-2">
                    <button className="btn-ghost min-h-10 px-3" onClick={() => setLines(lines.filter((_, j) => j !== i))} aria-label="Quitar">
                      ✕
                    </button>
                  </td>
                </tr>
              ))}
              {lines.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-3 py-6 text-center text-slate-500">
                    Busca y agrega productos.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        <div className="text-right text-lg font-bold text-brand">Total: {formatCents(total)}</div>
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-ghost" disabled={busy || lines.length === 0} onClick={() => saveDraft.mutate()}>
            Guardar borrador
          </button>
          <button
            className="btn-accent"
            disabled={busy || lines.length === 0}
            onClick={() => {
              if (window.confirm("¿Confirmar compra? Aumentará el stock y no podrá editarse.")) confirm.mutate();
            }}
          >
            Confirmar compra
          </button>
        </div>
      </div>
    </Modal>
  );
}

function PurchaseDetailDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const d = useQuery({ queryKey: ["purchase", id], queryFn: () => purchasesApi.get(id) });
  const data: PurchaseDetail | undefined = d.data;
  return (
    <Modal title={data ? `Compra ${data.number}` : "Compra"} onClose={onClose} wide>
      {data && (
        <div className="space-y-3">
          <p className="text-sm text-slate-600">
            {formatDateTime(data.purchased_at)} · {data.supplier_name ?? "Sin proveedor"} ·{" "}
            <span className={`rounded px-2 py-0.5 text-xs ${STATUS_STYLE[data.status]}`}>{STATUS_LABEL[data.status]}</span>
          </p>
          <table className="w-full text-left text-sm">
            <thead className="bg-brand-soft text-brand">
              <tr>
                <th className="px-3 py-2">Producto</th>
                <th className="px-3 py-2">Presentación</th>
                <th className="px-3 py-2 text-right">Cant.</th>
                <th className="px-3 py-2 text-right">Unidades base</th>
                <th className="px-3 py-2 text-right">Costo c/u</th>
                <th className="px-3 py-2 text-right">Total</th>
              </tr>
            </thead>
            <tbody>
              {data.items.map((it) => (
                <tr key={it.id} className="border-t border-slate-100">
                  <td className="px-3 py-2">{it.product_name}</td>
                  <td className="px-3 py-2">{it.presentation_name}</td>
                  <td className="px-3 py-2 text-right">{it.qty_presentations}</td>
                  <td className="px-3 py-2 text-right">{it.qty_presentations * it.factor_snapshot}</td>
                  <td className="px-3 py-2 text-right">{formatCents(it.unit_cost_cents)}</td>
                  <td className="px-3 py-2 text-right">{formatCents(it.total_cents)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="text-right text-lg font-bold text-brand">Total: {formatCents(data.total_cents)}</div>
        </div>
      )}
    </Modal>
  );
}

function PurchasesTab() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<PurchaseStatus | "">("");
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [viewing, setViewing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const list = useQuery({ queryKey: ["purchases", status], queryFn: () => purchasesApi.list(status || undefined) });
  const discard = useMutation({
    mutationFn: (p: PurchaseSummary) => purchasesApi.cancelDraft(p.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["purchases"] }),
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <select className="input max-w-52" value={status} onChange={(e) => setStatus(e.target.value as PurchaseStatus | "")}>
          <option value="">Todas</option>
          <option value="DRAFT">Borradores</option>
          <option value="CONFIRMED">Confirmadas</option>
          <option value="CANCELED">Descartadas</option>
        </select>
        <button className="btn-accent" onClick={() => setEditing("new")}>
          + Nueva compra
        </button>
      </div>
      {error && <p className="mb-3 rounded-lg bg-red-50 p-3 text-red-800">{error}</p>}
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">N°</th>
              <th className="px-4 py-3">Fecha</th>
              <th className="px-4 py-3">Proveedor</th>
              <th className="px-4 py-3">Estado</th>
              <th className="px-4 py-3 text-right">Total</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {list.data?.map((p) => (
              <tr key={p.id} className="border-t border-slate-100">
                <td className="px-4 py-3 font-mono text-sm">{p.number}</td>
                <td className="px-4 py-3">{formatDateTime(p.purchased_at)}</td>
                <td className="px-4 py-3">{p.supplier_name ?? "—"}</td>
                <td className="px-4 py-3">
                  <span className={`rounded px-2 py-0.5 text-xs ${STATUS_STYLE[p.status]}`}>{STATUS_LABEL[p.status]}</span>
                </td>
                <td className="px-4 py-3 text-right font-semibold">{formatCents(p.total_cents)}</td>
                <td className="space-x-2 px-4 py-3 text-right">
                  {p.status === "DRAFT" ? (
                    <>
                      <button className="btn-primary min-h-10 px-3" onClick={() => setEditing(p.id)}>
                        Editar
                      </button>
                      <button
                        className="btn-ghost min-h-10 px-3"
                        onClick={() => window.confirm(`¿Descartar el borrador ${p.number}?`) && discard.mutate(p)}
                      >
                        Descartar
                      </button>
                    </>
                  ) : (
                    <button className="btn-ghost min-h-10 px-3" onClick={() => setViewing(p.id)}>
                      Ver
                    </button>
                  )}
                </td>
              </tr>
            ))}
            {list.data?.length === 0 && (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-center text-slate-500">
                  Sin compras registradas.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {editing && <PurchaseForm purchaseId={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      {viewing && <PurchaseDetailDialog id={viewing} onClose={() => setViewing(null)} />}
    </div>
  );
}

function SupplierForm({ supplier, onClose }: { supplier: Supplier | null; onClose: () => void }) {
  const qc = useQueryClient();
  const [name, setName] = useState(supplier?.name ?? "");
  const [taxId, setTaxId] = useState(supplier?.tax_id ?? "");
  const [phone, setPhone] = useState(supplier?.phone ?? "");
  const [notes, setNotes] = useState(supplier?.notes ?? "");
  const [active, setActive] = useState(supplier?.active ?? true);
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: () =>
      purchasesApi.saveSupplier({ id: supplier?.id ?? null, name, tax_id: taxId || null, phone: phone || null, notes: notes || null, active }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["suppliers"] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Modal title={supplier ? "Editar proveedor" : "Nuevo proveedor"} onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <div>
          <label className="label">Nombre comercial *</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="label">RUC / identificador</label>
            <input className="input" value={taxId} onChange={(e) => setTaxId(e.target.value)} />
          </div>
          <div>
            <label className="label">Teléfono</label>
            <input className="input" value={phone} onChange={(e) => setPhone(e.target.value)} />
          </div>
        </div>
        <div>
          <label className="label">Nota</label>
          <input className="input" value={notes} onChange={(e) => setNotes(e.target.value)} />
        </div>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={active} onChange={(e) => setActive(e.target.checked)} /> Activo
        </label>
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button type="button" className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button type="submit" className="btn-accent" disabled={save.isPending}>
            Guardar
          </button>
        </div>
      </form>
    </Modal>
  );
}

function SuppliersTab() {
  const qc = useQueryClient();
  const [editing, setEditing] = useState<Supplier | "new" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const suppliers = useQuery({ queryKey: ["suppliers"], queryFn: purchasesApi.suppliers });
  const refresh = () => qc.invalidateQueries({ queryKey: ["suppliers"] });
  const toggle = useMutation({
    mutationFn: (s: Supplier) => purchasesApi.saveSupplier({ ...s, active: !s.active }),
    onSuccess: refresh,
    onError: (e) => setError(errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: (s: Supplier) => purchasesApi.deleteSupplier(s.id),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <div>
      <div className="mb-4 flex justify-end">
        <button className="btn-accent" onClick={() => setEditing("new")}>
          + Nuevo proveedor
        </button>
      </div>
      {error && <p className="mb-3 rounded-lg bg-red-50 p-3 text-red-800">{error}</p>}
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">Proveedor</th>
              <th className="px-4 py-3">RUC</th>
              <th className="px-4 py-3">Teléfono</th>
              <th className="px-4 py-3">Estado</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {suppliers.data?.map((s) => (
              <tr key={s.id} className={`border-t border-slate-100 ${s.active ? "" : "text-slate-400"}`}>
                <td className="px-4 py-3 font-medium">{s.name}</td>
                <td className="px-4 py-3">{s.tax_id ?? "—"}</td>
                <td className="px-4 py-3">{s.phone ?? "—"}</td>
                <td className="px-4 py-3">{s.active ? "Activo" : "Inactivo"}</td>
                <td className="space-x-2 px-4 py-3 text-right">
                  <button className="btn-ghost min-h-10 px-3" onClick={() => setEditing(s)}>
                    Editar
                  </button>
                  <button className="btn-ghost min-h-10 px-3" onClick={() => toggle.mutate(s)}>
                    {s.active ? "Desactivar" : "Activar"}
                  </button>
                  <button
                    className="btn-ghost min-h-10 px-3 text-red-700"
                    onClick={() => window.confirm(`¿Eliminar a «${s.name}»?`) && remove.mutate(s)}
                  >
                    Eliminar
                  </button>
                </td>
              </tr>
            ))}
            {suppliers.data?.length === 0 && (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-slate-500">
                  Aún no hay proveedores.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {editing && <SupplierForm supplier={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

export default function Purchases() {
  const [tab, setTab] = useState<"compras" | "proveedores">("compras");
  return (
    <div>
      <h1 className="mb-4 text-2xl font-bold text-brand">Compras / Proveedores</h1>
      <div className="mb-4 flex gap-2">
        {(["compras", "proveedores"] as const).map((t) => (
          <button key={t} className={tab === t ? "btn-primary" : "btn-ghost"} onClick={() => setTab(t)}>
            {t === "compras" ? "Compras" : "Proveedores"}
          </button>
        ))}
      </div>
      {tab === "compras" ? <PurchasesTab /> : <SuppliersTab />}
    </div>
  );
}
