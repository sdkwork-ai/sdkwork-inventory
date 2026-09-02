use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_inventory_repository_sqlx::{
    BackendInventoryListPage, BackendInventoryMovementListQuery,
    BackendInventoryReservationListQuery, BackendInventoryStockListQuery,
    PostgresCommerceInventoryStore, UpdateBackendInventoryStockCommand,
};
use sdkwork_utils_rust::http_api::{
    PageInfo, PageMode, SdkWorkApiResponse, SdkWorkPageData, SdkWorkResourceData,
};
use sdkwork_web_core::{
    problem_response, ProblemCorrelation, WebFrameworkError, WebFrameworkErrorKind,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use crate::subject::app_runtime_subject_from_extension;

pub type CommerceBackendInventoryFuture<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<T, CommerceServiceError>> + Send + 'a>,
>;

pub trait CommerceBackendInventoryStore: Send + Sync {
    fn list_stocks<'a>(
        &'a self,
        query: BackendInventoryStockListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage>;

    fn update_stock<'a>(
        &'a self,
        command: UpdateBackendInventoryStockCommand,
    ) -> CommerceBackendInventoryFuture<'a, serde_json::Value>;

    fn list_reservations<'a>(
        &'a self,
        query: BackendInventoryReservationListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage>;

    fn list_movements<'a>(
        &'a self,
        query: BackendInventoryMovementListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage>;
}

#[derive(Clone)]
struct BackendInventoryState {
    store: Arc<dyn CommerceBackendInventoryStore>,
}

#[derive(Debug, Deserialize)]
struct StockListParams {
    sku_id: Option<String>,
    warehouse_id: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ReservationListParams {
    order_id: Option<String>,
    sku_id: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MovementListParams {
    sku_id: Option<String>,
    movement_type: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStockRequest {
    available_quantity: Option<i64>,
    safety_stock_quantity: Option<i64>,
    status: Option<String>,
}

pub fn backend_inventory_router_with_postgres_pool(pool: PgPool) -> Router {
    build_backend_inventory_router(Arc::new(PostgresCommerceInventoryStore::new(pool)))
}

pub fn build_backend_inventory_router(store: Arc<dyn CommerceBackendInventoryStore>) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/inventory/stocks",
            get(list_inventory_stocks),
        )
        .route(
            "/backend/v3/api/inventory/stocks/{stockId}",
            patch(update_inventory_stock),
        )
        .route(
            "/backend/v3/api/inventory/reservations",
            get(list_inventory_reservations),
        )
        .route(
            "/backend/v3/api/inventory/movements",
            get(list_inventory_movements),
        )
        .with_state(BackendInventoryState { store })
}

impl CommerceBackendInventoryStore for PostgresCommerceInventoryStore {
    fn list_stocks<'a>(
        &'a self,
        query: BackendInventoryStockListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage> {
        Box::pin(async move { self.list_backend_stocks(query).await })
    }

    fn update_stock<'a>(
        &'a self,
        command: UpdateBackendInventoryStockCommand,
    ) -> CommerceBackendInventoryFuture<'a, serde_json::Value> {
        Box::pin(async move { self.update_backend_stock(command).await })
    }

    fn list_reservations<'a>(
        &'a self,
        query: BackendInventoryReservationListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage> {
        Box::pin(async move { self.list_backend_reservations(query).await })
    }

    fn list_movements<'a>(
        &'a self,
        query: BackendInventoryMovementListQuery,
    ) -> CommerceBackendInventoryFuture<'a, BackendInventoryListPage> {
        Box::pin(async move { self.list_backend_movements(query).await })
    }
}

async fn list_inventory_stocks(
    State(state): State<BackendInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<StockListParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = BackendInventoryStockListQuery {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        sku_id: params.sku_id,
        warehouse_id: params.warehouse_id,
        status: params.status,
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(20),
    };

    match state.store.list_stocks(query).await {
        Ok(page) => success_inventory_page(page),
        Err(error) => inventory_error_response("inventory stocks list failed", error),
    }
}

async fn update_inventory_stock(
    State(state): State<BackendInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(stock_id): Path<String>,
    Json(body): Json<UpdateStockRequest>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdateBackendInventoryStockCommand {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        stock_id,
        available_quantity: body.available_quantity,
        safety_stock_quantity: body.safety_stock_quantity,
        status: body.status,
    };

    match state.store.update_stock(command).await {
        Ok(item) => success_resource_response(item),
        Err(error) => inventory_error_response("inventory stock update failed", error),
    }
}

async fn list_inventory_reservations(
    State(state): State<BackendInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<ReservationListParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = BackendInventoryReservationListQuery {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        order_id: params.order_id,
        sku_id: params.sku_id,
        status: params.status,
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(20),
    };

    match state.store.list_reservations(query).await {
        Ok(page) => success_inventory_page(page),
        Err(error) => inventory_error_response("inventory reservations list failed", error),
    }
}

async fn list_inventory_movements(
    State(state): State<BackendInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<MovementListParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = BackendInventoryMovementListQuery {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
        sku_id: params.sku_id,
        movement_type: params.movement_type,
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(20),
    };

    match state.store.list_movements(query).await {
        Ok(page) => success_inventory_page(page),
        Err(error) => inventory_error_response("inventory movements list failed", error),
    }
}

fn unauthorized_response(message: impl Into<String>) -> Response {
    api_problem_response(WebFrameworkErrorKind::MissingCredentials, message)
}

fn inventory_error_response(context: &str, error: CommerceServiceError) -> Response {
    match error.code() {
        "validation" => api_problem_response(WebFrameworkErrorKind::BadRequest, error.message()),
        "not_found" => api_problem_response(WebFrameworkErrorKind::NotFound, error.message()),
        "conflict" => api_problem_response(WebFrameworkErrorKind::Conflict, error.message()),
        "unauthenticated" => {
            api_problem_response(WebFrameworkErrorKind::MissingCredentials, error.message())
        }
        _ => api_problem_response(
            WebFrameworkErrorKind::DependencyUnavailable,
            format!("{context}: {}", error.message()),
        ),
    }
}

fn trace_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn success_inventory_page(page: BackendInventoryListPage) -> Response {
    let page_number = page.page.max(1);
    let page_size = page.page_size.max(1);
    let total = page.total.max(0);
    let total_pages = total.saturating_add(page_size - 1) / page_size;
    Json(SdkWorkApiResponse::success(
        SdkWorkPageData {
            items: page.items,
            page_info: PageInfo {
                mode: PageMode::Offset,
                page: Some(page_number as i32),
                page_size: Some(page_size as i32),
                total_items: Some(total.to_string()),
                total_pages: Some(total_pages as i32),
                next_cursor: None,
                has_more: Some(page_number * page_size < total),
            },
        },
        trace_id(),
    ))
    .into_response()
}

fn success_resource_response<T: Serialize>(item: T) -> Response {
    Json(SdkWorkApiResponse::success(
        SdkWorkResourceData { item },
        trace_id(),
    ))
    .into_response()
}

fn api_problem_response(kind: WebFrameworkErrorKind, message: impl Into<String>) -> Response {
    let trace_id = trace_id();
    problem_response(
        &WebFrameworkError {
            kind,
            message: message.into(),
            retry_after_seconds: None,
            auth_profile: None,
            failed_stage: None,
            reason: None,
        },
        ProblemCorrelation::from(Some(trace_id.as_str())),
    )
}
