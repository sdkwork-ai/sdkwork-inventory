pub mod backend_inventory_router;
pub mod http_route_manifest;
pub mod subject;
pub mod web_bootstrap;

pub use backend_inventory_router::{
    backend_inventory_router_with_postgres_pool, build_backend_inventory_router,
    CommerceBackendInventoryStore,
};
pub use http_route_manifest::backend_route_manifest;
pub use web_bootstrap::wrap_router_with_web_framework_from_env;

use axum::Router;
use sdkwork_inventory_service_host::InventoryServiceHost;
use std::sync::Arc;

pub fn gateway_route_manifest() -> sdkwork_web_core::HttpRouteManifest {
    backend_route_manifest()
}

pub fn build_inventory_backend_router(host: Arc<InventoryServiceHost>) -> Router {
    let pool = host
        .database_pool()
        .as_postgres()
        .expect("inventory backend-api requires an authoritative PostgreSQL pool");
    backend_inventory_router_with_postgres_pool(pool.clone())
}

pub async fn build_inventory_backend_router_with_framework(
    host: Arc<InventoryServiceHost>,
) -> Router {
    wrap_router_with_web_framework_from_env(build_inventory_backend_router(host)).await
}

pub async fn gateway_mount(host: Arc<InventoryServiceHost>) -> Router {
    build_inventory_backend_router_with_framework(host).await
}

pub fn gateway_mount_business(host: Arc<InventoryServiceHost>) -> Router {
    build_inventory_backend_router(host)
}
