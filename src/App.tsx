import { useQuery } from "@tanstack/react-query";
import { Route, Routes } from "react-router-dom";
import { api, errorMessage } from "./lib/api";
import Layout from "./pages/Layout";
import Categories from "./pages/Categories";
import Inventory from "./pages/Inventory";
import Placeholder from "./pages/Placeholder";
import Products from "./pages/Products";
import Purchases from "./pages/Purchases";
import Setup from "./pages/Setup";

export default function App() {
  const boot = useQuery({ queryKey: ["boot"], queryFn: api.bootState });

  if (boot.isPending) return <p className="p-8">Cargando…</p>;
  if (boot.isError) return <p className="p-8 text-red-700">{errorMessage(boot.error)}</p>;
  if (!boot.data.db_ready || !boot.data.initialized) return <Setup boot={boot.data} />;

  const name = boot.data.settings!.business_name;
  return (
    <Routes>
      <Route element={<Layout businessName={name} engine={boot.data.engine} />}>
        <Route index element={<Placeholder title="Inicio" />} />
        <Route path="vender" element={<Placeholder title="Vender" />} />
        <Route path="productos" element={<Products />} />
        <Route path="categorias" element={<Categories />} />
        <Route path="inventario" element={<Inventory />} />
        <Route path="compras" element={<Purchases />} />
        <Route path="caja" element={<Placeholder title="Caja" />} />
        <Route path="reportes" element={<Placeholder title="Reportes" />} />
        <Route path="configuracion" element={<Placeholder title="Configuración / Respaldo" />} />
      </Route>
    </Routes>
  );
}
