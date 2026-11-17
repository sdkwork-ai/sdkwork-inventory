export interface AdjustInventoryStockRequest {
  /** Signed quantity delta; positive records an inbound movement, negative an outbound one. */
  quantityDelta: string;
  movementType?: 'inbound' | 'outbound' | 'stocktake';
  reason?: string;
  requestNo?: string;
}
