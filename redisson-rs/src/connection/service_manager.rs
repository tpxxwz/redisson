use crate::config::equal_jitter_delay::{DelayStrategy, EqualJitterDelay};
use crate::config::name_mapper::NameMapper;
use crate::config::RedissonConfig;
use crate::config::ServerMode;
use crate::renewal::renewal_scheduler_trait::RenewalScheduler;
use std::sync::{Arc, OnceLock};

// ============================================================
// ServiceManager — 对应 Java org.redisson.connection.ServiceManager
// ============================================================

pub struct ServiceManager {
    /// 节点唯一标识（UUID），对应 Java ServiceManager.id
    pub(crate) id: String,
    /// watchdog 续约调度器，对应 Java ServiceManager.renewalScheduler
    renewal_scheduler: OnceLock<Arc<dyn RenewalScheduler>>,
    /// 对应 Java ServiceManager.getNameMapper()
    pub(crate) name_mapper: Arc<dyn NameMapper>,
    /// 对应 Java ServiceManager.cfg (Config)
    cfg: Arc<RedissonConfig>,
    /// Pub/Sub 订阅建立超时（ms）
    pub(crate) subscribe_timeout_ms: u64,
    /// 对应 Java BaseConfig.timeout（命令响应超时，ms）
    pub(crate) command_timeout_ms: u64,
    /// 对应 Java BaseConfig.retryAttempts
    pub(crate) retry_attempts: u32,
    /// 对应 Java BaseConfig.retryDelay
    pub(crate) retry_delay: EqualJitterDelay,
}

impl ServiceManager {
    pub fn new(
        name_mapper: Arc<dyn NameMapper>,
        config: Arc<RedissonConfig>,
        subscribe_timeout_ms: u64,
        command_timeout_ms: u64,
        retry_attempts: u32,
        retry_delay: EqualJitterDelay,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            renewal_scheduler: OnceLock::new(),
            name_mapper,
            cfg: config,
            subscribe_timeout_ms,
            command_timeout_ms,
            retry_attempts,
            retry_delay,
        }
    }

    /// 对应 Java ServiceManager.register(LockRenewalScheduler)
    pub fn register(&self, scheduler: Arc<dyn RenewalScheduler>) {
        let _ = self.renewal_scheduler.set(scheduler);
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn renewal_scheduler(&self) -> &Arc<dyn RenewalScheduler> {
        self.renewal_scheduler
            .get()
            .expect("renewal scheduler not registered")
    }

    pub fn subscribe_timeout_ms(&self) -> u64 {
        self.subscribe_timeout_ms
    }

    /// 对应 Java ServiceManager.getCfg()
    pub fn cfg(&self) -> &RedissonConfig {
        &self.cfg
    }

    pub fn calc_unlock_latch_timeout_ms(&self) -> u64 {
        let delay = self.retry_delay.calc_delay(self.retry_attempts);
        let timeout = (self.command_timeout_ms + delay) * self.retry_attempts as u64;
        timeout.max(1)
    }

    /// 对应 Java Config.isClusterConfig()
    pub fn is_cluster_config(&self) -> bool {
        matches!(self.cfg.mode, ServerMode::Cluster { .. })
    }
}
