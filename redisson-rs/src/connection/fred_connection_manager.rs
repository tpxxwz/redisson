use super::connection_manager::ConnectionManager;
use super::service_manager::ServiceManager;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use async_trait::async_trait;
use fred::interfaces::ClientLike;
use fred::prelude::Pool;
use std::sync::Arc;

// ============================================================
// FredConnectionManager
// ============================================================

/// 基于 fred 客户端的连接管理器。
/// Java 中针对不同模式有 MasterSlaveConnectionManager / ClusterConnectionManager /
/// SentinelConnectionManager 等多个实现类；Rust 这里 fred Pool 统一支持
/// standalone / cluster / sentinel，无需拆分，故统一命名为 FredConnectionManager。
pub struct FredConnectionManager {
    /// Redis 命令连接池（fred Pool），支持 standalone / cluster / sentinel
    pub(crate) pool: Pool,
    /// Pub/Sub 订阅服务，对应 Java subscribeService
    pub(crate) subscribe_service: Arc<PublishSubscribeService>,
    /// 服务管理器，对应 Java serviceManager
    pub(crate) service_manager: Arc<ServiceManager>,
}

impl FredConnectionManager {
    pub fn new(
        pool: Pool,
        subscribe_service: Arc<PublishSubscribeService>,
        service_manager: Arc<ServiceManager>,
    ) -> Self {
        Self {
            pool,
            subscribe_service,
            service_manager,
        }
    }

    pub fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    pub fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }
}

#[async_trait]
impl ConnectionManager for FredConnectionManager {
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }

    /// 对应 Java ConnectionManager.shutdown()
    async fn shutdown(&self) {
        self.service_manager.renewal_scheduler().shutdown();
        let _ = self.pool.quit().await;
    }
}
