import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Modal from "../components/Modal";
import { catalogApi, errorMessage, type Category } from "../lib/api";

function CategoryForm({ category, onClose }: { category: Category | null; onClose: () => void }) {
  const qc = useQueryClient();
  const [name, setName] = useState(category?.name ?? "");
  const [active, setActive] = useState(category?.active ?? true);
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () => catalogApi.saveCategory(name, category?.id ?? null, active),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["categories"] });
      qc.invalidateQueries({ queryKey: ["products"] });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <Modal title={category ? "Editar categoría" : "Nueva categoría"} onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <div>
          <label className="label">Nombre *</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </div>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={active} onChange={(e) => setActive(e.target.checked)} /> Activa
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

export default function Categories() {
  const qc = useQueryClient();
  const [editing, setEditing] = useState<Category | "new" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const categories = useQuery({ queryKey: ["categories"], queryFn: catalogApi.categories });

  const refresh = () => qc.invalidateQueries({ queryKey: ["categories"] });
  const toggle = useMutation({
    mutationFn: (c: Category) => catalogApi.saveCategory(c.name, c.id, !c.active),
    onSuccess: refresh,
    onError: (e) => setError(errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: (c: Category) => catalogApi.deleteCategory(c.id),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-2xl font-bold text-brand">Categorías</h1>
        <button className="btn-accent" onClick={() => setEditing("new")}>
          + Nueva categoría
        </button>
      </div>
      {error && <p className="mb-3 rounded-lg bg-red-50 p-3 text-red-800">{error}</p>}
      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-left">
          <thead className="bg-brand-soft text-sm text-brand">
            <tr>
              <th className="px-4 py-3">Nombre</th>
              <th className="px-4 py-3 text-right">Productos</th>
              <th className="px-4 py-3">Estado</th>
              <th className="px-4 py-3" />
            </tr>
          </thead>
          <tbody>
            {categories.data?.map((c) => (
              <tr key={c.id} className={`border-t border-slate-100 ${c.active ? "" : "text-slate-400"}`}>
                <td className="px-4 py-3 font-medium">{c.name}</td>
                <td className="px-4 py-3 text-right">{c.product_count}</td>
                <td className="px-4 py-3">{c.active ? "Activa" : "Inactiva"}</td>
                <td className="space-x-2 px-4 py-3 text-right">
                  <button className="btn-ghost min-h-10 px-3" onClick={() => setEditing(c)}>
                    Editar
                  </button>
                  <button className="btn-ghost min-h-10 px-3" onClick={() => toggle.mutate(c)}>
                    {c.active ? "Desactivar" : "Activar"}
                  </button>
                  <button
                    className="btn-ghost min-h-10 px-3 text-red-700 disabled:text-slate-400"
                    disabled={c.product_count > 0}
                    title={c.product_count > 0 ? "Tiene productos: desactívala" : "Eliminar"}
                    onClick={() => {
                      if (window.confirm(`¿Eliminar la categoría «${c.name}»?`)) remove.mutate(c);
                    }}
                  >
                    Eliminar
                  </button>
                </td>
              </tr>
            ))}
            {categories.data?.length === 0 && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-slate-500">
                  Aún no hay categorías.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {editing && <CategoryForm category={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
