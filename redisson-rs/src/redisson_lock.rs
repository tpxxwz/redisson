use crate::api::rlock::RLock;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::command::redis_command::commands;
use crate::ext::RedisKey;
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::redisson_lock_entry::RedissonLockEntry;
use crate::redisson_base_lock::{prefix_name, RedissonBaseLock};
use anyhow::Result;
use fred::prelude::Value;
use fred::types::FromValue;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ============================================================
// Lua Scripts — 对应 Java RedissonLock 中的脚本常量
// ============================================================

/// 对应 Java RedissonLock.tryLockInnerAsync() 中的 LOCK 脚本
const LOCK_SCRIPT: &str = concat![
    "if ((redis.call('exists', KEYS[1]) == 0) ",
    "or (redis.call('hexists', KEYS[1], ARGV[2]) == 1)) then ",
    "redis.call('hincrby', KEYS[1], ARGV[2], 1); ",
    "redis.call('pexpire', KEYS[1], ARGV[1]); ",
    "return nil; ",
    "end; ",
    "return redis.call('pttl', KEYS[1]);",
];

/// 对应 Java RedissonLock.unlockInnerAsync() 中的 UNLOCK 脚本
const UNLOCK_SCRIPT: &str = concat![
    "local val = redis.call('get', KEYS[3]); ",
    "if val ~= false then ",
    "return tonumber(val);",
    "end; ",
    "if (redis.call('hexists', KEYS[1], ARGV[3]) == 0) then ",
    "return nil;",
    "end; ",
    "local counter = redis.call('hincrby', KEYS[1], ARGV[3], -1); ",
    "if (counter > 0) then ",
    "redis.call('pexpire', KEYS[1], ARGV[2]); ",
    "redis.call('set', KEYS[3], 0, 'px', ARGV[5]); ",
    "return 0; ",
    "else ",
    "redis.call('del', KEYS[1]); ",
    "redis.call(ARGV[4], KEYS[2], ARGV[1]); ",
    "redis.call('set', KEYS[3], 1, 'px', ARGV[5]); ",
    "return 1; ",
    "end; ",
];

/// 对应 Java RedissonBaseLock.getHoldCountAsync() 中的 HGET 脚本
const GET_HOLD_COUNT_SCRIPT: &str = concat![
    "if redis.call('hexists', KEYS[1], ARGV[1]) == 1 then ",
    "return redis.call('hget', KEYS[1], ARGV[1]) ",
    "else ",
    "return 0 ",
    "end",
];

/// 对应 Java RedissonLock.forceUnlockAsync() 中的 DEL 脚本
const FORCE_UNLOCK_SCRIPT: &str = concat![
    "if (redis.call('del', KEYS[1]) == 1) then ",
    "redis.call(ARGV[2], KEYS[2], ARGV[1]); ",
    "return 1 ",
    "else ",
    "return 0 ",
    "end",
];

// ============================================================
// 对应 Java Thread.currentThread().getId()
// 在每个公开 API 入口调用一次，捕获后传入内部方法。
// ============================================================

/// 对应 Java Thread.currentThread().getId()：当前执行单元的唯一标识。
/// tokio::spawn 场景用 task id；block_on 场景回退到线程 id（block_on 期间线程独占）。
fn current_thread_id() -> String {
    tokio::task::try_id()
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("{:?}", std::thread::current().id()))
}

// ============================================================
// RedissonLock — 对应 Java org.redisson.RedissonLock
// ============================================================

/// 可重入分布式锁，对应 Java RedissonLock extends RedissonBaseLock。
/// Rust 用组合代替继承：base 对应 RedissonBaseLock 字段；
/// 通过 Deref<Target = RedissonBaseLock<CE>> 使 self.xxx 直接访问基类成员。
///
/// CE 为命令执行器具体类型（静态分发）。
pub struct RedissonLock<CE: CommandAsyncExecutor> {
    pub(crate) base: RedissonBaseLock<CE>,
    /// 对应 Java RedissonLock.internalLockLeaseTime，AtomicU64 支持多 task 共享时的可变更新
    pub(crate) internal_lock_lease_time: Arc<AtomicU64>,
    /// 对应 Java RedissonLock.pubSub（LockPubSub 订阅管理）
    pub(crate) pub_sub: LockPubSub,
}

impl<CE: CommandAsyncExecutor> Deref for RedissonLock<CE> {
    type Target = RedissonBaseLock<CE>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<CE: CommandAsyncExecutor> RedissonLock<CE> {
    /// 对应 Java RedissonLock(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<CE>, key: impl RedisKey) -> Self {
        let base = RedissonBaseLock::new(command_executor, key);
        let service_manager = command_executor.service_manager();
        Self {
            internal_lock_lease_time: Arc::new(AtomicU64::new(
                service_manager.renewal_scheduler().internal_lock_lease_time(),
            )),
            pub_sub: LockPubSub::new(command_executor.connection_manager().subscribe_service().clone()),
            base,
        }
    }

    // ── 内部实现 ──────────────────────────────────────────────────────────

    /// 对应 Java RedissonLock.getChannelName()
    fn get_channel_name(&self) -> String {
        prefix_name("redisson_lock__channel", &self.name)
    }

