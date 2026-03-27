use super::lock_entry::LockEntry;
use super::renewal_task::RenewalTask;
use crate::command::command_async_executor::CommandAsyncExecutor;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use fred::types::FromValue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ============================================================
// LockTask — 对应 Java org.redisson.renewal.LockTask
// ============================================================

/// 批量续约脚本，对应 Java LockTask.buildChunk 的 Lua
/// KEYS[1..n] = lockKey1, lockKey2, ...
/// ARGV[1]    = internalLockLeaseTime (ms)
/// ARGV[2..n+1] = owner_id per key
/// 返回 table：1 = 续约成功，0 = 锁已不属于该 owner
const RENEW_SCRIPT: &str = concat![
    "local result = {} ",
    "for i = 1, #KEYS, 1 do ",
    "if (redis.call('hexists', KEYS[i], ARGV[i + 1]) == 1) then ",
    "redis.call('pexpire', KEYS[i], ARGV[1]); ",
    "table.insert(result, 1); ",
    "else ",
    "table.insert(result, 0); ",
    "end; ",
    "end; ",
    "return result;",
];

/// 标准锁续约任务，对应 Java LockTask extends RenewalTask。
/// 持有 name2entry（key → LockEntry）、running 标志和续约定时器逻辑。
pub struct LockTask {
    /// 对应 Java RenewalTask.internalLockLeaseTime
    internal_lock_lease_time: u64,
    /// 对应 Java RenewalTask.executor（CommandAsyncExecutor）
    executor: Arc<dyn CommandAsyncExecutor>,
    /// key → LockEntry，对应 Java RenewalTask.name2entry
    pub(crate) name2entry: Arc<DashMap<String, LockEntry>>,
    /// 续约循环运行标志，对应 Java RenewalTask.running
    running: Arc<AtomicBool>,
    shutdown_token: CancellationToken,
}

impl LockTask {
    pub fn new(executor: Arc<dyn CommandAsyncExecutor>, internal_lock_lease_time: u64) -> Self {
        Self {
            internal_lock_lease_time,
            executor,
            name2entry: Arc::new(DashMap::new()),
            running: Arc::new(AtomicBool::new(false)),
            shutdown_token: CancellationToken::new(),
        }
    }

    /// 批量续约，对应 Java LockTask.execute() → buildChunk → evalScript
    async fn execute(
        name2entry: &DashMap<String, LockEntry>,
        internal_lock_lease_time: u64,
        executor: &Arc<dyn CommandAsyncExecutor>,
    ) {
        if name2entry.is_empty() {
            return;
        }

        let snapshot: Vec<(String, String)> = name2entry
            .iter()
            .map(|e| (e.key().clone(), e.value().owner_id().to_string()))
            .collect();

        let ttl_str = internal_lock_lease_time.to_string();
        let keys: Vec<&str> = snapshot.iter().map(|(k, _)| k.as_str()).collect();
        let mut argv: Vec<&str> = Vec::with_capacity(snapshot.len() + 1);
        argv.push(ttl_str.as_str());
        argv.extend(snapshot.iter().map(|(_, o)| o.as_str()));

        match executor.eval_raw(RENEW_SCRIPT, keys, argv).await {
            Ok(raw) => {
                let results = Vec::<i64>::from_value(raw).unwrap_or_default();
                for (i, (key, owner_id)) in snapshot.iter().enumerate() {
                    if results.get(i).copied().unwrap_or(0) == 0 {
                        tracing::warn!(
                            "Lock {} no longer held by {}, removing from renewal",
                            key,
                            owner_id
                        );
                        name2entry.remove(key);
                    } else {
                        tracing::trace!("Lock {} renewed (batch {} locks)", key, snapshot.len());
                    }
                }
            }
            Err(e) => {
                // 续约失败不移除，等下一个 interval 重试（对应 Java schedule() on error）
                tracing::error!("Batch renewal failed for {} locks: {}", snapshot.len(), e);
            }
        }
    }

    /// 启动事件驱动续约循环，对应 Java RenewalTask.run(timeout) + schedule()
    fn spawn(
        name2entry: Arc<DashMap<String, LockEntry>>,
        executor: Arc<dyn CommandAsyncExecutor>,
        internal_lock_lease_time: u64,
        running: Arc<AtomicBool>,
        shutdown_token: CancellationToken,
    ) {
        tokio::spawn(async move {
            tracing::debug!(
                "LockTask started (ttl={}ms, interval={}ms)",
                internal_lock_lease_time,
                internal_lock_lease_time / 3
            );
            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        tracing::debug!("LockTask shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(internal_lock_lease_time / 3)) => {}
                }

                if !running.load(Ordering::Acquire) {
                    tracing::debug!("LockTask stopped (no active locks)");
                    break;
                }

                Self::execute(&name2entry, internal_lock_lease_time, &executor).await;

                if name2entry.is_empty() {
                    running.store(false, Ordering::Release);
                    tracing::debug!("LockTask stopped after execute (no active locks)");
                    break;
                }
            }
        });
    }

    /// CAS 启动续约循环，对应 Java RenewalTask.tryRun() + schedule()
    fn try_run(&self) {
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            Self::spawn(
                self.name2entry.clone(),
                self.executor.clone(),
                self.internal_lock_lease_time,
                self.running.clone(),
                self.shutdown_token.clone(),
            );
        }
    }
}

impl RenewalTask for LockTask {
    /// 对应 Java LockTask.add(rawName, lockName, threadId)：
    /// putIfAbsent — 首次注册时启动续约循环，重入时 watchdog 已在运行，无需额外操作
    fn add(&self, name: String, lock_name: String) {
        match self.name2entry.entry(name) {
            Entry::Occupied(_) => {
                // watchdog 已在运行，幂等，无需操作
            }
            Entry::Vacant(e) => {
                // 对应 Java: name2entry.putIfAbsent 返回 null → tryRun() + schedule()
                e.insert(LockEntry::new(lock_name));
                self.try_run();
            }
        }
    }

    /// 对应 Java RenewalTask.cancelExpirationRenewal(name, threadId)：
    /// Rust 只在锁完全释放时调用一次，直接从 map 移除
    fn cancel_expiration_renewal(&self, name: &str) {
        self.name2entry.remove(name);
        if self.name2entry.is_empty() {
            self.running.store(false, Ordering::Release);
        }
    }

    fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        self.shutdown_token.cancel();
    }
}
