use sdkwork_api_inventory_assembly::assemble_api_router_runtime;
use sdkwork_iam_web_adapter::{
    build_web_framework_builder, iam_web_request_context_resolver_from_database_pool_for_audiences,
    iam_web_request_context_resolver_from_env, IamAuditEmitter, IamSecurityEventEmitter,
};
use sdkwork_web_bootstrap::{infra_public_path_prefixes, ApiModuleRegistry, ComposedApiAssembly};
use std::sync::Arc;

const APPLICATION_ID: &str = "sdkwork-inventory";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    sdkwork_web_bootstrap::init_tracing_from_env();
    let runtime = assemble_api_router_runtime().await?;
    let assembly = runtime.contribution;
    let environment = std::env::var("SDKWORK_ENVIRONMENT")
        .or_else(|_| std::env::var("SDKWORK_INVENTORY_ENVIRONMENT"))
        .unwrap_or_else(|_| "development".to_owned());
    let production = matches!(
        environment.trim().to_ascii_lowercase().as_str(),
        "prod" | "production"
    );
    let resolver = if production {
        iam_web_request_context_resolver_from_database_pool_for_audiences(
            runtime.database_pool.clone(),
            &[APPLICATION_ID],
        )
        .await?
    } else {
        iam_web_request_context_resolver_from_env().await
    };
    let mut framework = build_web_framework_builder(
        resolver,
        assembly.route_manifest.clone(),
        infra_public_path_prefixes(),
    );
    if production {
        let postgres_pool = runtime
            .database_pool
            .as_postgres()
            .cloned()
            .ok_or("production Inventory gateway requires PostgreSQL")?;
        framework = framework
            .audit_emitter(Arc::new(IamAuditEmitter::new(
                postgres_pool.clone(),
                APPLICATION_ID,
                environment.clone(),
            )))
            .security_event_emitter(Arc::new(IamSecurityEventEmitter::new(
                postgres_pool,
                environment,
            )));
    }
    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_modules(vec![assembly]);
    let app = module_registry
        .try_compose("SDKWork Inventory API")?
        .into_hosted(framework)
        .router;
    let addr = std::env::var("INVENTORY_API_BIND").unwrap_or_else(|_| "0.0.0.0:18092".to_owned());
    let addr = addr.parse()?;
    sdkwork_web_bootstrap::serve(app, addr).await?;
    Ok(())
}
