use crate::ext::RedisKey;
use crate::redisson_lock::RedissonLock;

// ============================================================
// RedissonClient — 对应 Java org.redisson.api.RedissonClient（接口）
// ============================================================

/// Redisson 客户端接口，对应 Java org.redisson.api.RedissonClient。
/// 实现类：Redisson
pub trait RedissonClient {
    /// 对应 Java RedissonClient.getLock(String name)
    fn get_lock<K: RedisKey>(&self, name: K) -> RedissonLock;
}