    /// 对应 Java RedissonLock.cancelExpirationRenewal override：
    /// 调用基类后，若锁完全释放则重置 internalLockLeaseTime 回 watchdog timeout
    pub(crate) fn cancel_expiration_renewal(&self, thread_id: Option<&str>, unlock_result: Option<bool>) {
        self.base.cancel_expiration_renewal(thread_id, unlock_result);
        if unlock_result.is_none() || unlock_result == Some(true) {
            let watchdog_timeout = self
                .command_executor
                .service_manager()
                .renewal_scheduler()
                .internal_lock_lease_time();
            self.internal_lock_lease_time.store(watchdog_timeout, Ordering::Release);
        }
    }

    /// 对应 Java RedissonLock.tryAcquireAsync()：
    /// 执行 LOCK 脚本，返回 None 表示加锁成功，返回 Some(ttl) 表示锁被他人持有。
    async fn try_acquire(&self, lease_time: u64, thread_id: &str) -> Result<Option<u64>> {
        let lock_name = self.lock_name(thread_id);
        self.command_executor
            .eval_write_async::<Option<u64>>(
                LOCK_SCRIPT,
                vec![self.name.as_str()],
                vec![&lease_time.to_string(), lock_name.as_str()],
            )
            .await
    }

    /// 对应 Java RedissonLock.subscribe(long threadId)
    async fn subscribe(&self, _thread_id: &str) -> Result<Arc<RedissonLockEntry>> {
        let subscribe_timeout_ms = self
            .command_executor
            .service_manager()
            .subscribe_timeout_ms();
        let channel_name = self.get_channel_name();
        let fut = self.pub_sub.subscribe(&self.entry_name, &channel_name);
        if subscribe_timeout_ms == 0 {
            fut.await
        } else {
            tokio::time::timeout(Duration::from_millis(subscribe_timeout_ms), fut)
                .await
                .map_err(|_| {
                    anyhow::anyhow!("Subscribe timeout after {}ms", subscribe_timeout_ms)
                })?
        }
    }

    /// 对应 Java RedissonLock 私有方法 lock(long leaseTime, TimeUnit unit, boolean interruptibly)
    async fn lock_inner(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        cancel: Option<&CancellationToken>,
        thread_id: &str,
    ) -> Result<()> {
        if self.try_acquire(lease_time, thread_id).await?.is_none() {
            if use_watchdog {
                self.schedule_expiration_renewal(thread_id);
            } else {
                self.internal_lock_lease_time.store(lease_time, Ordering::Release);
            }
            return Ok(());
        }

        let entry = self.subscribe(thread_id).await?;
        let result = self
            .lock_wait_loop(lease_time, use_watchdog, cancel, &entry, thread_id)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    /// 对应 Java RedissonLock.lock() 内部循环
    async fn lock_wait_loop(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        cancel: Option<&CancellationToken>,
        entry: &Arc<RedissonLockEntry>,
        thread_id: &str,
    ) -> Result<()> {
        loop {
            let Some(ttl) = self.try_acquire(lease_time, thread_id).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(thread_id);
                } else {
                    self.internal_lock_lease_time.store(lease_time, Ordering::Release);
                }
                return Ok(());
            };

            let wait_dur = Duration::from_millis(ttl + 100);
            if let Some(c) = cancel {
                tokio::select! {
                    _ = c.cancelled() => anyhow::bail!("lock_interruptibly: cancelled"),
                    _ = entry.wait() => {}
                    _ = tokio::time::sleep(wait_dur) => {}
                }
            } else {
                tokio::select! {
                    _ = entry.wait() => {}
                    _ = tokio::time::sleep(wait_dur) => {}
                }
            }
        }
    }

    /// 对应 Java RedissonLock.tryLock(long waitTime, ...) 内部循环
    async fn try_lock_wait_loop(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        deadline: tokio::time::Instant,
        entry: &Arc<RedissonLockEntry>,
        thread_id: &str,
    ) -> Result<bool> {
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }

            let Some(ttl) = self.try_acquire(lease_time, thread_id).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(thread_id);
                } else {
                    self.internal_lock_lease_time.store(lease_time, Ordering::Release);
                }
                return Ok(true);
            };

            let wait_dur = remaining.min(Duration::from_millis(ttl + 100));
            tokio::select! {
                _ = tokio::time::sleep(remaining) => return Ok(false),
                _ = entry.wait() => {}
                _ = tokio::time::sleep(wait_dur) => {}
            }
        }
    }
}

// ============================================================
// impl RLock for RedissonLock — 对应 Java RedissonLock implements RLock
// ============================================================

