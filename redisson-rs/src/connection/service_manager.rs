use crate::config::equal_jitter_delay::{DelayStrategy, EqualJitterDelay};
use crate::renewal::renewal_scheduler_trait::RenewalScheduler;
use std::sync::{Arc, OnceLock};

// ============================================================
// ServiceManager — 对应 Java org.redisson.connection.ServiceManager
// ============================================================

pub struct ServiceManager {
    /// 节点唯一标识（UUID），对应 Java ServiceManager.id
    pub(crate) id: String,
    /// watchdog 续约调度器，对应 Java ServiceManager.renewalScheduler
    /// 通过 register() 后置注入，对应 Java ServiceManager.register(LockRenewalScheduler)
    renewal_scheduler: OnceLock<Arc<dyn RenewalScheduler>>,
    /// Pub/Sub 订阅建立超时（ms），对应 Java ServiceManager.getSubscribeTimeout()
    pub(crate) subscribe_timeout_ms: u64,
    /// 对应 Java BaseConfig.timeout（命令响应超时，ms）
    pub(crate) command_timeout_ms: u64,
    /// 对应 Java BaseConfig.retryAttempts
    pub(crate) retry_attempts: u32,
    /// 对应 Java BaseConfig.retryDelay（DelayStrategy 实现类）
    pub(crate) retry_delay: EqualJitterDelay,
}

impl ServiceManager {
    pub fn new(
        id: String,
        subscribe_timeout_ms: u64,
        command_timeout_ms: u64,
        retry_attempts: u32,
        retry_delay: EqualJitterDelay,
    ) -> Self {
        Self {
            id,
            renewal_scheduler: OnceLock::new(),
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

    pub fn calc_unlock_latch_timeout_ms(&self) -> u64 {
        let delay = self.retry_delay.calc_delay(self.retry_attempts);
        let timeout = (self.command_timeout_ms + delay) * self.retry_attempts as u64;
        timeout.max(1)
    }
}
