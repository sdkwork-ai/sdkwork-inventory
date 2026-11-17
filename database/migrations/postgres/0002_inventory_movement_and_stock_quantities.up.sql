-- sdkwork:migration
-- id: 0002_inventory_movement_and_stock_quantities
-- engine: postgres
-- module: sdkwork-inventory
-- purpose: Materialize the append-only stock movement ledger the repository
--   already reads (commerce_inventory_movement) and add the inbound/damaged
--   quantity columns the merchant stock projections select. Both were missing
--   from the consolidated baseline, so listings and adjustments failed on a
--   freshly initialized database. Idempotent: safe to re-run on deployments
--   that already received the columns or table from a manual repair.
-- reversible: false
-- rollback: forward-fix (the ledger is additive; dropping it would destroy
--   stock history)
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_inventory_stock
    ADD COLUMN IF NOT EXISTS inbound_quantity BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS damaged_quantity BIGINT NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS commerce_inventory_movement (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    movement_no TEXT NOT NULL,
    sku_id TEXT NOT NULL,
    warehouse_id TEXT,
    fulfillment_node_id TEXT,
    movement_type TEXT NOT NULL,
    source_type TEXT NOT NULL,
    quantity BIGINT NOT NULL DEFAULT 0,
    direction TEXT NOT NULL,
    quantity_before BIGINT NOT NULL DEFAULT 0,
    quantity_after BIGINT NOT NULL DEFAULT 0,
    business_type TEXT,
    source_id TEXT,
    reason TEXT,
    request_no TEXT,
    idempotency_key TEXT,
    occurred_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_commerce_inventory_movement_tenant_sku
    ON commerce_inventory_movement (tenant_id, sku_id, occurred_at);

CREATE INDEX IF NOT EXISTS idx_commerce_inventory_movement_tenant_warehouse
    ON commerce_inventory_movement (tenant_id, warehouse_id, occurred_at);

COMMIT;
