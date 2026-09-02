use sdkwork_database_sqlx::DatabasePool;
use sdkwork_inventory_database_host::{
    bootstrap_inventory_database_from_env, InventoryDatabaseHost,
};

pub struct InventoryServiceHost {
    database: InventoryDatabaseHost,
}

impl InventoryServiceHost {
    pub async fn new() -> Self {
        Self::from_env()
            .await
            .expect("inventory service host bootstrap failed")
    }

    pub async fn from_env() -> Result<Self, String> {
        let database = bootstrap_inventory_database_from_env().await?;
        Ok(Self { database })
    }

    /// Build the inventory service host against a caller-provided database pool so
    /// the platform cloud gateway can share its process-wide PostgreSQL pool.
    pub async fn from_pool(pool: DatabasePool) -> Result<Self, String> {
        let database =
            sdkwork_inventory_database_host::bootstrap_inventory_database_with_pool(pool).await?;
        Ok(Self { database })
    }

    pub fn database_pool(&self) -> &DatabasePool {
        self.database.pool()
    }

    pub fn database_module(&self) -> std::sync::Arc<sdkwork_database_spi::DefaultDatabaseModule> {
        self.database.module()
    }
}
