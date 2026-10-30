-- Auditoría de ajustes: saldo antes/después de cada movimiento (RF-02)
ALTER TABLE stock_movements ADD COLUMN balance_before INTEGER;
ALTER TABLE stock_movements ADD COLUMN balance_after INTEGER;
-- Orden estable del kardex aunque dos movimientos compartan milisegundo
ALTER TABLE stock_movements ADD COLUMN seq INTEGER;
CREATE INDEX idx_stock_mov_seq ON stock_movements(product_id, seq);
