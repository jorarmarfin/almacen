import { NavLink, Outlet } from "react-router-dom";
import logo from "../assets/logo.png";
import type { Engine } from "../lib/api";

const links = [
  ["/", "Inicio"],
  ["/vender", "Vender"],
  ["/productos", "Productos"],
  ["/categorias", "Categorías"],
  ["/inventario", "Inventario"],
  ["/compras", "Compras"],
  ["/caja", "Caja"],
  ["/reportes", "Reportes"],
  ["/configuracion", "Configuración"],
] as const;

export default function Layout({ businessName, engine }: { businessName: string; engine: Engine }) {
  return (
    <div className="flex min-h-screen">
      <aside className="flex w-60 flex-col gap-1 border-r border-slate-200 bg-white p-4">
        <img src={logo} alt="Mi Bodega" className="mx-auto mb-3 w-36" />
        <div className="mb-4 border-b border-slate-200 pb-3 text-center">
          <div className="font-bold text-brand">{businessName}</div>
          <div className="text-xs uppercase text-slate-500">BD: {engine}</div>
        </div>
        {links.map(([to, label]) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) =>
              `rounded-lg px-4 py-3 text-base font-medium ${isActive ? "bg-brand text-white shadow-[inset_4px_0_0_var(--color-accent)]" : "text-brand hover:bg-brand-soft"}`
            }
          >
            {label}
          </NavLink>
        ))}
      </aside>
      <main className="flex-1 p-8">
        <Outlet />
      </main>
    </div>
  );
}
