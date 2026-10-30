import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { errorMessage, salesApi } from "../lib/api";
import { parseCents } from "../lib/money";

export default function OpenCashCard() {
  const qc = useQueryClient();
  const [amount, setAmount] = useState("0.00");
  const [error, setError] = useState<string | null>(null);
  const open = useMutation({
    mutationFn: async () => {
      const cents = parseCents(amount);
      if (cents === null) throw { message: "Monto inválido" };
      return salesApi.openCash(cents);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["cash"] });
      qc.invalidateQueries({ queryKey: ["cash-sessions"] });
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <div className="mx-auto mt-16 max-w-md rounded-2xl bg-white p-8 shadow">
      <h1 className="text-2xl font-bold text-brand">Abre la caja para vender</h1>
      <p className="mt-1 text-slate-600">Indica el efectivo con el que inicias el turno.</p>
      <label className="label mt-6">Saldo inicial (S/)</label>
      <input className="input" inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus />
      {error && <p className="mt-3 text-red-700">{error}</p>}
      <button className="btn-accent mt-6 w-full" disabled={open.isPending} onClick={() => open.mutate()}>
        Abrir caja
      </button>
    </div>
  );
}
