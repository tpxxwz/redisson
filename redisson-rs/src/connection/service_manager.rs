use crate::config::equal_jitter_delay::{DelayStrategy, EqualJitterDelay};
use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
use std::sync::Arc;

// ============================================================
// ServiceManager — 对应 Java org.redisson.connection.ServiceManager
// ============================================================

/// 服务管理器，持有节点标识、watchdog 续约调度器等基础设施。
/// 对应 Java ServiceManager，通过 ConnectionManager.getServiceManager() 访问。
pub struct ServiceManager {
    /// 节点唯一标识（UUID），对应 Java ServiceManager.id
    pub(crate) id: String,
    /// watchdog 续约调度器，对应 Java ServiceManager.renewalScheduler
    pub(crate) renewal_scheduler: Arc<LockRenewalScheduler>,
    /// Pub/Sub 订阅建立超时（ms），对应 Java ServiceManager.getSubscribeTimeout()
    pub(crate) subscribe_timeout_ms: u64,
    /// 对应 Java BaseConfig.timeout（命令响应超时，ms）
    pub(crate) command_timeout_ms: u64,
    /// 对应 Java BaseConfig.retryAttempts
    pub(crate) retry_attempts: u32,
    /// 对应 Java BaseConfig.retryDelay（DelayStrategy 实现类）
    pub(crate) retry_delay: EqualJitterDelay,
    /// Redis publish 命令，standalone/sentinel 用 "publish"，cluster sharded 用 "spublish"
    /// 对应 Java getSubscribeService().getPublishCommand()
    pub(crate) publish_command: &'static str,
}

impl ServiceManager {
    pub fn new(
        id: String,
        renewal_scheduler: Arc<LockRenewalScheduler>,
        subscribe_timeout_ms: u64,
        command_timeout_ms: u64,
        retry_attempts: u32,
        retry_delay: EqualJitterDelay,
        publish_command: &'static str,
    ) -> Self {
        Self {
            id,
            renewal_scheduler,
            subscribe_timeout_ms,
            command_timeout_ms,
            retry_attempts,
            retry_delay,
            publish_command,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn renewal_scheduler(&self) -> &Arc<LockRenewalScheduler> {
        &self.renewal_scheduler
    }

    /// 对应 Java ServiceManager.getSubscribeTimeout()
    pub fn subscribe_timeout_ms(&self) -> u64 {
        self.subscribe_timeout_ms
    }

    /// 对应 Java unlockInnerAsync 中的 timeout 计算：
    /// (config.getTimeout() + config.getRetryDelay().calcDelay(retryAttempts)) * retryAttempts
    /// Math.max(timeout, 1)
    pub fn calc_unlock_latch_timeout_ms(&self) -> u64 {
        let delay = self.retry_delay.calc_delay(self.retry_attempts);
        let timeout = (self.command_timeout_ms + delay) * self.retry_attempts as u64;
        timeout.max(1)
    }

    /// 对应 Java getSubscribeService().getPublishCommand()
    pub fn publish_command(&self) -> &'static str {
        self.publish_command
    }
}
