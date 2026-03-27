use super::connection_manager::ConnectionManager;
use super::service_manager::ServiceManager;
use crate::config::server_mode::ServerMode;
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use crate::config::{RedissonConfig, build_connection_config, build_fred_config, build_perf_config};
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::{Context, Result};
use async_trait::async_trait;
use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, EventInterface, PubsubInterface};
use fred::prelude::{Pool, ReconnectPolicy};
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
    /// 对应 Java MasterSlaveConnectionManager(MasterSlaveServersConfig, Config, UUID id)：
    /// 内部完成连接池、订阅客户端、PublishSubscribeService、ServiceManager 的初始化。
    pub async fn init(config: &RedissonConfig, id: String) -> Result<Arc<Self>> {
        let reconnect_policy = ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        );

        tracing::info!(
            "Connecting to Redis [mode={}] with pool_size={}",
            config.mode.as_str(),
            config.pool_size
        );

        let pool = Pool::new(
            build_fred_config(config)?,
            Some(build_perf_config(config)),
            Some(build_connection_config(config)),
            Some(reconnect_policy.clone()),
            config.pool_size,
        )
        .context("Failed to create Redis pool")?;

        pool.init().await.context("Failed to connect to Redis")?;
        tracing::info!("Redis connection pool established");

        let subscriber = SubscriberClient::new(
            build_fred_config(config)?,
            Some(build_perf_config(config)),
            Some(build_connection_config(config)),
            Some(reconnect_policy),
        );
        subscriber.init().await.context("Failed to connect subscriber")?;

        let subscriber_clone = subscriber.clone();
        tokio::spawn(async move {
            let _ = subscriber_clone.manage_subscriptions().await;
        });
        tracing::info!("Redis Pub/Sub subscriber established");

        let publish_command = Self::check_sharding_support(&pool, config).await;
        let subscribe_service = Arc::new(PublishSubscribeService::new(
            subscriber.clone(),
            publish_command,
        ));
        tracing::info!("PublishSubscribeService initialized");

        let service_manager = Arc::new(ServiceManager::new(
            id,
            config.subscription_timeout,
            config.command_timeout_ms,
            config.retry_attempts,
            config.retry_delay.clone(),
        ));

        let lock_pub_sub = LockPubSub::new(subscribe_service.clone());
        tokio::spawn(Self::pubsub_message_listener(subscriber, lock_pub_sub));

        Ok(Arc::new(Self {
            pool,
            subscribe_service,
            service_manager,
        }))
    }

    pub fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    pub fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }

    /// 对应 Java ClusterConnectionManager.checkShardingSupport()
    async fn check_sharding_support(pool: &Pool, config: &RedissonConfig) -> &'static str {
        if !matches!(config.mode, ServerMode::Cluster) {
            return "publish";
        }
        match config.sharded_subscription_mode {
            ShardedSubscriptionMode::Off => "publish",
            ShardedSubscriptionMode::On => "spublish",
            ShardedSubscriptionMode::Auto => {
                let result: Result<fred::types::Value, _> = pool
                    .next()
                    .pubsub_shardnumsub::<fred::types::Value, _>(vec![""])
                    .await;
                if result.is_ok() {
                    tracing::info!("Sharded Pub/Sub supported, using SPUBLISH");
                    "spublish"
                } else {
                    tracing::info!("Sharded Pub/Sub not supported, using PUBLISH");
                    "publish"
                }
            }
        }
    }

    async fn pubsub_message_listener(subscriber: SubscriberClient, lock_pub_sub: LockPubSub) {
        let mut message_rx = subscriber.message_rx();
        tracing::info!("Pub/Sub message listener started");

        while let Ok(message) = message_rx.recv().await {
            let channel: &str = &message.channel;
            let msg_value: Option<i64> = message.value.convert().ok();
            lock_pub_sub.on_message(channel, msg_value);
        }
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

    async fn shutdown(&self) {
        self.service_manager.renewal_scheduler().shutdown();
        let _ = self.pool.quit().await;
    }
}
