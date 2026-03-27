use super::lock_task::LockTask;
use super::renewal_task::RenewalTask;
use crate::command::command_async_executor::CommandAsyncExecutor;
use std::sync::{Arc, OnceLock};

// ============================================================
// LockRenewalScheduler — 对应 Java org.redisson.renewal.LockRenewalScheduler
// ============================================================

/// watchdog 续约调度器，持有各类型锁的续约任务。
/// 对应 Java LockRenewalScheduler，通过 ServiceManager.renewalScheduler 访问。
///
/// Java 用 AtomicReference<LockTask> 惰性初始化（compareAndSet(null, new LockTask(...))）；
/// Rust 用 OnceLock<LockTask>，语义等价。
/// executor 同样后置注入（set_executor），避免 LockRenewalScheduler → CommandAsyncService
/// → ServiceManager → LockRenewalScheduler 的循环依赖。
pub struct LockRenewalScheduler {
    /// 对应 Java LockRenewalScheduler.internalLockLeaseTime
    internal_lock_lease_time: u64,
    /// 对应 Java LockRenewalScheduler.executor (CommandAsyncExecutor)
    /// 构造后通过 set_executor 注入，避免循环依赖
    executor: OnceLock<Arc<dyn CommandAsyncExecutor>>,
    /// 对应 Java LockRenewalScheduler.reference (AtomicReference<LockTask>)
    reference: OnceLock<LockTask>,
    // 以下留给后续实现：
    // read_lock_reference: OnceLock<ReadLockTask>,      ← Java readLockReference
    // multilock_reference: OnceLock<FastMultilockTask>, ← Java multilockReference
}

impl LockRenewalScheduler {
    pub fn new(internal_lock_lease_time: u64) -> Self {
        Self {
            internal_lock_lease_time,
            executor: OnceLock::new(),
            reference: OnceLock::new(),
        }
    }

    /// 对应 Java LockRenewalScheduler(CommandAsyncExecutor executor) 构造参数；
    /// Rust 因循环依赖改为后置注入，在 CommandAsyncService 创建后调用一次
    pub fn set_executor(&self, executor: Arc<dyn CommandAsyncExecutor>) {
        let _ = self.executor.set(executor);
    }

    pub fn internal_lock_lease_time(&self) -> u64 {
        self.internal_lock_lease_time
    }

    /// 惰性获取或创建 LockTask，对应 Java 的 compareAndSet(null, new LockTask(...))
    fn task(&self) -> &LockTask {
        self.reference.get_or_init(|| {
            let executor = self
                .executor
                .get()
                .expect("LockRenewalScheduler: executor not set before use")
                .clone();
            LockTask::new(executor, self.internal_lock_lease_time)
        })
    }

    /// 注册标准锁续约，对应 Java LockRenewalScheduler.renewLock(name, threadId, lockName)
    pub fn renew_lock(&self, key: String, lock_name: String) {
        self.task().add(key, lock_name);
    }

    /// 取消标准锁续约，对应 Java LockRenewalScheduler.cancelLockRenewal(name, threadId)
    pub fn cancel_lock_renewal(&self, key: &str) {
        if let Some(task) = self.reference.get() {
            task.cancel_expiration_renewal(key);
        }
    }

    pub fn shutdown(&self) {
        if let Some(task) = self.reference.get() {
            task.shutdown();
        }
    }
}
