use sdkwork_contract_service::CommerceServiceError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct MerchantInventoryScopeQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
}

impl MerchantInventoryScopeQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
    ) -> Result<Self, CommerceServiceError> {
        let tenant_id = tenant_id.trim();
        if tenant_id.is_empty() {
            return Err(CommerceServiceError::validation("tenant_id is required"));
        }
        let organization_id = organization_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        Ok(Self {
            tenant_id: tenant_id.to_owned(),
            organization_id,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MerchantInventoryListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

impl MerchantInventoryListQuery {
    pub fn new(
        tenant_id: &str,
        organization_id: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Self, CommerceServiceError> {
        let scope = MerchantInventoryScopeQuery::new(tenant_id, organization_id)?;
        if page < 1 {
            return Err(CommerceServiceError::validation(
                "page must be greater than or equal to 1",
            ));
        }
        if !(1..=200).contains(&page_size) {
            return Err(CommerceServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        Ok(Self {
            tenant_id: scope.tenant_id,
            organization_id: scope.organization_id,
            page,
            page_size,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendInventoryStockListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub sku_id: Option<String>,
    pub warehouse_id: Option<String>,
    pub status: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBackendInventoryStockCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub stock_id: String,
    pub available_quantity: Option<i64>,
    pub safety_stock_quantity: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendInventoryReservationListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub order_id: Option<String>,
    pub sku_id: Option<String>,
    pub status: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendInventoryMovementListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub sku_id: Option<String>,
    pub movement_type: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInventoryListPage {
    pub items: Vec<serde_json::Value>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PostgresCommerceInventoryStore {
    pool: PgPool,
}

impl PostgresCommerceInventoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_backend_stocks(
        &self,
        query: BackendInventoryStockListQuery,
    ) -> Result<BackendInventoryListPage, CommerceServiceError> {
        let page = query.page.max(1);
        let page_size = query.page_size.clamp(1, 200);
        let offset = (page - 1) * page_size;
        let organization_id = query.organization_id.as_deref().unwrap_or("");

        let total_row = sqlx::query(
            r#"
            SELECT COUNT(*) AS total
            FROM commerce_inventory_stock
            WHERE tenant_id = $1
              AND (organization_id = CASE WHEN COALESCE($3, '') = '' THEN '0' ELSE $2 END)
            "#,
        )
        .bind(&query.tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("failed to count inventory stocks", error))?;
        let total: i64 = total_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, shop_id, sku_id, warehouse_id, fulfillment_node_id,
                   on_hand_quantity, available_quantity, locked_quantity, reserved_quantity,
                   sold_quantity, safety_stock_quantity, version, status, created_at, updated_at
            FROM commerce_inventory_stock
            WHERE tenant_id = $1
              AND (organization_id = CASE WHEN COALESCE($3, '') = '' THEN '0' ELSE $2 END)
            ORDER BY updated_at DESC, id DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(&query.tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("failed to list inventory stocks", error))?;

        Ok(BackendInventoryListPage {
            items: rows.iter().map(map_stock_row).collect(),
            page,
            page_size,
            total,
        })
    }

    pub async fn update_backend_stock(
        &self,
        command: UpdateBackendInventoryStockCommand,
    ) -> Result<serde_json::Value, CommerceServiceError> {
        let now = current_timestamp_string();
        if command.available_quantity.is_none()
            && command.safety_stock_quantity.is_none()
            && command.status.is_none()
        {
            return Err(CommerceServiceError::validation(
                "at least one stock field must be provided",
            ));
        }

        let available_quantity = command.available_quantity.unwrap_or(0);
        let safety_stock_quantity = command.safety_stock_quantity.unwrap_or(0);
        let status = command.status.as_deref().unwrap_or("active");

        sqlx::query(
            r#"
            UPDATE commerce_inventory_stock
            SET available_quantity = $1,
                safety_stock_quantity = $2,
                status = $3,
                updated_at = $4
            WHERE tenant_id = $5 AND id = $6
            "#,
        )
        .bind(available_quantity)
        .bind(safety_stock_quantity)
        .bind(status)
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.stock_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("failed to update inventory stock", error))?;

        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, shop_id, sku_id, warehouse_id, fulfillment_node_id,
                   on_hand_quantity, available_quantity, locked_quantity, reserved_quantity,
                   sold_quantity, safety_stock_quantity, version, status, created_at, updated_at
            FROM commerce_inventory_stock
            WHERE tenant_id = $1 AND id = $2
            LIMIT 1
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.stock_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("failed to retrieve updated inventory stock", error))?;

        row.map(|row| map_stock_row(&row))
            .ok_or_else(|| CommerceServiceError::not_found("inventory stock was not found"))
    }

    pub async fn list_backend_reservations(
        &self,
        query: BackendInventoryReservationListQuery,
    ) -> Result<BackendInventoryListPage, CommerceServiceError> {
        list_simple_page(
            &self.pool,
            reservation_select_columns(),
            "commerce_inventory_reservation",
            "updated_at DESC, id DESC",
            SimplePageRequest {
                tenant_id: &query.tenant_id,
                organization_id: query.organization_id.as_deref(),
                page: query.page,
                page_size: query.page_size,
            },
            map_reservation_row,
        )
        .await
    }

    pub async fn list_backend_movements(
        &self,
        query: BackendInventoryMovementListQuery,
    ) -> Result<BackendInventoryListPage, CommerceServiceError> {
        list_simple_page(
            &self.pool,
            movement_select_columns(),
            "commerce_inventory_movement",
            "occurred_at DESC, id DESC",
            SimplePageRequest {
                tenant_id: &query.tenant_id,
                organization_id: query.organization_id.as_deref(),
                page: query.page,
                page_size: query.page_size,
            },
            map_movement_row,
        )
        .await
    }

    pub async fn list_merchant_stocks(
        &self,
        query: MerchantInventoryListQuery,
    ) -> Result<BackendInventoryListPage, CommerceServiceError> {
        list_simple_page(
            &self.pool,
            merchant_stock_select_columns(),
            "commerce_inventory_stock",
            "updated_at DESC, id DESC",
            SimplePageRequest {
                tenant_id: &query.tenant_id,
                organization_id: query.organization_id.as_deref(),
                page: query.page,
                page_size: query.page_size,
            },
            map_merchant_stock_row,
        )
        .await
    }

    pub async fn create_merchant_adjustment(
        &self,
        scope: MerchantInventoryScopeQuery,
        stock_id: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, CommerceServiceError> {
        let quantity_delta = payload
            .get("quantityDelta")
            .or_else(|| payload.get("quantity_delta"))
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        sqlx::query(
            r#"
            UPDATE commerce_inventory_stock
            SET available_quantity = available_quantity + $1,
                updated_at = $2
            WHERE tenant_id = $3
              AND id = $4
            "#,
        )
        .bind(quantity_delta)
        .bind(current_timestamp_string())
        .bind(&scope.tenant_id)
        .bind(stock_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("failed to adjust merchant inventory stock", error))?;

        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, sku_id, warehouse_id, fulfillment_node_id,
                   available_quantity, reserved_quantity, inbound_quantity, damaged_quantity,
                   status, version, created_at, updated_at
            FROM commerce_inventory_stock
            WHERE tenant_id = $1
              AND id = $2
            LIMIT 1
            "#,
        )
        .bind(&scope.tenant_id)
        .bind(stock_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("failed to retrieve adjusted merchant stock", error))?;

        row.map(|row| map_merchant_stock_row(&row))
            .ok_or_else(|| CommerceServiceError::not_found("inventory stock was not found"))
    }
}

struct SimplePageRequest<'a> {
    tenant_id: &'a str,
    organization_id: Option<&'a str>,
    page: i64,
    page_size: i64,
}

async fn list_simple_page(
    pool: &PgPool,
    select_columns: &str,
    table: &str,
    order_by: &str,
    request: SimplePageRequest<'_>,
    map_row: fn(&sqlx::postgres::PgRow) -> serde_json::Value,
) -> Result<BackendInventoryListPage, CommerceServiceError> {
    let page = request.page.max(1);
    let page_size = request.page_size.clamp(1, 200);
    let offset = (page - 1) * page_size;
    let organization_id = request.organization_id.unwrap_or("");

    let count_sql = format!("SELECT COUNT(*) AS total FROM {table} WHERE tenant_id = $1 AND (organization_id = CASE WHEN COALESCE($3, '') = '' THEN '0' ELSE $2 END)");
    let total_row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .bind(request.tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .map_err(|error| store_error("failed to count inventory rows", error))?;
    let total: i64 = total_row.try_get("total").unwrap_or(0);

    let list_sql = format!(
        "SELECT {select_columns} FROM {table} WHERE tenant_id = $1 AND (organization_id = CASE WHEN COALESCE($3, '') = '' THEN '0' ELSE $2 END) ORDER BY {order_by} LIMIT $4 OFFSET $5"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(list_sql.as_str()))
        .bind(request.tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to list inventory rows", error))?;

    Ok(BackendInventoryListPage {
        items: rows.iter().map(map_row).collect(),
        page,
        page_size,
        total,
    })
}

fn reservation_select_columns() -> &'static str {
    "id, tenant_id, organization_id, reservation_no, order_id, checkout_session_id, order_item_id, reservation_source_type, reservation_source_id, reservation_type, sku_id, warehouse_id, fulfillment_node_id, quantity, reserved_quantity, consumed_quantity, released_quantity, status, release_reason_code, request_no, idempotency_key, expires_at, consumed_at, released_at, created_at, updated_at"
}

fn movement_select_columns() -> &'static str {
    "id, tenant_id, organization_id, movement_no, sku_id, warehouse_id, fulfillment_node_id, movement_type, source_type, quantity, direction, quantity_before, quantity_after, business_type, source_id, request_no, idempotency_key, occurred_at, created_at"
}

fn merchant_stock_select_columns() -> &'static str {
    "id, tenant_id, organization_id, sku_id, warehouse_id, fulfillment_node_id, available_quantity, reserved_quantity, inbound_quantity, damaged_quantity, status, version, created_at, updated_at"
}

fn map_stock_row(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": string_cell(row, "id"),
        "tenantId": string_cell(row, "tenant_id"),
        "organizationId": optional_string_cell(row, "organization_id"),
        "shopId": optional_string_cell(row, "shop_id"),
        "skuId": string_cell(row, "sku_id"),
        "warehouseId": optional_string_cell(row, "warehouse_id"),
        "fulfillmentNodeId": string_cell(row, "fulfillment_node_id"),
        "onHandQuantity": i64_cell(row, "on_hand_quantity"),
        "availableQuantity": i64_cell(row, "available_quantity"),
        "lockedQuantity": i64_cell(row, "locked_quantity"),
        "reservedQuantity": i64_cell(row, "reserved_quantity"),
        "soldQuantity": i64_cell(row, "sold_quantity"),
        "safetyStockQuantity": i64_cell(row, "safety_stock_quantity"),
        "version": i64_cell(row, "version"),
        "status": string_cell(row, "status"),
        "createdAt": string_cell(row, "created_at"),
        "updatedAt": string_cell(row, "updated_at"),
    })
}

fn map_reservation_row(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": string_cell(row, "id"),
        "tenantId": string_cell(row, "tenant_id"),
        "organizationId": optional_string_cell(row, "organization_id"),
        "reservationNo": string_cell(row, "reservation_no"),
        "orderId": optional_string_cell(row, "order_id"),
        "skuId": string_cell(row, "sku_id"),
        "status": string_cell(row, "status"),
        "quantity": i64_cell(row, "quantity"),
        "createdAt": string_cell(row, "created_at"),
        "updatedAt": string_cell(row, "updated_at"),
    })
}

fn map_movement_row(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": string_cell(row, "id"),
        "tenantId": string_cell(row, "tenant_id"),
        "movementNo": string_cell(row, "movement_no"),
        "skuId": string_cell(row, "sku_id"),
        "movementType": string_cell(row, "movement_type"),
        "quantity": i64_cell(row, "quantity"),
        "direction": string_cell(row, "direction"),
        "occurredAt": string_cell(row, "occurred_at"),
        "createdAt": string_cell(row, "created_at"),
    })
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn i64_cell(row: &sqlx::postgres::PgRow, column: &str) -> i64 {
    row.try_get::<i64, _>(column).unwrap_or(0)
}

fn map_merchant_stock_row(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": string_cell(row, "id"),
        "tenantId": string_cell(row, "tenant_id"),
        "organizationId": optional_string_cell(row, "organization_id"),
        "skuId": string_cell(row, "sku_id"),
        "warehouseId": optional_string_cell(row, "warehouse_id"),
        "fulfillmentNodeId": string_cell(row, "fulfillment_node_id"),
        "availableQuantity": i64_cell(row, "available_quantity"),
        "reservedQuantity": i64_cell(row, "reserved_quantity"),
        "inboundQuantity": i64_cell(row, "inbound_quantity"),
        "damagedQuantity": i64_cell(row, "damaged_quantity"),
        "status": string_cell(row, "status"),
        "version": i64_cell(row, "version"),
        "createdAt": string_cell(row, "created_at"),
        "updatedAt": string_cell(row, "updated_at"),
    })
}

fn store_error(message: &str, error: impl std::fmt::Display) -> CommerceServiceError {
    CommerceServiceError::storage(format!("{message}: {error}"))
}

fn current_timestamp_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}")
}
