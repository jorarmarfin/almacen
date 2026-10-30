import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import { catalogApi, errorMessage, type Product } from "../lib/api";
import { formatDateTime } from "../lib/money";

const TYPE_LABEL: Record<string, string> = {
  COMPRA: "Compra",
  VENTA: "Venta",
  AJUSTE_ENTRADA: "Ajuste entrada",
  AJUSTE_SALIDA: "Ajuste salida",
  ANULACION_VENTA: "Anulación venta",
  ANULACION_COMPRA: "Anulación compra",
};

function AdjustDialog({ product, onClose }: { product: Product; onClose: () => void }) {
  const qc = useQueryClient();
  const [kind, setKind] = useState<"ENTRADA" | "SALIDA">("ENTRADA");
  const [qty, setQty] = useState("");
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);

  const adjust = useMutation({
    mutationFn: () => catalogApi.adjustStock(product.id, kind, Number(qty), reason),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["kardex", product.id] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  const n = Number(qty);
  const valid = Number.isInteger(n) && n > 0 && reason.trim() !== "";
  const after = product.stock + (kind === "ENTRADA" ? n : -n);

  return (
    <Modal title={`Ajustar stock — ${product.name}`} onClose={onClose}>
      <div className="space-y-4">
        <div className="flex gap-2">
          {(["ENTRADA", "SALIDA"] as const).map((k) => (
            <button key={k} className={kind === k ? "btn-primary flex-1" : "btn-ghost flex-1"} onClick={() => setKind(k)}>
              {k === "ENTRADA" ? "Entrada (+)" : "Salida (−)"}
            </button>
          ))}
        </div>
        <div>
          <label className="label">Cantidad ({product.base_unit})</label>
          <input className="input" type="number" min={1} value={qty} onChange={(e) => setQty(e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">Motivo * (STOCK_INICIAL, merma, conteo…)</label>
          <input className="input" value={reason} onChange={(e) => setReason(e.target.value)} />
        </div>
        {valid && (
          <p className="text-sm text-slate-600">
            Stock: {product.stock} → <b className={after < 0 ? "text-red-700" : ""}>{after}</b>
          </p>
        )}
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-accent" disabled={!valid || adjust.isPending} onClick={() => adjust.mutate()}>
            Confirmar ajuste
          </button>
        </div>
      </div>
    </Modal>
  );
}

function KardexDialog({ product, onClose }: { product: Product; onClose: () => void }) {
  const kardex = useQuery({ queryKey: ["kardex", product.id], queryFn: () => catalogApi.kardex(product.id) });
  return (
    <Modal title={`Kardex — ${product.name}`} onClose={onClose} wide>
      <div className="max-h-[60vh] overflow-y-auto rounded-lg border border-slate-200">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 bg-brand-soft text-brand">
            <tr>
              <th className="px-3 py-2">Fecha</th>
              <th className="px-3 py-2">Tipo</th>
              <th className="px-3 py-2 text-right">Cantidad</th>
              <th className="px-3 py-2 text-right">Antes → Después</th>
              <th className="px-3 py-2">Motivo</th>
            </tr>
          </thead>
          <tbody>
            {kardex.data?.map((m) => (
              <tr key={m.id} className="border-t border-slate-100">
                <td className="px-3 py-2">{formatDateTime(m.occurred_at)}</td>
                <td className="px-3 py-2">{TYPE_LABEL[m.type] ?? m.type}</td>
                <td className={`px-3 py-2 text-right font-semibold ${m.qty_base_signed < 0 ? "text-accent-dark" : "text-emerald-700"}`}>
                  {m.qty_base_signed > 0 ? "+" : ""}
                  {m.qty_base_signed}
                </td>
                <td className="px-3 py-2 text-right">
                  {m.balance_before ?? "—"} → {m.balance_after ?? "—"}
                </td>
                <td className="px-3 py-2">{m.note ?? ""}</td>
              </tr>
            ))}
            {kardex.data?.length === 0 && (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-center text-slate-500">
                  Sin movimientos.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </Modal>
  );
}

export default function Inventory() {
  const [search, setSearch] = useState("");
  const [lowOnly, setLowOnly] = useState(false);
  const [adjusting, setAdjusting] = useState<Product | null>(null);
  const [viewing, setViewing] = useState<Product | null>(null);

  const products = useQuery({
    queryKey: ["products", "inv", search, lowOnly],
    queryFn: () => catalogApi.products({ search, low_stock_only: lowOnly }),
  });

  return (
    <div>
      <h1 className="mb-4 text-2xl font-bold text-brand">Inventario</h1>
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <input className="input max-w-sm" placeholder="Buscar producto…" value={search} onChange={(e) => setSearch(e.target.value)} />
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={lowOnly} onChange={(e) => setLowOnly(e.target.checked)} /> Solo por reponer
        </label>
      </div>
      {products.isError && <p className="text-red-700">{errorMessage(products.error)}</p>}
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">Producto</th>
              <th className="px-4 py-3 text-right">Stock</th>
              <th className="px-4 py-3 text-right">Mínimo</th>
              <th className="px-4 py-3" />
            </tr>
          </thead>
          <tbody>
            {products.data?.map((p) => (
              <tr key={p.id} className="border-t border-slate-100">
                <td className="px-4 py-3">
                  <div className="font-medium">{p.name}</div>
                  <div className="font-mono text-xs text-slate-500">{p.sku}</div>
                </td>
                <td className={`px-4 py-3 text-right font-semibold ${p.stock <= p.min_stock ? "text-accent-dark" : ""}`}>
                  {p.stock} {p.base_unit}
                  {p.stock <= p.min_stock && <span className="ml-2 rounded bg-accent/10 px-2 py-0.5 text-xs">Reponer</span>}
                </td>
                <td className="px-4 py-3 text-right">{p.min_stock}</td>
                <td className="space-x-2 px-4 py-3 text-right">
                  <button className="btn-ghost min-h-10 px-3" onClick={() => setViewing(p)}>
                    Kardex
                  </button>
                  <button className="btn-primary min-h-10 px-3" onClick={() => setAdjusting(p)}>
                    Ajustar
                  </button>
                </td>
              </tr>
            ))}
            {products.data?.length === 0 && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-slate-500">
                  Sin resultados.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {adjusting && <AdjustDialog product={adjusting} onClose={() => setAdjusting(null)} />}
      {viewing && <KardexDialog product={viewing} onClose={() => setViewing(null)} />}
    </div>
  );
}
