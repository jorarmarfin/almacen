import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import { catalogApi, errorMessage, type ImportReport, type Product, type ProductInput } from "../lib/api";
import { centsToInput, formatCents, parseCents } from "../lib/money";

interface PresRow {
  id?: string;
  name: string;
  factor: string;
  price: string;
}

function ProductForm({ product, onClose }: { product: Product | null; onClose: () => void }) {
  const qc = useQueryClient();
  const categories = useQuery({ queryKey: ["categories"], queryFn: catalogApi.categories });
  const [name, setName] = useState(product?.name ?? "");
  const [sku, setSku] = useState(product?.sku ?? "");
  const [barcode, setBarcode] = useState(product?.barcode ?? "");
  const [categoryId, setCategoryId] = useState(product?.category_id ?? "");
  const [newCategory, setNewCategory] = useState("");
  const [unit, setUnit] = useState(product?.base_unit ?? "unidad");
  const [cost, setCost] = useState(centsToInput(product?.reference_cost_cents ?? 0));
  const [minStock, setMinStock] = useState(product ? String(product.min_stock) : "");
  const [rows, setRows] = useState<PresRow[]>(
    product
      ? product.presentations
          .filter((p) => p.active)
          .map((p) => ({ id: p.id, name: p.name, factor: String(p.factor_to_base), price: centsToInput(p.sale_price_cents) }))
      : [{ name: "Unidad", factor: "1", price: "0.00" }],
  );
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: async () => {
      const costCents = parseCents(cost);
      if (costCents === null) throw { message: "Costo inválido" };
      const presentations = rows.map((r) => {
        const price = parseCents(r.price);
        const factor = Number(r.factor);
        if (price === null) throw { message: `Precio inválido en «${r.name}»` };
        if (!Number.isInteger(factor) || factor <= 0) throw { message: `Factor inválido en «${r.name}»` };
        return { id: r.id ?? null, name: r.name, factor_to_base: factor, sale_price_cents: price };
      });
      let catId: string | null = categoryId || null;
      if (newCategory.trim()) catId = await catalogApi.saveCategory(newCategory.trim());
      const input: ProductInput = {
        id: product?.id ?? null,
        category_id: catId,
        sku: sku.trim() || null,
        barcode: barcode.trim() || null,
        name,
        base_unit: unit,
        reference_cost_cents: costCents,
        min_stock: minStock.trim() === "" ? null : Number(minStock),
        presentations,
      };
      return catalogApi.saveProduct(input);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["categories"] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  const setRow = (i: number, patch: Partial<PresRow>) =>
    setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  return (
    <Modal title={product ? "Editar producto" : "Nuevo producto"} onClose={onClose} wide>
      <div className="grid grid-cols-2 gap-4">
        <div className="col-span-2">
          <label className="label">Nombre *</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">SKU (vacío = automático)</label>
          <input className="input" value={sku} onChange={(e) => setSku(e.target.value)} />
        </div>
        <div>
          <label className="label">Código de barras</label>
          <input className="input" value={barcode} onChange={(e) => setBarcode(e.target.value)} />
        </div>
        <div>
          <label className="label">Categoría</label>
          <select className="input" value={categoryId} onChange={(e) => setCategoryId(e.target.value)}>
            <option value="">Sin categoría</option>
            {categories.data?.filter((c) => c.active).map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="label">…o crear categoría nueva</label>
          <input className="input" value={newCategory} onChange={(e) => setNewCategory(e.target.value)} />
        </div>
        <div>
          <label className="label">Unidad base</label>
          <input className="input" value={unit} onChange={(e) => setUnit(e.target.value)} />
        </div>
        <div>
          <label className="label">Costo de referencia (S/)</label>
          <input className="input" inputMode="decimal" value={cost} onChange={(e) => setCost(e.target.value)} />
        </div>
        <div>
          <label className="label">Stock mínimo (vacío = predeterminado)</label>
          <input className="input" type="number" min={0} value={minStock} onChange={(e) => setMinStock(e.target.value)} />
        </div>
      </div>

      <h3 className="mb-2 mt-6 font-semibold text-brand">Presentaciones</h3>
      <div className="space-y-2">
        {rows.map((r, i) => (
          <div key={i} className="grid grid-cols-[1fr_7rem_8rem_auto] gap-2">
            <input className="input" placeholder="Nombre (Caja x24)" value={r.name} onChange={(e) => setRow(i, { name: e.target.value })} />
            <input className="input" type="number" min={1} title="Unidades base que contiene" value={r.factor} onChange={(e) => setRow(i, { factor: e.target.value })} />
            <input className="input" inputMode="decimal" title="Precio de venta (S/)" value={r.price} onChange={(e) => setRow(i, { price: e.target.value })} />
            <button className="btn-ghost min-h-12 px-3" disabled={rows.length === 1} onClick={() => setRows(rows.filter((_, j) => j !== i))} aria-label="Quitar">
              ✕
            </button>
          </div>
        ))}
      </div>
      <p className="mt-1 text-xs text-slate-500">Columnas: nombre · factor (unidades base) · precio de venta S/</p>
      <button className="btn-ghost mt-2 min-h-10" onClick={() => setRows([...rows, { name: "", factor: "1", price: "0.00" }])}>
        + Agregar presentación
      </button>

      {error && <p className="mt-4 text-red-700">{error}</p>}
      <div className="mt-6 flex justify-end gap-3">
        <button className="btn-ghost" onClick={onClose}>
          Cancelar
        </button>
        <button className="btn-accent" disabled={save.isPending} onClick={() => save.mutate()}>
          Guardar
        </button>
      </div>
    </Modal>
  );
}

function ImportDialog({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const [content, setContent] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useMutation({
    mutationFn: (dry: boolean) => catalogApi.importCsv(content!, dry),
    onSuccess: (r) => {
      setReport(r);
      if (!r.dry_run) qc.invalidateQueries({ queryKey: ["products"] });
    },
    onError: (e) => setError(errorMessage(e)),
  });

  async function onFile(f: File | undefined) {
    if (!f) return;
    setReport(null);
    setError(null);
    const text = await f.text();
    setContent(text);
    try {
      setReport(await catalogApi.importCsv(text, true));
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <Modal title="Importar productos (CSV)" onClose={onClose} wide>
      <p className="text-sm text-slate-600">
        Columnas: <code>nombre</code> (obligatoria), <code>sku</code>, <code>codigo_barras</code>, <code>categoria</code>,{" "}
        <code>unidad</code>, <code>costo</code>, <code>precio</code>, <code>stock_minimo</code>, <code>stock_inicial</code>.
        Separador coma o punto y coma.
      </p>
      <input className="input mt-4 py-2" type="file" accept=".csv,text/csv" onChange={(e) => onFile(e.target.files?.[0])} />
      {error && <p className="mt-3 text-red-700">{error}</p>}
      {report && (
        <div className="mt-4">
          <p className="font-medium">
            {report.dry_run ? "Vista previa: " : "Importado: "}
            {report.imported} correctas, {report.rows.length - report.imported} con errores
          </p>
          <div className="mt-2 max-h-60 overflow-y-auto rounded-lg border border-slate-200">
            <table className="w-full text-sm">
              <tbody>
                {report.rows.map((r) => (
                  <tr key={r.line} className={r.ok ? "" : "bg-red-50 text-red-800"}>
                    <td className="px-3 py-1">Fila {r.line}</td>
                    <td className="px-3 py-1">{r.ok ? "OK" : r.message}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
      <div className="mt-6 flex justify-end gap-3">
        <button className="btn-ghost" onClick={onClose}>
          {report && !report.dry_run ? "Cerrar" : "Cancelar"}
        </button>
        {report?.dry_run && report.imported > 0 && (
          <button className="btn-accent" disabled={run.isPending} onClick={() => run.mutate(false)}>
            Importar {report.imported} productos
          </button>
        )}
      </div>
    </Modal>
  );
}

export default function Products() {
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [inactive, setInactive] = useState(false);
  const [editing, setEditing] = useState<Product | "new" | null>(null);
  const [importing, setImporting] = useState(false);

  const categories = useQuery({ queryKey: ["categories"], queryFn: catalogApi.categories });
  const products = useQuery({
    queryKey: ["products", search, categoryId, inactive],
    queryFn: () => catalogApi.products({ search, category_id: categoryId || null, include_inactive: inactive }),
  });
  const toggle = useMutation({
    mutationFn: (p: Product) => catalogApi.setProductActive(p.id, !p.active),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["products"] }),
  });

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-2xl font-bold text-brand">Productos</h1>
        <div className="flex gap-2">
          <button className="btn-ghost" onClick={() => setImporting(true)}>
            Importar CSV
          </button>
          <button className="btn-accent" onClick={() => setEditing("new")}>
            + Nuevo producto
          </button>
        </div>
      </div>

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <input className="input max-w-sm" placeholder="Buscar por nombre, SKU o código…" value={search} onChange={(e) => setSearch(e.target.value)} />
        <select className="input max-w-52" value={categoryId} onChange={(e) => setCategoryId(e.target.value)}>
          <option value="">Todas las categorías</option>
          {categories.data?.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={inactive} onChange={(e) => setInactive(e.target.checked)} /> Mostrar inactivos
        </label>
      </div>

      {products.isError && <p className="text-red-700">{errorMessage(products.error)}</p>}
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">SKU</th>
              <th className="px-4 py-3">Producto</th>
              <th className="px-4 py-3">Categoría</th>
              <th className="px-4 py-3 text-right">Stock</th>
              <th className="px-4 py-3 text-right">Precio</th>
              <th className="px-4 py-3" />
            </tr>
          </thead>
          <tbody>
            {products.data?.map((p) => {
              const base = p.presentations.find((x) => x.active && x.factor_to_base === 1) ?? p.presentations.find((x) => x.active);
              return (
                <tr key={p.id} className={`border-t border-slate-100 ${p.active ? "" : "text-slate-400"}`}>
                  <td className="px-4 py-3 font-mono text-sm">{p.sku}</td>
                  <td className="px-4 py-3 font-medium">{p.name}</td>
                  <td className="px-4 py-3">{p.category_name ?? "—"}</td>
                  <td className={`px-4 py-3 text-right font-semibold ${p.stock <= p.min_stock ? "text-accent-dark" : ""}`}>
                    {p.stock} {p.base_unit}
                  </td>
                  <td className="px-4 py-3 text-right">{base ? formatCents(base.sale_price_cents) : "—"}</td>
                  <td className="space-x-2 px-4 py-3 text-right">
                    <button className="btn-ghost min-h-10 px-3" onClick={() => setEditing(p)}>
                      Editar
                    </button>
                    <button className="btn-ghost min-h-10 px-3" onClick={() => toggle.mutate(p)}>
                      {p.active ? "Desactivar" : "Activar"}
                    </button>
                  </td>
                </tr>
              );
            })}
            {products.data?.length === 0 && (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-center text-slate-500">
                  Sin productos. Crea uno o importa un CSV.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {editing && <ProductForm product={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      {importing && <ImportDialog onClose={() => setImporting(false)} />}
    </div>
  );
}
