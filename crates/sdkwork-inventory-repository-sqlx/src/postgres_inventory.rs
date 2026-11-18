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

/// Operator stock adjustment.
///
/// The command is a *delta*, not an absolute value: the repository reads the
/// current quantity under a row lock, records the before/after pair in
/// `commerce_inventory_movement`, and only then commits the new quantity. An
/// absolute overwrite would silently lose concurrent movements.
#[derive(Debug, Clone, Deserialize)]
pub struct AdjustBackendInventoryStockCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub stock_id: String,
    pub quantity_delta: i64,
    pub movement_type: String,
    pub reason: Option<String>,
    pub request_no: Option<String>,
    /// Client operation key; a replay with the same key returns the current
    /// stock state instead of applying the delta twice.
    pub idempotency_key: String,
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
        if let Some(available_quantity) = command.available_quantity {
            if available_quantity < 0 {
                return Err(CommerceServiceError::validation(
                    "available_quantity must be greater than or equal to 0",
                ));
            }
        }
        if let Some(safety_stock_quantity) = command.safety_stock_quantity {
            if safety_stock_quantity < 0 {
                return Err(CommerceServiceError::validation(
                    "safety_stock_quantity must be greater than or equal to 0",
                ));
            }
        }
        let organization_id = command.organization_id.clone().unwrap_or_default();

        sqlx::query(
            r#"
            UPDATE commerce_inventory_stock
            SET available_quantity = COALESCE($1, available_quantity),
                safety_stock_quantity = COALESCE($2, safety_stock_quantity),
                status = COALESCE($3, status),
                version = version + 1,
                updated_at = $4
            WHERE tenant_id = $5
              AND id = $6
              AND ($7 = '' OR organization_id = $7)
            "#,
        )
        .bind(command.available_quantity)
        .bind(command.safety_stock_quantity)
        .bind(command.status.as_deref())
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.stock_id)
        .bind(&organization_id)
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

    /// Records an operator stock adjustment.
    ///
    /// One transaction locks the stock row, validates the resulting quantity,
    /// appends the movement ledger row with the before/after pair, and advances
    /// the row version. A negative resulting quantity is rejected, so an
    /// outbound adjustment can never drive available stock below zero.
    pub async fn adjust_backend_stock(
        &self,
        command: AdjustBackendInventoryStockCommand,
    ) -> Result<serde_json::Value, CommerceServiceError> {
        if command.quantity_delta == 0 {
            return Err(CommerceServiceError::validation(
                "quantity_delta must not be zero",
            ));
        }
        let movement_type = command.movement_type.trim().to_ascii_lowercase();
        if !matches!(
            movement_type.as_str(),
            "inbound" | "outbound" | "stocktake"
        ) {
            return Err(CommerceServiceError::validation(
                "movement_type must be one of inbound, outbound, stocktake",
            ));
        }
        if command.idempotency_key.trim().is_empty() {
            return Err(CommerceServiceError::validation(
                "idempotency_key must not be empty",
            ));
        }
        let movement_id = adjustment_movement_id(&command.stock_id, &command.idempotency_key);

        let now = current_timestamp_string();
        let organization_id = command.organization_id.clone().unwrap_or_default();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to start inventory adjustment", error))?;

        let replayed = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id FROM commerce_inventory_movement
            WHERE tenant_id = $1 AND id = $2
            LIMIT 1
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&movement_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to check inventory adjustment replay", error))?;

        if replayed.is_some() {
            let current = sqlx::query(
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
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to reload replayed inventory stock", error))?
            .ok_or_else(|| CommerceServiceError::not_found("inventory stock was not found"))?;
            transaction
                .commit()
                .await
                .map_err(|error| store_error("failed to commit replayed inventory adjustment", error))?;
            return Ok(map_stock_row(&current));
        }

        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, sku_id, warehouse_id,
                   on_hand_quantity, available_quantity
            FROM commerce_inventory_stock
            WHERE tenant_id = $1
              AND id = $2
              AND ($3 = '' OR organization_id = $3)
            FOR UPDATE
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.stock_id)
        .bind(&organization_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to lock inventory stock for adjustment", error))?
        .ok_or_else(|| CommerceServiceError::not_found("inventory stock was not found"))?;

        let available_before: i64 = row.try_get("available_quantity").unwrap_or(0);
        let on_hand_before: i64 = row.try_get("on_hand_quantity").unwrap_or(0);
        let available_after = available_before + command.quantity_delta;
        let on_hand_after = on_hand_before + command.quantity_delta;
        if available_after < 0 {
            return Err(CommerceServiceError::conflict(
                "inventory adjustment would make available quantity negative",
            ));
        }
        if on_hand_after < 0 {
            return Err(CommerceServiceError::conflict(
                "inventory adjustment would make on-hand quantity negative",
            ));
        }

        let stock_organization_id: String = row.try_get("organization_id").unwrap_or_default();
        let sku_id: String = row.try_get("sku_id").unwrap_or_default();
        let warehouse_id: Option<String> = row.try_get("warehouse_id").ok().flatten();

        sqlx::query(
            r#"
            UPDATE commerce_inventory_stock
            SET available_quantity = $1,
                on_hand_quantity = $2,
                version = version + 1,
                updated_at = $3
            WHERE tenant_id = $4
              AND id = $5
            "#,
        )
        .bind(available_after)
        .bind(on_hand_after)
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.stock_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to apply inventory adjustment", error))?;

        let movement_no = adjustment_movement_no(&movement_id, &now);
        let direction = if command.quantity_delta > 0 {
            "in"
        } else {
            "out"
        };
        let quantity = command.quantity_delta.abs();
        let request_no = command
            .request_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| movement_no.clone());

        sqlx::query(
            r#"
            INSERT INTO commerce_inventory_movement (
                id, tenant_id, organization_id, movement_no, sku_id, warehouse_id,
                movement_type, source_type, quantity, direction, quantity_before,
                quantity_after, business_type, source_id, reason, request_no,
                occurred_at, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, 'admin_adjustment', $8, $9, $10,
                $11, $7, $12, $13, $14,
                $15, $15
            )
            "#,
        )
        .bind(&movement_id)
        .bind(&command.tenant_id)
        .bind(&stock_organization_id)
        .bind(&movement_no)
        .bind(&sku_id)
        .bind(&warehouse_id)
        .bind(&movement_type)
        .bind(quantity)
        .bind(direction)
        .bind(available_before)
        .bind(available_after)
        .bind(&command.stock_id)
        .bind(&command.reason)
        .bind(&request_no)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to append inventory movement", error))?;

        let updated = sqlx::query(
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
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to reload adjusted inventory stock", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit inventory adjustment", error))?;

        Ok(map_stock_row(&updated))
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
        let quantity_delta = parse_adjustment_delta(&payload).ok_or_else(|| {
            CommerceServiceError::validation("quantityDelta must be a non-zero int64 string")
        })?;
        let organization_id = scope.organization_id.clone().unwrap_or_default();
        let now = current_timestamp_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to start merchant inventory adjustment", error))?;

        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, sku_id, warehouse_id,
                   on_hand_quantity, available_quantity
            FROM commerce_inventory_stock
            WHERE tenant_id = $1
              AND id = $2
              AND ($3 = '' OR organization_id = $3)
            FOR UPDATE
            "#,
        )
        .bind(&scope.tenant_id)
        .bind(stock_id)
        .bind(&organization_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to lock merchant inventory stock", error))?
        .ok_or_else(|| CommerceServiceError::not_found("inventory stock was not found"))?;

        let available_before: i64 = row.try_get("available_quantity").unwrap_or(0);
        let on_hand_before: i64 = row.try_get("on_hand_quantity").unwrap_or(0);
        let available_after = available_before + quantity_delta;
        let on_hand_after = on_hand_before + quantity_delta;
        if available_after < 0 || on_hand_after < 0 {
            return Err(CommerceServiceError::conflict(
                "inventory adjustment would make the stock quantity negative",
            ));
        }

        let stock_organization_id: String = row.try_get("organization_id").unwrap_or_default();
        let sku_id: String = row.try_get("sku_id").unwrap_or_default();
        let warehouse_id: Option<String> = row.try_get("warehouse_id").ok().flatten();

        sqlx::query(
            r#"
            UPDATE commerce_inventory_stock
            SET available_quantity = $1,
                on_hand_quantity = $2,
                version = version + 1,
                updated_at = $3
            WHERE tenant_id = $4
              AND id = $5
            "#,
        )
        .bind(available_after)
        .bind(on_hand_after)
        .bind(&now)
        .bind(&scope.tenant_id)
        .bind(stock_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to apply merchant inventory adjustment", error))?;

        let movement_id = uuid::Uuid::new_v4().to_string();
        let movement_no = adjustment_movement_no(&movement_id, &now);
        let direction = if quantity_delta > 0 { "in" } else { "out" };

        sqlx::query(
            r#"
            INSERT INTO commerce_inventory_movement (
                id, tenant_id, organization_id, movement_no, sku_id, warehouse_id,
                movement_type, source_type, quantity, direction, quantity_before,
                quantity_after, business_type, source_id, request_no, occurred_at, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                'stocktake', 'merchant_adjustment', $7, $8, $9,
                $10, 'stocktake', $11, $12, $13, $13
            )
            "#,
        )
        .bind(&movement_id)
        .bind(&scope.tenant_id)
        .bind(&stock_organization_id)
        .bind(&movement_no)
        .bind(&sku_id)
        .bind(&warehouse_id)
        .bind(quantity_delta.abs())
        .bind(direction)
        .bind(available_before)
        .bind(available_after)
        .bind(stock_id)
        .bind(&movement_no)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to append merchant inventory movement", error))?;

        let updated = sqlx::query(
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
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("failed to reload adjusted merchant stock", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit merchant inventory adjustment", error))?;

        Ok(map_merchant_stock_row(&updated))
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
        "onHandQuantity": int64_string_cell(row, "on_hand_quantity"),
        "availableQuantity": int64_string_cell(row, "available_quantity"),
        "lockedQuantity": int64_string_cell(row, "locked_quantity"),
        "reservedQuantity": int64_string_cell(row, "reserved_quantity"),
        "soldQuantity": int64_string_cell(row, "sold_quantity"),
        "safetyStockQuantity": int64_string_cell(row, "safety_stock_quantity"),
        "version": int64_string_cell(row, "version"),
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
        "quantity": int64_string_cell(row, "quantity"),
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
        "quantity": int64_string_cell(row, "quantity"),
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

fn int64_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    row.try_get::<i64, _>(column)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

/// Reads a signed quantity delta from the adjustment payload.
///
/// API_SPEC §13.6 puts int64 on the wire as a decimal string; the numeric form
/// stays accepted only for internal callers that already hold a `Value` number.
/// A zero delta is not an adjustment and is rejected here.
fn parse_adjustment_delta(payload: &serde_json::Value) -> Option<i64> {
    payload
        .get("quantityDelta")
        .or_else(|| payload.get("quantity_delta"))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse::<i64>().ok()))
        })
        .filter(|value| *value != 0)
}

