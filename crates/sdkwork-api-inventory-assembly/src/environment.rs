use sdkwork_web_bootstrap::WebModule;
use std::sync::Arc;

use sdkwork_database_sqlx::DatabasePool;

use sdkwork_inventory_service_host::InventoryServiceHost;
use sdkwork_web_bootstrap::DatabasePoolReadinessCheck;

use crate::bootstrap::{
    assemble_api_router, assemble_app_api_contribution, assemble_backend_api_contribution,
    ApiAssembly, ApiAssemblyContext,
};

pub struct ApiAssemblyRuntime {
    pub contribution: ApiAssembly,
    pub database_pool: DatabasePool,
}

async fn context_from_env() -> Result<ApiAssemblyContext, String> {
    let host = Arc::new(InventoryServiceHost::from_env().await?);
    let readiness_check = Arc::new(DatabasePoolReadinessCheck::new(
        host.database_pool().clone(),
    ));
    Ok(ApiAssemblyContext {
        host,
        domain_context_injectors: Vec::new(),
        readiness_check,
    })
}

async fn context_from_pool(pool: DatabasePool) -> Result<ApiAssemblyContext, String> {
    let host = Arc::new(InventoryServiceHost::from_pool(pool).await?);
    let readiness_check = Arc::new(DatabasePoolReadinessCheck::new(
        host.database_pool().clone(),
    ));
    Ok(ApiAssemblyContext {
        host,
        domain_context_injectors: Vec::new(),
        readiness_check,
    })
}

/// Assemble the full inventory router against a caller-provided database pool so
/// the platform cloud gateway can share its process-wide PostgreSQL pool.
pub async fn assemble_api_router_with_pool(pool: DatabasePool) -> Result<ApiAssembly, String> {
    assemble_api_router(context_from_pool(pool).await?).await
}

pub async fn assemble_api_router_from_env() -> Result<ApiAssembly, String> {
    assemble_api_router(context_from_env().await?).await
}

pub async fn assemble_api_router_runtime() -> Result<ApiAssemblyRuntime, String> {
    let context = context_from_env().await?;
    let database_pool = context.host.database_pool().clone();
    let contribution = assemble_api_router(context).await?;
    Ok(ApiAssemblyRuntime {
        contribution,
        database_pool,
    })
}

pub async fn assemble_app_api_contribution_from_env() -> Result<ApiAssembly, String> {
    assemble_app_api_contribution(context_from_env().await?).await
}

pub async fn assemble_backend_api_contribution_from_env() -> Result<ApiAssembly, String> {
    assemble_backend_api_contribution(context_from_env().await?).await
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
pub async fn web_module() -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router_from_env().await?,
    ))
}

/// Same as [`web_module`] but composed on a process-shared database pool
/// (platform gateways, API_ASSEMBLY_SPEC §4.1.1).
pub async fn web_module_with_pool(pool: DatabasePool) -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router_with_pool(pool).await?,
    ))
}
