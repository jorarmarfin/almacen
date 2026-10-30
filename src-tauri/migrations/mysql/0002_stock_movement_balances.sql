-- Auditoría de ajustes: saldo antes/después de cada movimiento (RF-02)
ALTER TABLE stock_movements ADD COLUMN balance_before BIGINT NULL, ADD COLUMN balance_after BIGINT NULL, ADD COLUMN seq BIGINT NULL;
CREATE INDEX idx_stock_mov_seq ON stock_movements(product_id, seq);
