use crate::api::redisson_client::RedissonClient;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::command::command_async_service::CommandAsyncService;
use crate::config::server_mode::ServerMode;
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use crate::config::{
    RedissonConfig, build_connection_config, build_fred_config, build_perf_config,
};
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use crate::ext::RedisKey;
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use crate::redisson_lock::RedissonLock;
use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
use anyhow::{Context, Result};
use fred::clients::SubscriberClient;
use fred::prelude::*;
use std::sync::Arc;

// ============================================================
// Redisson — 对应 Java org.redisson.Redisson
// ============================================================

/// Redisson 客户端入口，对应 Java public final class Redisson implements RedissonClient。
pub struct Redisson {
    /// 连接管理器，对应 Java private final ConnectionManager connectionManager
    connection_manager: Arc<dyn ConnectionManager>,
    /// 命令执行器，对应 Java private final CommandAsyncExecutor commandExecutor
    command_executor: Arc<dyn CommandAsyncExecutor>,
    /// 客户端配置，对应 Java Redisson.config
    config: RedissonConfig,
}

impl Redisson {
    pub fn connection_manager(&self) -> &Arc<dyn ConnectionManager> {
        &self.connection_manager
    }

    pub fn command_executor(&self) -> &Arc<dyn CommandAsyncExecutor> {
        &self.command_executor
    }

    pub fn config(&self) -> &RedissonConfig {
        &self.config
    }
}

impl<T: std::ops::Deref<Target = Redisson>> RedissonClient for T {
    fn get_lock<K: RedisKey>(&self, name: K) -> RedissonLock {
        RedissonLock::new(&self.command_executor, name)
    }
}

// ============================================================
// init — 对应 Java Redisson.create(config)
// ============================================================

pub async fn init(config: RedissonConfig) -> Result<Arc<Redisson>> {
    let fred_config = build_fred_config(&config)?;
    let perf = build_perf_config(&config);
    let connection = build_connection_config(&config);
    let policy = ReconnectPolicy::new_exponential(
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
        fred_config,
        Some(perf),
        Some(connection),
        Some(policy),
        config.pool_size,
    )
    .context("Failed to create Redis pool")?;

    pool.init().await.context("Failed to connect to Redis")?;
    tracing::info!("Redis connection pool established");

    let id = uuid::Uuid::new_v4().to_string();
    tracing::info!("Redisson id: {}", id);
    tracing::info!(
        "Redis lock watchdog timeout: {}s",
        config.lock_watchdog_timeout
    );

    // 创建 Pub/Sub 订阅客户端
    let subscriber = SubscriberClient::new(
        build_fred_config(&config)?,
        Some(build_perf_config(&config)),
        Some(build_connection_config(&config)),
        Some(ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        )),
    );
    subscriber
        .init()
        .await
        .context("Failed to connect subscriber")?;

    // manage_subscriptions() 在后台维护重连后的频道重订阅，必须 spawn 执行
    let subscriber_clone = subscriber.clone();
    tokio::spawn(async move {
        let _ = subscriber_clone.manage_subscriptions().await;
    });
    tracing::info!("Redis Pub/Sub subscriber established");

    let renewal_scheduler = Arc::new(LockRenewalScheduler::new(
        config.lock_watchdog_timeout * 1000,
    ));
    tracing::info!("Lock renewal scheduler initialized");

    let subscribe_service = Arc::new(PublishSubscribeService::new(subscriber.clone()));
    tracing::info!("PublishSubscribeService initialized");

    // 对应 Java ClusterConnectionManager 里的 checkShardingSupport 逻辑：
    // 非 cluster 模式始终用 publish；cluster 模式下按 ShardedSubscriptionMode 决定
    let publish_command = check_sharding_support(&pool, &config).await;
    let service_manager = Arc::new(ServiceManager::new(
        id,
        renewal_scheduler.clone(),
        config.subscription_timeout,
        config.command_timeout_ms,
        config.retry_attempts,
        config.retry_delay.clone(),
        publish_command,
    ));

    let connection_manager = Arc::new(FredConnectionManager::new(
        pool,
        subscribe_service.clone(),
        service_manager,
    ));

    let command_executor: Arc<dyn CommandAsyncExecutor> =
        Arc::new(CommandAsyncService::new(connection_manager.clone()));
    renewal_scheduler.set_executor(command_executor.clone());

    let redisson = Arc::new(Redisson {
        connection_manager: connection_manager as Arc<dyn ConnectionManager>,
        command_executor,
        config,
    });

    // pubsub_message_listener 是无限循环，必须 spawn 执行
    let lock_pub_sub = LockPubSub::new(subscribe_service);
    tokio::spawn(pubsub_message_listener(subscriber, lock_pub_sub));

    Ok(redisson)
}

// ============================================================
// Pub/Sub Listener
// ============================================================

/// 对应 Java ClusterConnectionManager.checkShardingSupport()：
/// - 非 cluster 模式：始终返回 "publish"
/// - cluster + Off：返回 "publish"
/// - cluster + On：返回 "spublish"
/// - cluster + Auto：向 Redis 发 PUBSUB SHARDNUMSUB 探测，成功返回 "spublish"，失败返回 "publish"
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
