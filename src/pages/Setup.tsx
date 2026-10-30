import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, errorMessage, type BootState, type DbConfig, type Engine } from "../lib/api";

const emptyMysql = { host: "127.0.0.1", port: 3306, user: "root", password: "", database: "mibodega" };

export default function Setup({ boot }: { boot: BootState }) {
  const qc = useQueryClient();
  const [engine, setEngine] = useState<Engine>(boot.engine);
  const [mysql, setMysql] = useState(emptyMysql);
  const [name, setName] = useState("");
  const [minStock, setMinStock] = useState(5);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  const config = (): DbConfig => ({ engine, mysql: engine === "mysql" ? mysql : null });

  async function run(fn: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setMsg(null);
    try {
      await fn();
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  }

  const test = () =>
    run(async () => {
      await api.testDbConnection(config());
      setMsg({ ok: true, text: "Conexión correcta" });
    });

  const start = () =>
    run(async () => {
      if (!name.trim()) throw { message: "Ingresa el nombre del negocio" };
      if (!boot.db_ready || boot.engine !== engine || engine === "mysql") {
        await api.setDbConfig(config());
      }
      const current = await api.bootState();
      if (!current.initialized) {
        await api.initBusiness({ business_name: name, default_min_stock: minStock });
      }
      await qc.invalidateQueries({ queryKey: ["boot"] });
    });

  return (
    <div className="mx-auto mt-12 max-w-xl rounded-2xl bg-white p-8 shadow">
      <h1 className="text-2xl font-bold">Bienvenido a Mi Bodega</h1>
      <p className="mt-1 text-slate-600">Configura tu negocio para empezar a vender.</p>

      <div className="mt-6 space-y-4">
        <div>
          <label className="label">Nombre del negocio</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">Stock mínimo predeterminado</label>
          <input
            className="input"
            type="number"
            min={0}
            value={minStock}
            onChange={(e) => setMinStock(Math.max(0, Number(e.target.value)))}
          />
        </div>
        <div>
          <label className="label">Base de datos</label>
          <select className="input" value={engine} onChange={(e) => setEngine(e.target.value as Engine)}>
            <option value="sqlite">SQLite (local, recomendado)</option>
            <option value="mysql">MySQL (servidor)</option>
          </select>
        </div>

        {engine === "mysql" && (
          <div className="grid grid-cols-2 gap-3 rounded-lg bg-slate-50 p-4">
            {(["host", "user", "password", "database"] as const).map((k) => (
              <div key={k}>
                <label className="label capitalize">{k}</label>
                <input
                  className="input"
                  type={k === "password" ? "password" : "text"}
                  value={mysql[k]}
                  onChange={(e) => setMysql({ ...mysql, [k]: e.target.value })}
                />
              </div>
            ))}
            <div>
              <label className="label">Puerto</label>
              <input
                className="input"
                type="number"
                value={mysql.port}
                onChange={(e) => setMysql({ ...mysql, port: Number(e.target.value) })}
              />
            </div>
          </div>
        )}

        {msg && <p className={msg.ok ? "text-green-700" : "text-red-700"}>{msg.text}</p>}

        <div className="flex gap-3">
          <button className="btn-ghost" onClick={test} disabled={busy}>
            Probar conexión
          </button>
          <button className="btn-primary flex-1" onClick={start} disabled={busy}>
            Comenzar
          </button>
        </div>
      </div>
    </div>
  );
}
