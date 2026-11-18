-- ============================================================================
-- Inventory demo stock seed (tenant 100001)
-- ============================================================================
-- The shop module seeds the default catalog with 1200 generated products and
-- 2400 generated SKUs (160001..162400), plus the 003 retail SKUs (1630..1642).
-- Without stock rows the Product Center inventory views would be empty, so this
-- seed materializes one `wh-main` stock row per physical SKU, a small movement
-- ledger, and two reservations for the demo orders.
--
-- Idempotent: every row has a stable `demo-*` id and upserts on the primary key.
-- Quantities are deterministic, not random, so every environment starts with the
-- same demo stock. Real deployments replace these rows through stock
-- adjustments; re-running the seed only runs once (seed history by checksum).
-- ============================================================================

-- ---------------------------------------------------------------------------
-- Stock: generated catalog SKUs 160001..162400
-- ---------------------------------------------------------------------------
INSERT INTO commerce_inventory_stock
    (id, tenant_id, organization_id, shop_id, sku_id, warehouse_id, fulfillment_node_id,
     on_hand_quantity, available_quantity, locked_quantity, reserved_quantity, sold_quantity,
     safety_stock_quantity, version, status, created_at, updated_at)
SELECT 'demo-stock-' || (160000 + n),
       '100001',
       '0',
       NULL,
       (160000 + n)::text,
       'wh-main',
       NULL,
       CASE WHEN n % 37 = 0 THEN 5 ELSE 200 + ((n * 37) % 300) END,
       CASE WHEN n % 37 = 0 THEN 5 ELSE 200 + ((n * 37) % 300) END,
       0,
       0,
       0,
       CASE WHEN n % 37 = 0 THEN 50 ELSE 10 END,
       0,
       'active',
       NOW()::text,
       NOW()::text
FROM generate_series(1, 2400) AS n
ON CONFLICT (id) DO UPDATE SET
    on_hand_quantity = EXCLUDED.on_hand_quantity,
    available_quantity = EXCLUDED.available_quantity,
    safety_stock_quantity = EXCLUDED.safety_stock_quantity,
    status = EXCLUDED.status,
    updated_at = NOW()::text;

-- ---------------------------------------------------------------------------
-- Stock: the retail SKUs introduced by the 003 catalog seed
-- ---------------------------------------------------------------------------
INSERT INTO commerce_inventory_stock
    (id, tenant_id, organization_id, shop_id, sku_id, warehouse_id, fulfillment_node_id,
     on_hand_quantity, available_quantity, locked_quantity, reserved_quantity, sold_quantity,
     safety_stock_quantity, version, status, created_at, updated_at)
