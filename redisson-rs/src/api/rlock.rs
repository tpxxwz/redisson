use anyhow::Result;
use std::future::Future;
use tokio_util::sync::CancellationToken;

// ============================================================
// RLock — 对应 Java org.redisson.api.RLock（接口）
// ============================================================

/// 可重入分布式锁接口，对应 Java RLock extends Lock, RExpirable, RLockAsync。
/// 实现类：RedissonLock
pub trait RLock {
    fn lock(&self) -> impl Future<Output = Result<()>> + Send;
    fn lock_with_lease(&self, lease_ms: u64) -> impl Future<Output = Result<()>> + Send;
    fn lock_interruptibly(
        &self,
        cancel: CancellationToken,
    ) -> impl Future<Output = Result<()>> + Send;
    fn try_lock(&self) -> impl Future<Output = Result<bool>> + Send;
    fn try_lock_with_timeout(
        &self,
        wait_ms: u64,
        lease_ms: Option<u64>,
    ) -> impl Future<Output = Result<bool>> + Send;
    fn unlock(&self) -> impl Future<Output = Result<()>> + Send;
    fn force_unlock(&self) -> impl Future<Output = Result<bool>> + Send;
    fn is_locked(&self) -> impl Future<Output = Result<bool>> + Send;
    fn is_held_by_current_thread(&self) -> impl Future<Output = Result<bool>> + Send;
    fn get_hold_count(&self) -> impl Future<Output = Result<i64>> + Send;
    fn remain_time_to_live(&self) -> impl Future<Output = Result<i64>> + Send;
}
