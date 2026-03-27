use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::ext::RedisKey;
use std::sync::Arc;

// ============================================================
// RedissonBaseLock — 对应 Java org.redisson.RedissonBaseLock
// ============================================================

/// 对应 Java RedissonObject.prefixName(prefix, name)
pub(crate) fn prefix_name(prefix: &str, name: &str) -> String {
    if name.contains('{') {
        format!("{}:{}", prefix, name)
    } else {
        format!("{}:{{{}}}", prefix, name)
    }
}

/// 分布式锁公共基类，对应 Java abstract class RedissonBaseLock。
/// Rust 无继承，将 RedissonObject / RedissonExpirable / RedissonBaseLock 三层字段拍平：
///   - commandExecutor、name 来自 RedissonObject
///   - id、entryName、renewalScheduler 来自 RedissonBaseLock
///
/// CE 为命令执行器具体类型（静态分发），对应 Java CommandAsyncExecutor 接口。
pub struct RedissonBaseLock<CE: CommandAsyncExecutor> {
    // ── 来自 RedissonObject ───────────────────────────────────
    /// 对应 Java RedissonObject.commandExecutor
    pub(crate) command_executor: Arc<CE>,
    /// 对应 Java RedissonObject.name（经 hash-tag 包裹）
    pub(crate) name: String,

    // ── 来自 RedissonBaseLock ─────────────────────────────────
    /// 对应 Java RedissonBaseLock.id = ServiceManager.getId()（节点 UUID）
    pub(crate) id: String,
    /// 对应 Java RedissonBaseLock.entryName = id + ":" + name
    pub(crate) entry_name: String,
}

impl<CE: CommandAsyncExecutor> RedissonBaseLock<CE> {
    /// 对应 Java RedissonBaseLock(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<CE>, name: impl RedisKey) -> Self {
        let service_manager = command_executor.service_manager();
        let id = service_manager.id();
        let name = name.key();
        let entry_name = format!("{}:{}", id, name);

        Self {
            command_executor: command_executor.clone(),
            name,
            id: id.to_string(),
            entry_name,
        }
    }

    /// 对应 Java RedissonBaseLock.getLockName(threadId) = id + ":" + threadId
    /// thread_id 由调用方在公开 API 入口处捕获一次后传入，不在此处动态查询。
    pub(crate) fn lock_name(&self, thread_id: &str) -> String {
        format!("{}:{}", self.id, thread_id)
    }

    /// 对应 Java RedissonBaseLock.getUnlockLatchName(requestId)
    pub(crate) fn get_unlock_latch_name(&self, request_id: &str) -> String {
        format!("{}:{}", prefix_name("redisson_unlock_latch", &self.name), request_id)
    }

    /// 对应 Java RedissonBaseLock.scheduleExpirationRenewal(long threadId)
    pub(crate) fn schedule_expiration_renewal(&self, thread_id: &str) {
        self.command_executor
            .service_manager()
            .renewal_scheduler()
            .renew_lock(self.name.clone(), self.lock_name(thread_id), thread_id.to_string());
    }

    /// 对应 Java RedissonBaseLock.cancelExpirationRenewal(Long threadId, Boolean unlockResult)
    /// thread_id 为 None 时对应 Java 传 null（forceUnlock），直接移除整个 entry
    pub(crate) fn cancel_expiration_renewal(&self, thread_id: Option<&str>, _unlock_result: Option<bool>) {
        self.command_executor
            .service_manager()
            .renewal_scheduler()
            .cancel_lock_renewal(&self.name, thread_id);
    }
}
