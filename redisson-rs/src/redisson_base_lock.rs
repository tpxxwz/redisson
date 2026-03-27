use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::ext::RedisKey;
use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
use std::sync::Arc;

// ============================================================
// RedissonBaseLock — 对应 Java org.redisson.RedissonBaseLock
// ============================================================

/// 对应 Java RedissonObject.prefixName() + cluster hash-tag 包裹逻辑：
/// 若 key 不含 '{' 则包裹为 {key}，保证 cluster 模式下同一 slot。
pub(crate) fn ensure_hash_tag(name: &str) -> String {
    if name.contains('{') {
        name.to_string()
    } else {
        format!("{{{}}}", name)
    }
}

/// 分布式锁公共基类，对应 Java abstract class RedissonBaseLock。
/// Rust 无继承，将 RedissonObject / RedissonExpirable / RedissonBaseLock 三层字段拍平：
///   - commandExecutor、name 来自 RedissonObject
///   - id、entryName、renewalScheduler 来自 RedissonBaseLock
pub struct RedissonBaseLock {
    // ── 来自 RedissonObject ───────────────────────────────────
    /// 对应 Java RedissonObject.commandExecutor
    pub(crate) command_executor: Arc<dyn CommandAsyncExecutor>,
    /// 对应 Java RedissonObject.name（经 hash-tag 包裹）
    pub(crate) name: String,

    // ── 来自 RedissonBaseLock ─────────────────────────────────
    /// 对应 Java RedissonBaseLock.id = ServiceManager.getId()（节点 UUID）
    pub(crate) id: String,
    /// 对应 Java RedissonBaseLock.entryName = id + ":" + name
    pub(crate) entry_name: String,
    /// 对应 Java RedissonBaseLock.renewalScheduler
    pub(crate) renewal_scheduler: Arc<LockRenewalScheduler>,
}

impl RedissonBaseLock {
    /// 对应 Java RedissonBaseLock(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<dyn CommandAsyncExecutor>, name: impl RedisKey) -> Self {
        let sm = command_executor.service_manager();
        let id = sm.id();
        let name = ensure_hash_tag(&name.key());
        let entry_name = format!("{}:{}", id, name);

        Self {
            command_executor: command_executor.clone(),
            name,
            id: id.to_string(),
            entry_name,
            renewal_scheduler: sm.renewal_scheduler().clone(),
        }
    }

    /// 对应 Java RedissonBaseLock.scheduleExpirationRenewal(long threadId)
    /// lock_name 对应 Java getLockName(threadId)，由调用方（RedissonLock）传入
    pub(crate) fn schedule_expiration_renewal(&self, lock_name: &str) {
        self.renewal_scheduler
            .renew_lock(self.name.clone(), lock_name.to_string());
    }

    /// 对应 Java RedissonBaseLock.cancelExpirationRenewal(Long threadId, Boolean unlockResult)
    pub(crate) fn cancel_expiration_renewal(&self) {
        self.renewal_scheduler.cancel_lock_renewal(&self.name);
    }
}
