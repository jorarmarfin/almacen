import { NavLink, Outlet } from "react-router-dom";
import type { Engine } from "../lib/api";

const links = [
  ["/", "Inicio"],
  ["/vender", "Vender"],
  ["/productos", "Productos"],
  ["/inventario", "Inventario"],
  ["/compras", "Compras"],
  ["/caja", "Caja"],
  ["/reportes", "Reportes"],
  ["/configuracion", "Configuración"],
] as const;

export default function Layout({ businessName, engine }: { businessName: string; engine: Engine }) {
  return (
    <div className="flex min-h-screen">
      <aside className="flex w-56 flex-col gap-1 bg-slate-900 p-4 text-white">
        <div className="mb-4">
          <div className="text-lg font-bold">{businessName}</div>
          <div className="text-xs uppercase text-slate-400">BD: {engine}</div>
        </div>
        {links.map(([to, label]) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) =>
              `rounded-lg px-4 py-3 text-base font-medium ${isActive ? "bg-brand" : "hover:bg-slate-800"}`
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
