pub mod app_merchant_inventory_router;
pub mod http_route_manifest;
pub mod routes;
pub mod subject;
pub mod web_bootstrap;

pub use app_merchant_inventory_router::{
    app_merchant_inventory_router_with_postgres_pool, build_app_merchant_inventory_router,
    CommerceMerchantInventoryStore,
};
pub use http_route_manifest::app_route_manifest;
pub use routes::build_inventory_app_router_with_framework;
pub use web_bootstrap::wrap_router_with_web_framework_from_env;

use axum::Router;
use sdkwork_inventory_service_host::InventoryServiceHost;
use std::sync::Arc;

pub fn gateway_route_manifest() -> sdkwork_web_core::HttpRouteManifest {
    app_route_manifest()
}

pub async fn gateway_mount(host: Arc<InventoryServiceHost>) -> Router {
    build_inventory_app_router_with_framework(host).await
}

pub fn gateway_mount_business(host: Arc<InventoryServiceHost>) -> Router {
    routes::build_inventory_app_router(host)
}
