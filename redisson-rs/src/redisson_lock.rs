use crate::api::rlock::RLock;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::ext::RedisKey;
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::redisson_lock_entry::RedissonLockEntry;
use crate::redisson_base_lock::RedissonBaseLock;
use anyhow::Result;
use fred::types::FromValue;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::task;
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
// RedissonLock — 对应 Java org.redisson.RedissonLock
// ============================================================

/// 可重入分布式锁，对应 Java RedissonLock extends RedissonBaseLock。
/// Rust 用组合代替继承：base 对应 RedissonBaseLock 字段；
/// 通过 Deref<Target = RedissonBaseLock> 使 self.xxx 直接访问基类成员。
pub struct RedissonLock {
    pub(crate) base: RedissonBaseLock,
    /// 对应 Java RedissonLock.internalLockLeaseTime
    pub(crate) internal_lock_lease_time: u64,
    /// 对应 Java RedissonLock.pubSub（LockPubSub 订阅管理）
    pub(crate) pub_sub: LockPubSub,
    /// 对应 Java getLockName(threadId) = id + ":" + threadId，构造时固定
    /// Rust 用 tokio task id 代替 Java 的 threadId，在 new() 时计算一次存储
    pub(crate) lock_name: String,
}

impl Deref for RedissonLock {
    type Target = RedissonBaseLock;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl RedissonLock {
    /// 对应 Java RedissonLock(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<dyn CommandAsyncExecutor>, key: impl RedisKey) -> Self {
        let base = RedissonBaseLock::new(command_executor, key);
        let sm = command_executor.service_manager();
        // 对应 Java getLockName(threadId) = id + ":" + threadId，构造时固定
        let task_part = task::try_id()
            .map(|id| format!("{:?}", id))
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let lock_name = format!("{}:{}", base.id, task_part);
        Self {
            internal_lock_lease_time: sm.renewal_scheduler().internal_lock_lease_time(),
            pub_sub: LockPubSub::new(command_executor.subscribe_service().clone()),
            lock_name,
            base,
        }
    }

    // ── 内部实现 ──────────────────────────────────────────────────────────

    /// 对应 Java RedissonLock.getChannelName()
    fn get_channel_name(&self) -> String {
        format!("redisson_lock__channel:{}", self.name)
    }

    /// 对应 Java RedissonLock.tryAcquireAsync()：
    /// 执行 LOCK 脚本，返回 None 表示加锁成功，返回 Some(ttl) 表示锁被他人持有。
    async fn try_acquire(&self, lease_time: u64) -> Result<Option<u64>> {
        let raw = self
            .command_executor
            .eval_raw(
                LOCK_SCRIPT,
                vec![self.name.as_str()],
                vec![&lease_time.to_string(), self.lock_name.as_str()],
            )
            .await?;
        Ok(Option::<u64>::from_value(raw)?)
    }

