import type { PayMethod, SaleDetail } from "../lib/api";
import { formatCents, formatDateTime } from "../lib/money";

const METHOD: Record<PayMethod, string> = { EFECTIVO: "Efectivo", YAPE: "Yape", PLIN: "Plin", OTRO: "Otro" };

/** Comprobante interno NO tributario (no es boleta ni factura electrónica). */
export default function Ticket({ sale, businessName }: { sale: SaleDetail; businessName: string }) {
  return (
    <div className="ticket mx-auto w-full max-w-xs rounded-lg border border-dashed border-slate-300 bg-white p-4 font-mono text-xs text-black">
      <div className="text-center">
        <div className="text-sm font-bold">{businessName}</div>
        <div>COMPROBANTE INTERNO</div>
        <div>(no válido como boleta o factura)</div>
        <div className="mt-1">
          {sale.number} · {formatDateTime(sale.sold_at)}
        </div>
        {sale.status === "CANCELED" && <div className="mt-1 font-bold">*** ANULADA ***</div>}
      </div>
      <hr className="my-2 border-dashed border-slate-400" />
      {sale.items.map((it, i) => (
        <div key={i} className="mb-1">
          <div>{it.product_name}</div>
          <div className="flex justify-between">
            <span>
              {it.qty_presentations} × {it.presentation_name} @ {formatCents(it.unit_price_cents)}
            </span>
            <span>{formatCents(it.qty_presentations * it.unit_price_cents)}</span>
          </div>
          {it.discount_cents > 0 && (
            <div className="flex justify-between">
              <span>  Desc.</span>
              <span>-{formatCents(it.discount_cents)}</span>
            </div>
          )}
        </div>
      ))}
      <hr className="my-2 border-dashed border-slate-400" />
      <div className="flex justify-between">
        <span>Subtotal</span>
        <span>{formatCents(sale.subtotal_cents)}</span>
      </div>
      {sale.discount_cents > 0 && (
        <div className="flex justify-between">
          <span>Descuento</span>
          <span>-{formatCents(sale.discount_cents)}</span>
        </div>
      )}
      <div className="flex justify-between text-sm font-bold">
        <span>TOTAL</span>
        <span>{formatCents(sale.total_cents)}</span>
      </div>
      <hr className="my-2 border-dashed border-slate-400" />
      {sale.payments.map((p, i) => (
        <div key={i}>
          <div className="flex justify-between">
            <span>{METHOD[p.method]}</span>
            <span>{formatCents(p.amount_cents)}</span>
          </div>
          {p.method === "EFECTIVO" && p.received_cents !== null && p.received_cents !== p.amount_cents && (
            <>
              <div className="flex justify-between">
                <span>  Recibido</span>
                <span>{formatCents(p.received_cents)}</span>
              </div>
              <div className="flex justify-between">
                <span>  Vuelto</span>
                <span>{formatCents(p.change_cents ?? 0)}</span>
              </div>
            </>
          )}
        </div>
      ))}
      <div className="mt-3 text-center">¡Gracias por su compra!</div>
    </div>
  );
}
