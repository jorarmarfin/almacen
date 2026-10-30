import { useQuery } from "@tanstack/react-query";
import { Route, Routes } from "react-router-dom";
import { api, errorMessage } from "./lib/api";
import Layout from "./pages/Layout";
import Cash from "./pages/Cash";
import Categories from "./pages/Categories";
import Home from "./pages/Home";
import Inventory from "./pages/Inventory";
import Products from "./pages/Products";
import Purchases from "./pages/Purchases";
import Sell from "./pages/Sell";
import SalesHistory from "./pages/SalesHistory";
import Reports from "./pages/Reports";
import Settings from "./pages/Settings";
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
        <Route index element={<Home businessName={name} />} />
        <Route path="vender" element={<Sell businessName={name} />} />
        <Route path="ventas" element={<SalesHistory businessName={name} />} />
        <Route path="productos" element={<Products />} />
        <Route path="categorias" element={<Categories />} />
        <Route path="inventario" element={<Inventory />} />
        <Route path="compras" element={<Purchases />} />
        <Route path="caja" element={<Cash />} />
        <Route path="reportes" element={<Reports />} />
        <Route path="configuracion" element={<Settings businessName={name} />} />
      </Route>
    </Routes>
  );
}