impl<CE: CommandAsyncExecutor> RLock for RedissonLock<CE> {
    /// 对应 Java RedissonBaseLock.lock()
    async fn lock(&self) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(self.internal_lock_lease_time.load(Ordering::Acquire), true, None, &thread_id)
            .await
    }

    /// 对应 Java RedissonBaseLock.lock(long leaseTime, TimeUnit unit)
    async fn lock_with_lease(&self, lease_ms: u64) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(lease_ms, false, None, &thread_id).await
    }

    /// 对应 Java RedissonBaseLock.lockInterruptibly()
    async fn lock_interruptibly(&self, cancel: CancellationToken) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(self.internal_lock_lease_time.load(Ordering::Acquire), true, Some(&cancel), &thread_id)
            .await
    }

    /// 对应 Java RedissonBaseLock.tryLockAsync(long threadId)（非阻塞，立即返回）
    async fn try_lock(&self) -> Result<bool> {
        let thread_id = current_thread_id();
        if self
            .try_acquire(self.internal_lock_lease_time.load(Ordering::Acquire), &thread_id)
            .await?
            .is_none()
        {
            self.schedule_expiration_renewal(&self.lock_name(&thread_id));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 对应 Java RedissonBaseLock.tryLock(long waitTime, long leaseTime, TimeUnit unit)
    async fn try_lock_with_timeout(&self, wait_ms: u64, lease_ms: Option<u64>) -> Result<bool> {
        let thread_id = current_thread_id();
        let lease_time = lease_ms.unwrap_or_else(|| self.internal_lock_lease_time.load(Ordering::Acquire));
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        if self.try_acquire(lease_time, &thread_id).await?.is_none() {
            if lease_ms.is_none() {
                self.schedule_expiration_renewal(&thread_id);
            } else {
                self.internal_lock_lease_time.store(lease_time, Ordering::Release);
            }
            return Ok(true);
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }

        let entry = self.subscribe(&thread_id).await?;
        let result = self
            .try_lock_wait_loop(lease_time, lease_ms.is_none(), deadline, &entry, &thread_id)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    /// 对应 Java RedissonBaseLock.unlock()
    async fn unlock(&self) -> Result<()> {
        let thread_id = current_thread_id();
        let lock_name = self.lock_name(&thread_id);
        let request_id = uuid::Uuid::new_v4().to_string();
        let latch_key = self.get_unlock_latch_name(&request_id);
        let lease_time_str = self.internal_lock_lease_time.load(Ordering::Acquire).to_string();
        let latch_timeout = self
            .command_executor
            .service_manager()
            .calc_unlock_latch_timeout_ms()
            .to_string();
        let publish_cmd = self
            .command_executor
            .connection_manager()
            .subscribe_service()
            .publish_command();

        let result: Option<i64> = self
            .command_executor
            .eval_write_async(
                UNLOCK_SCRIPT,
                vec![
                    self.name.as_str(),
                    self.get_channel_name().as_str(),
                    latch_key.as_str(),
                ],
                vec![
                    "0",
                    lease_time_str.as_str(),
                    lock_name.as_str(),
                    publish_cmd,
                    latch_timeout.as_str(),
                ],
            )
            .await?;

        {
            let executor = self.command_executor.clone();
            let latch_key_owned = latch_key.clone();
            tokio::spawn(async move {
                let _ = executor.write_async::<i64>(&latch_key_owned, commands::DEL, vec![]).await;
            });
        }

        match result {
            None => {
                self.cancel_expiration_renewal(Some(&thread_id), None);
                anyhow::bail!("attempt to unlock lock, not locked by current thread")
            }
            Some(0) => Ok(()),
            Some(v) => {
                self.cancel_expiration_renewal(Some(&thread_id), Some(v == 1));
                Ok(())
            }
        }
    }

    /// 对应 Java RedissonBaseLock.forceUnlock()
    async fn force_unlock(&self) -> Result<bool> {
        self.cancel_expiration_renewal(None, None);
        let publish_cmd = self
            .command_executor
            .connection_manager()
            .subscribe_service()
            .publish_command();
        let raw: Value = self
            .command_executor
            .eval_write_async(
                FORCE_UNLOCK_SCRIPT,
                vec![self.name.as_str(), self.get_channel_name().as_str()],
                vec!["0", publish_cmd],
            )
            .await?;
        Ok(i64::from_value(raw)? == 1)
    }

    /// 对应 Java RedissonBaseLock.isLocked()
    async fn is_locked(&self) -> Result<bool> {
        self.command_executor
            .read_async(&self.name, commands::EXISTS, vec![])
            .await
    }

    /// 对应 Java RedissonBaseLock.isHeldByCurrentThread()
    async fn is_held_by_current_thread(&self) -> Result<bool> {
        let thread_id = current_thread_id();
        let lock_name = self.lock_name(&thread_id);
        self.command_executor
            .read_async(&self.name, commands::HEXISTS, vec![lock_name.as_str()])
            .await
    }

    /// 对应 Java RedissonBaseLock.getHoldCount()
    async fn get_hold_count(&self) -> Result<i64> {
        let thread_id = current_thread_id();
        let lock_name = self.lock_name(&thread_id);
        self.command_executor
            .eval_write_async(
                GET_HOLD_COUNT_SCRIPT,
                vec![self.name.as_str()],
                vec![lock_name.as_str()],
            )
            .await
    }

    /// 对应 Java RedissonExpirable.remainTimeToLive()
    async fn remain_time_to_live(&self) -> Result<i64> {
        self.command_executor
            .read_async(&self.name, commands::PTTL, vec![])
            .await
    }
}
