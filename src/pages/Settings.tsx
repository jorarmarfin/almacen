import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import Modal from "../components/Modal";
import { backupApi, errorCode, errorMessage, type BackupManifest } from "../lib/api";
import { formatDateTime } from "../lib/money";

const EXT = "mibodega-backup";

function stamp() {
  const n = new Date();
  const p = (v: number) => String(v).padStart(2, "0");
  return `${n.getFullYear()}${p(n.getMonth() + 1)}${p(n.getDate())}-${p(n.getHours())}${p(n.getMinutes())}`;
}

function RestoreDialog({ path, manifest, password, onClose }: { path: string; manifest: BackupManifest; password: string | null; onClose: () => void }) {
  const qc = useQueryClient();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const info = useQuery({ queryKey: ["app-info"], queryFn: backupApi.info });

  async function run() {
    setBusy(true);
    setError(null);
    try {
      const rep = await backupApi.restore(path, password);
      setDone(rep.pre_restore_backup ?? "(no fue posible crear el respaldo previo)");
      await qc.invalidateQueries();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (done) {
    return (
      <Modal title="Restauración completada" onClose={onClose}>
        <p className="text-emerald-700">Los datos fueron reemplazados por los del respaldo.</p>
        <p className="mt-3 text-sm text-slate-600">
          Se guardó un respaldo de seguridad de los datos anteriores en:
          <br />
          <code className="break-all">{done}</code>
        </p>
        <div className="mt-6 flex justify-end">
          <button className="btn-accent" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </Modal>
    );
  }

  const rows = manifest.tables;
  const differentEngine = info.data && manifest.engine !== info.data.engine;
  return (
    <Modal title="Restaurar respaldo" onClose={onClose}>
      <div className="space-y-4">
        <div className="rounded-lg bg-brand-soft p-4 text-sm">
          <div>
            <b>Negocio:</b> {manifest.business_name ?? "—"}
          </div>
          <div>
            <b>Fecha del respaldo:</b> {formatDateTime(manifest.created_at)}
          </div>
          <div>
            <b>Origen:</b> {manifest.engine} · versión {manifest.app_version}
          </div>
          <div>
            <b>Contenido:</b> {rows.products?.rows ?? 0} productos, {rows.sales?.rows ?? 0} ventas, {rows.purchases?.rows ?? 0} compras,{" "}
            {rows.cash_sessions?.rows ?? 0} cajas
          </div>
          <div className="mt-1 text-emerald-700">✓ Archivo íntegro: todas las sumas de verificación coinciden.</div>
        </div>
        <div className="rounded-lg border border-accent bg-accent/5 p-4 text-sm">
          <b>Atención:</b> se reemplazarán <b>todos</b> los datos actuales (productos, ventas, caja, etc.) por los del respaldo. Antes se guardará
          automáticamente un respaldo de los datos actuales para poder deshacer.
          {differentEngine && (
            <div className="mt-2">
              El respaldo viene de <b>{manifest.engine}</b> y se cargará en <b>{info.data?.engine}</b>. Es válido: así se migra entre motores.
            </div>
          )}
        </div>
        <div>
          <label className="label">Escribe RESTAURAR para confirmar</label>
          <input className="input" value={typed} onChange={(e) => setTyped(e.target.value)} />
        </div>
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-accent" disabled={typed !== "RESTAURAR" || busy} onClick={run}>
            Restaurar ahora
          </button>
        </div>
      </div>
    </Modal>
  );
}

function ExportDialog({ onClose, onConfirm }: { onClose: () => void; onConfirm: (password: string | null) => void }) {
  const [encrypt, setEncrypt] = useState(true);
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const tooShort = encrypt && pw.length > 0 && pw.length < 8;
  const mismatch = encrypt && pw !== pw2;
  const invalid = encrypt && (pw.length < 8 || mismatch);
  return (
    <Modal title="Exportar respaldo" onClose={onClose}>
      <div className="space-y-4">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={encrypt} onChange={(e) => setEncrypt(e.target.checked)} /> Proteger con contraseña (recomendado)
        </label>
        {encrypt && (
          <>
            <div>
              <label className="label">Contraseña (mínimo 8 caracteres)</label>
              <input className="input" type="password" value={pw} onChange={(e) => setPw(e.target.value)} autoFocus />
            </div>
            <div>
              <label className="label">Repite la contraseña</label>
              <input className="input" type="password" value={pw2} onChange={(e) => setPw2(e.target.value)} />
            </div>
            {tooShort && <p className="text-sm text-accent-dark">Usa al menos 8 caracteres.</p>}
            {pw2 && mismatch && <p className="text-sm text-accent-dark">Las contraseñas no coinciden.</p>}
            <p className="rounded-lg bg-amber-50 p-3 text-sm text-amber-900">
              <b>Si olvidas la contraseña, el respaldo no se puede recuperar.</b> Anótala en un lugar seguro.
            </p>
          </>
        )}
        {!encrypt && <p className="text-sm text-slate-600">Sin contraseña, cualquiera con el archivo puede leer tus ventas y costos.</p>}
        <div className="flex justify-end gap-3">
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-accent" disabled={invalid} onClick={() => onConfirm(encrypt ? pw : null)}>
            Elegir destino…
          </button>
        </div>
      </div>
    </Modal>
  );
}

function PasswordPrompt({ error, onClose, onConfirm }: { error: string | null; onClose: () => void; onConfirm: (pw: string) => void }) {
  const [pw, setPw] = useState("");
  return (
    <Modal title="Respaldo protegido" onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (pw) onConfirm(pw);
        }}
      >
        <p className="text-sm text-slate-600">Este respaldo está cifrado. Escribe su contraseña para verlo y restaurarlo.</p>
        <input className="input" type="password" value={pw} onChange={(e) => setPw(e.target.value)} autoFocus />
        {error && <p className="text-red-700">{error}</p>}
        <div className="flex justify-end gap-3">
          <button type="button" className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button type="submit" className="btn-accent" disabled={!pw}>
            Continuar
          </button>
        </div>
      </form>
    </Modal>
  );
}