VALUES
    ('demo-stock-1630', '100001', '0', NULL, '1630', 'wh-main', NULL, 120, 120, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1631', '100001', '0', NULL, '1631', 'wh-main', NULL, 80,  80,  0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1632', '100001', '0', NULL, '1632', 'wh-main', NULL, 150, 150, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1633', '100001', '0', NULL, '1633', 'wh-main', NULL, 90,  90,  0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1634', '100001', '0', NULL, '1634', 'wh-main', NULL, 60,  60,  0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1635', '100001', '0', NULL, '1635', 'wh-main', NULL, 55,  55,  0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1636', '100001', '0', NULL, '1636', 'wh-main', NULL, 200, 200, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1637', '100001', '0', NULL, '1637', 'wh-main', NULL, 300, 300, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1638', '100001', '0', NULL, '1638', 'wh-main', NULL, 400, 400, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1640', '100001', '0', NULL, '1640', 'wh-main', NULL, 180, 180, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1641', '100001', '0', NULL, '1641', 'wh-main', NULL, 170, 170, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1642', '100001', '0', NULL, '1642', 'wh-main', NULL, 40,  40,  0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text),
    ('demo-stock-1643', '100001', '0', NULL, '1643', 'wh-main', NULL, 999, 999, 0, 0, 0, 10, 0, 'active', NOW()::text, NOW()::text)
ON CONFLICT (id) DO UPDATE SET
    on_hand_quantity = EXCLUDED.on_hand_quantity,
    available_quantity = EXCLUDED.available_quantity,
    safety_stock_quantity = EXCLUDED.safety_stock_quantity,
    status = EXCLUDED.status,
    updated_at = NOW()::text;

-- ---------------------------------------------------------------------------
-- Movement ledger: a couple of inbound and outbound rows behind the demo stock
-- ---------------------------------------------------------------------------
INSERT INTO commerce_inventory_movement
    (id, tenant_id, organization_id, movement_no, sku_id, warehouse_id, fulfillment_node_id,
     movement_type, source_type, quantity, direction, quantity_before, quantity_after,
     business_type, source_id, reason, request_no, idempotency_key, occurred_at, created_at)
VALUES
    ('demo-movement-1', '100001', '0', 'MV-DEMO-1', '160001', 'wh-main', NULL,
     'inbound', 'adjustment', 100, 'in', 100, 200,
     'purchase_receipt', 'demo-po-1', '新品入库', 'demo-movement-1', 'demo-movement-1',
     NOW()::text, NOW()::text),
    ('demo-movement-2', '100001', '0', 'MV-DEMO-2', '160001', 'wh-main', NULL,
     'outbound', 'adjustment', 20, 'out', 200, 180,
     'sales_shipment', 'demo-order-1', '订单发货', 'demo-movement-2', 'demo-movement-2',
     NOW()::text, NOW()::text),
    ('demo-movement-3', '100001', '0', 'MV-DEMO-3', '1637', 'wh-main', NULL,
     'inbound', 'adjustment', 300, 'in', 0, 300,
     'purchase_receipt', 'demo-po-2', '首批到货', 'demo-movement-3', 'demo-movement-3',
     NOW()::text, NOW()::text),
    ('demo-movement-4', '100001', '0', 'MV-DEMO-4', '1637', 'wh-main', NULL,
     'stocktake', 'adjustment', 0, 'out', 300, 300,
     'stocktake', 'demo-stocktake-1', '月度盘点', 'demo-movement-4', 'demo-movement-4',
     NOW()::text, NOW()::text)
ON CONFLICT (id) DO UPDATE SET
    movement_type = EXCLUDED.movement_type,
    source_type = EXCLUDED.source_type,
    quantity = EXCLUDED.quantity,
    direction = EXCLUDED.direction,
    quantity_before = EXCLUDED.quantity_before,
    quantity_after = EXCLUDED.quantity_after,
    business_type = EXCLUDED.business_type,
    source_id = EXCLUDED.source_id,
    reason = EXCLUDED.reason,
    occurred_at = EXCLUDED.occurred_at;

-- ---------------------------------------------------------------------------
-- Reservations for the demo orders
-- ---------------------------------------------------------------------------
INSERT INTO commerce_inventory_reservation
    (id, tenant_id, organization_id, reservation_no, order_id, reservation_source_type,
     reservation_source_id, reservation_type, sku_id, warehouse_id, fulfillment_node_id,
     quantity, reserved_quantity, consumed_quantity, released_quantity, status,
     release_reason_code, request_no, idempotency_key, expires_at, consumed_at, released_at,
     created_at, updated_at)
VALUES
    ('demo-reservation-1', '100001', '0', 'RES-DEMO-1', 'demo-order-1', 'order',
     'demo-order-1', 'physical_shipment', '160001', 'wh-main', NULL,
     2, 2, 0, 0, 'reserved', NULL, 'demo-reservation-1', 'demo-reservation-1',
     (NOW() + INTERVAL '7 days')::text, NULL, NULL, NOW()::text, NOW()::text),
    ('demo-reservation-2', '100001', '0', 'RES-DEMO-2', 'demo-order-2', 'order',
     'demo-order-2', 'physical_shipment', '1637', 'wh-main', NULL,
     1, 0, 1, 0, 'consumed', NULL, 'demo-reservation-2', 'demo-reservation-2',
     (NOW() + INTERVAL '7 days')::text, NOW()::text, NULL, NOW()::text, NOW()::text)
ON CONFLICT (id) DO UPDATE SET
    quantity = EXCLUDED.quantity,
    reserved_quantity = EXCLUDED.reserved_quantity,
    consumed_quantity = EXCLUDED.consumed_quantity,
    status = EXCLUDED.status,
    expires_at = EXCLUDED.expires_at,
    consumed_at = EXCLUDED.consumed_at,
    updated_at = NOW()::text;
