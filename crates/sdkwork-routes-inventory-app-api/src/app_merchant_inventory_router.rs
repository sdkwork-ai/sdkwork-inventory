use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_inventory_repository_sqlx::{
    BackendInventoryListPage, MerchantInventoryListQuery, MerchantInventoryScopeQuery,
    PostgresCommerceInventoryStore,
};
use sdkwork_utils_rust::http_api::{
    validated_offset_list_params, PageInfo, PageMode, SdkWorkApiResponse, SdkWorkPageData,
    SdkWorkResourceData,
};
use sdkwork_web_core::{
    problem_response, ProblemCorrelation, WebFrameworkError, WebFrameworkErrorKind,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use crate::subject::app_runtime_subject_from_extension;

pub type CommerceMerchantInventoryFuture<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<T, CommerceServiceError>> + Send + 'a>,
>;

pub trait CommerceMerchantInventoryStore: Send + Sync {
    fn list_merchant_stocks<'a>(
        &'a self,
        query: MerchantInventoryListQuery,
    ) -> CommerceMerchantInventoryFuture<'a, BackendInventoryListPage>;

    fn create_merchant_adjustment<'a>(
        &'a self,
        scope: MerchantInventoryScopeQuery,
        stock_id: String,
        payload: serde_json::Value,
    ) -> CommerceMerchantInventoryFuture<'a, serde_json::Value>;
}

#[derive(Clone)]
struct MerchantInventoryState {
    store: Arc<dyn CommerceMerchantInventoryStore>,
}

#[derive(Debug, Deserialize)]
struct MerchantStockListParams {
    page: Option<i64>,
    page_size: Option<i64>,
}

pub fn app_merchant_inventory_router_with_postgres_pool(pool: PgPool) -> Router {
    build_app_merchant_inventory_router(Arc::new(PostgresCommerceInventoryStore::new(pool)))
}

pub fn build_app_merchant_inventory_router(
    store: Arc<dyn CommerceMerchantInventoryStore>,
) -> Router {
    Router::new()
        .route(
            "/app/v3/api/shops/current/inventory/stocks",
            get(list_current_inventory_stocks),
        )
        .route(
            "/app/v3/api/shops/current/inventory/stocks/{stockId}/adjustments",
            post(create_current_inventory_adjustment),
        )
        .with_state(MerchantInventoryState { store })
}

impl CommerceMerchantInventoryStore for PostgresCommerceInventoryStore {
    fn list_merchant_stocks<'a>(
        &'a self,
        query: MerchantInventoryListQuery,
    ) -> CommerceMerchantInventoryFuture<'a, BackendInventoryListPage> {
        Box::pin(async move { self.list_merchant_stocks(query).await })
    }

    fn create_merchant_adjustment<'a>(
        &'a self,
        scope: MerchantInventoryScopeQuery,
        stock_id: String,
        payload: serde_json::Value,
    ) -> CommerceMerchantInventoryFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.create_merchant_adjustment(scope, &stock_id, payload)
                .await
        })
    }
}

async fn merchant_scope(
    runtime_context: Option<Extension<IamAppContext>>,
) -> Result<MerchantInventoryScopeQuery, Response> {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return Err(unauthorized_response(message)),
    };
    MerchantInventoryScopeQuery::new(&subject.tenant_id, subject.organization_id.as_deref())
        .map_err(|error| validation_response(error.message()))
}

async fn merchant_list_query(
    runtime_context: Option<Extension<IamAppContext>>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<MerchantInventoryListQuery, Response> {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return Err(unauthorized_response(message)),
    };
    let page = validated_offset_list_params(page, page_size).map_err(|_| {
        validation_response("page must be >= 1 and page_size must be between 1 and 200")
    })?;
    MerchantInventoryListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        page.page,
        page.page_size,
    )
    .map_err(|error| validation_response(error.message()))
}

async fn list_current_inventory_stocks(
    State(state): State<MerchantInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<MerchantStockListParams>,
) -> Response {
    let query = match merchant_list_query(runtime_context, params.page, params.page_size).await {
        Ok(query) => query,
        Err(resp) => return resp,
    };
    match state.store.list_merchant_stocks(query).await {
        Ok(page) => success_page_response(page.items, page.page, page.page_size, page.total),
        Err(error) => inventory_error_response("merchant inventory stocks list failed", error),
    }
}

async fn create_current_inventory_adjustment(
    State(state): State<MerchantInventoryState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(stock_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let scope = match merchant_scope(runtime_context).await {
        Ok(scope) => scope,
        Err(resp) => return resp,
    };
    match state
        .store
        .create_merchant_adjustment(scope, stock_id, body)
        .await
    {
        Ok(item) => created_resource_response(item),
        Err(error) => inventory_error_response("merchant inventory adjustment failed", error),
    }
}

fn unauthorized_response(message: impl Into<String>) -> Response {
    api_problem_response(WebFrameworkErrorKind::MissingCredentials, message)
}

fn validation_response(message: impl Into<String>) -> Response {
    api_problem_response(WebFrameworkErrorKind::BadRequest, message)
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

fn success_page_response<T: Serialize>(
    items: Vec<T>,
    page: i64,
    page_size: i64,
    total: i64,
) -> Response {
    let total_pages = if page_size == 0 {
        0
    } else {
        (total.saturating_add(page_size - 1) / page_size) as i32
    };
    Json(SdkWorkApiResponse::success(
        SdkWorkPageData {
            items,
            page_info: PageInfo {
                mode: PageMode::Offset,
                page: Some(page as i32),
                page_size: Some(page_size as i32),
                total_items: Some(total.max(0).to_string()),
                total_pages: Some(total_pages),
                next_cursor: None,
                has_more: Some(page * page_size < total),
            },
        },
        trace_id(),
    ))
    .into_response()
}

fn created_resource_response<T: Serialize>(item: T) -> Response {
    (
        StatusCode::CREATED,
        Json(SdkWorkApiResponse::success(
            SdkWorkResourceData { item },
            trace_id(),
        )),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stock_page_response_reports_total_and_has_more() {
        let response = success_page_response(vec![serde_json::json!({ "id": "stock-3" })], 2, 2, 5);
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("response json");
        assert_eq!(json["code"], 0);
        assert_eq!(json["data"]["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(json["data"]["pageInfo"]["page"], 2);
        assert_eq!(json["data"]["pageInfo"]["pageSize"], 2);
        assert_eq!(json["data"]["pageInfo"]["totalItems"], "5");
        assert_eq!(json["data"]["pageInfo"]["hasMore"], true);
    }
}