export default function Settings({ businessName }: { businessName: string }) {
  const qc = useQueryClient();
  const info = useQuery({ queryKey: ["app-info"], queryFn: backupApi.info });
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [restore, setRestore] = useState<{ path: string; manifest: BackupManifest; password: string | null } | null>(null);
  const [exporting, setExporting] = useState(false);
  const [asking, setAsking] = useState<{ path: string; error: string | null } | null>(null);

  async function guard(fn: () => Promise<void>) {
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

  const exportBackup = (password: string | null) =>
    guard(async () => {
      const path = await save({ defaultPath: `mibodega-${stamp()}.${EXT}`, filters: [{ name: "Respaldo Mi Bodega", extensions: [EXT] }] });
      if (!path) return;
      const m = await backupApi.exportBackup(path, password);
      setMsg({
        ok: true,
        text: `Respaldo ${password ? "cifrado " : ""}guardado en ${path} (${m.tables.products?.rows ?? 0} productos, ${m.tables.sales?.rows ?? 0} ventas).`,
      });
      qc.invalidateQueries({ queryKey: ["app-info"] });
    });

  const exportDb = () =>
    guard(async () => {
      const path = await save({ defaultPath: `mibodega-${stamp()}.db`, filters: [{ name: "SQLite", extensions: ["db"] }] });
      if (!path) return;
      await backupApi.exportSqliteNative(path);
      setMsg({ ok: true, text: `Copia SQLite guardada en ${path}` });
    });

  const pickRestore = () =>
    guard(async () => {
      const picked = await open({ multiple: false, filters: [{ name: "Respaldo Mi Bodega", extensions: [EXT] }] });
      if (!picked || Array.isArray(picked)) return;
      try {
        const manifest = await backupApi.inspect(picked); // valida sin tocar nada
        setRestore({ path: picked, manifest, password: null });
      } catch (e) {
        if (errorCode(e) === "PASSWORD_REQUIRED") setAsking({ path: picked, error: null });
        else throw e;
      }
    });

  const submitPassword = (pw: string) =>
    guard(async () => {
      if (!asking) return;
      try {
        const manifest = await backupApi.inspect(asking.path, pw);
        setRestore({ path: asking.path, manifest, password: pw });
        setAsking(null);
      } catch (e) {
        setAsking({ path: asking.path, error: errorMessage(e) });
      }
    });

  const d = info.data;
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-brand">Configuración y respaldo</h1>

      <section className="rounded-xl border border-slate-200 bg-white p-5">
        <h2 className="mb-3 font-bold text-brand">Respaldo de datos</h2>
        <p className="mb-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-900">
          Tus datos viven solo en este equipo. Si el equipo falla o se reinstala sin respaldo, <b>se pierden</b>. Haz un respaldo seguido y guárdalo
          fuera del equipo (USB o nube). El archivo contiene información comercial: protégelo con contraseña.
        </p>
        <p className="mb-4 text-sm text-slate-600">
          Último respaldo: <b>{d?.last_backup_at ? formatDateTime(d.last_backup_at) : "nunca"}</b>
        </p>
        <div className="flex flex-wrap gap-3">
          <button className="btn-accent" disabled={busy} onClick={() => setExporting(true)}>
            ⬇ Exportar respaldo
          </button>
          <button className="btn-ghost" disabled={busy} onClick={pickRestore}>
            ⬆ Restaurar respaldo…
          </button>
          {d?.engine === "sqlite" && (
            <button className="btn-ghost" disabled={busy} onClick={exportDb}>
              Copia SQLite (.db)
            </button>
          )}
        </div>
        {msg && <p className={`mt-4 break-words ${msg.ok ? "text-emerald-700" : "text-red-700"}`}>{msg.text}</p>}
        <p className="mt-4 text-xs text-slate-500">
          El respaldo funciona con SQLite y MySQL y se puede restaurar en cualquiera de los dos motores.
        </p>
      </section>

      <section className="rounded-xl border border-slate-200 bg-white p-5">
        <h2 className="mb-3 font-bold text-brand">Acerca de</h2>
        <dl className="grid grid-cols-[10rem_1fr] gap-y-2 text-sm">
          <dt className="text-slate-500">Negocio</dt>
          <dd>{businessName}</dd>
          <dt className="text-slate-500">Versión</dt>
          <dd>{d?.version ?? "—"}</dd>
          <dt className="text-slate-500">Base de datos</dt>
          <dd>{d?.engine === "mysql" ? "MySQL (servidor)" : "SQLite (local)"}</dd>
          <dt className="text-slate-500">Carpeta de datos</dt>
          <dd className="break-all font-mono text-xs">{d?.data_dir}</dd>
        </dl>
      </section>

      {exporting && (
        <ExportDialog
          onClose={() => setExporting(false)}
          onConfirm={(pw) => {
            setExporting(false);
            exportBackup(pw);
          }}
        />
      )}
      {asking && <PasswordPrompt error={asking.error} onClose={() => setAsking(null)} onConfirm={submitPassword} />}
      {restore && <RestoreDialog path={restore.path} manifest={restore.manifest} password={restore.password} onClose={() => setRestore(null)} />}
    </div>
  );
}