/// Deterministic movement id for a stock adjustment.
///
/// The same client operation key must never append a second ledger row, so the
/// id is derived from the stock and the key instead of a random uuid; a replay
/// finds the existing row and returns the current stock state.
fn adjustment_movement_id(stock_id: &str, idempotency_key: &str) -> String {
    let mut id = String::with_capacity(stock_id.len() + idempotency_key.len() + 5);
    id.push_str("imv-");
    for character in stock_id
        .chars()
        .chain(std::iter::once('-'))
        .chain(idempotency_key.chars())
    {
        id.push(if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            character
        } else {
            '-'
        });
    }
    id
}

fn adjustment_movement_no(movement_id: &str, occurred_at: &str) -> String {    let short_id: String = movement_id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(8)
        .collect();
    let timestamp_digits: String = occurred_at.chars().filter(char::is_ascii_digit).collect();
    format!("IMV-{timestamp_digits}-{short_id}")
}

fn map_merchant_stock_row(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": string_cell(row, "id"),
        "tenantId": string_cell(row, "tenant_id"),
        "organizationId": optional_string_cell(row, "organization_id"),
        "skuId": string_cell(row, "sku_id"),
        "warehouseId": optional_string_cell(row, "warehouse_id"),
        "fulfillmentNodeId": string_cell(row, "fulfillment_node_id"),
        "availableQuantity": int64_string_cell(row, "available_quantity"),
        "reservedQuantity": int64_string_cell(row, "reserved_quantity"),
        "inboundQuantity": int64_string_cell(row, "inbound_quantity"),
        "damagedQuantity": int64_string_cell(row, "damaged_quantity"),
        "status": string_cell(row, "status"),
        "version": int64_string_cell(row, "version"),
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

#[cfg(test)]
mod adjustment_validation_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn store() -> PostgresCommerceInventoryStore {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@127.0.0.1:1/inventory")
            .expect("lazy pool must be constructible without a server");
        PostgresCommerceInventoryStore::new(pool)
    }

    fn command(quantity_delta: i64, movement_type: &str) -> AdjustBackendInventoryStockCommand {
        AdjustBackendInventoryStockCommand {
            tenant_id: "t-1".to_string(),
            organization_id: None,
            stock_id: "stock-1".to_string(),
            quantity_delta,
            movement_type: movement_type.to_string(),
            reason: None,
            request_no: None,
            idempotency_key: "test-adjustment-key".to_string(),
        }
    }

    #[tokio::test]
    async fn rejects_zero_delta_before_touching_the_database() {
        let error = store()
            .adjust_backend_stock(command(0, "inbound"))
            .await
            .expect_err("a zero delta must be rejected");
        assert_eq!(error.code(), "validation");
    }

    #[tokio::test]
    async fn rejects_negative_safety_stock_before_touching_the_database() {
        let error = store()
            .update_backend_stock(UpdateBackendInventoryStockCommand {
                tenant_id: "t-1".to_string(),
                organization_id: None,
                stock_id: "stock-1".to_string(),
                available_quantity: None,
                safety_stock_quantity: Some(-1),
                status: None,
            })
            .await
            .expect_err("a negative safety stock must be rejected");
        assert_eq!(error.code(), "validation");
    }

    #[tokio::test]
    async fn rejects_unknown_movement_type_before_touching_the_database() {
        let error = store()
            .adjust_backend_stock(command(1, "teleport"))
            .await
            .expect_err("an unknown movement type must be rejected");
        assert_eq!(error.code(), "validation");
    }
}