    /// 对应 Java RedissonLock.subscribe(long threadId)
    async fn subscribe(&self) -> Result<Arc<RedissonLockEntry>> {
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
    ) -> Result<()> {
        if self.try_acquire(lease_time).await?.is_none() {
            if use_watchdog {
                self.schedule_expiration_renewal(&self.lock_name);
            }
            return Ok(());
        }

        let entry = self.subscribe().await?;
        let result = self
            .lock_wait_loop(lease_time, use_watchdog, cancel, &entry)
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
    ) -> Result<()> {
        loop {
            let Some(ttl) = self.try_acquire(lease_time).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(&self.lock_name);
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
    ) -> Result<bool> {
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }

            let Some(ttl) = self.try_acquire(lease_time).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(&self.lock_name);
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

impl RLock for RedissonLock {
    /// 对应 Java RedissonBaseLock.lock()
    async fn lock(&self) -> Result<()> {
        self.lock_inner(self.internal_lock_lease_time, true, None)
            .await
    }

    /// 对应 Java RedissonBaseLock.lock(long leaseTime, TimeUnit unit)
    async fn lock_with_lease(&self, lease_ms: u64) -> Result<()> {
        self.lock_inner(lease_ms, false, None).await
    }

    /// 对应 Java RedissonBaseLock.lockInterruptibly()
    async fn lock_interruptibly(&self, cancel: CancellationToken) -> Result<()> {
        self.lock_inner(self.internal_lock_lease_time, true, Some(&cancel))
            .await
    }

    /// 对应 Java RedissonBaseLock.tryLockAsync(long threadId)（非阻塞，立即返回）
    async fn try_lock(&self) -> Result<bool> {
        if self
            .try_acquire(self.internal_lock_lease_time)
            .await?
            .is_none()
        {
            self.schedule_expiration_renewal(&self.lock_name);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 对应 Java RedissonBaseLock.tryLock(long waitTime, long leaseTime, TimeUnit unit)
    async fn try_lock_with_timeout(&self, wait_ms: u64, lease_ms: Option<u64>) -> Result<bool> {
        let lease_time = lease_ms.unwrap_or(self.internal_lock_lease_time);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        if self.try_acquire(lease_time).await?.is_none() {
            if lease_ms.is_none() {
                self.schedule_expiration_renewal(&self.lock_name);
            }
            return Ok(true);
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }

        let entry = self.subscribe().await?;
        let result = self
            .try_lock_wait_loop(lease_time, lease_ms.is_none(), deadline, &entry)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    /// 对应 Java RedissonBaseLock.unlock()
    async fn unlock(&self) -> Result<()> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let latch_key = format!("redisson_unlock_latch:{}:{}", self.name, request_id);
        let lease_time_str = self.internal_lock_lease_time.to_string();
        let latch_timeout = self.command_executor.calc_unlock_latch_timeout_ms().to_string();
        let publish_cmd = self.command_executor.publish_command();

        let raw = self
            .command_executor
            .eval_raw(
                UNLOCK_SCRIPT,
                vec![
                    self.name.as_str(),
                    self.get_channel_name().as_str(),
                    latch_key.as_str(),
                ],
                vec![
                    "0",
                    lease_time_str.as_str(),
                    self.lock_name.as_str(),
                    publish_cmd,
                    latch_timeout.as_str(),
                ],
            )
            .await?;
        let result = Option::<i64>::from_value(raw)?;

        {
            let executor = self.command_executor.clone();
            let latch_key_owned = latch_key.clone();
            tokio::spawn(async move {
                let _ = executor.del(&latch_key_owned).await;
            });
        }

        match result {
            None => {
                self.cancel_expiration_renewal();
                anyhow::bail!("attempt to unlock lock, not locked by current thread")
            }
            Some(0) => Ok(()),
            Some(_) => {
                self.cancel_expiration_renewal();
                Ok(())
            }
        }
    }

    /// 对应 Java RedissonBaseLock.forceUnlock()
    async fn force_unlock(&self) -> Result<bool> {
        self.cancel_expiration_renewal();
        let publish_cmd = self.command_executor.publish_command();
        let raw = self
            .command_executor
            .eval_raw(
                FORCE_UNLOCK_SCRIPT,
                vec![self.name.as_str(), self.get_channel_name().as_str()],
                vec!["0", publish_cmd],
            )
            .await?;
        Ok(i64::from_value(raw)? == 1)
    }

    /// 对应 Java RedissonBaseLock.isLocked()
    async fn is_locked(&self) -> Result<bool> {
        self.command_executor.exists(&self.name).await
    }

    /// 对应 Java RedissonBaseLock.isHeldByCurrentThread()
    async fn is_held_by_current_thread(&self) -> Result<bool> {
        self.command_executor
            .hexists(&self.name, &self.lock_name)
            .await
    }

    /// 对应 Java RedissonBaseLock.getHoldCount()
    async fn get_hold_count(&self) -> Result<i64> {
        let raw = self
            .command_executor
            .eval_raw(
                GET_HOLD_COUNT_SCRIPT,
                vec![self.name.as_str()],
                vec![self.lock_name.as_str()],
            )
            .await?;
        Ok(i64::from_value(raw)?)
    }

    /// 对应 Java RedissonExpirable.remainTimeToLive()
    async fn remain_time_to_live(&self) -> Result<i64> {
        self.command_executor.pttl(&self.name).await
    }
}
