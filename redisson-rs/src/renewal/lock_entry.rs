// ============================================================
// LockEntry — 对应 Java org.redisson.renewal.LockEntry
// ============================================================

/// 单把锁的续约信息，对应 Java LockEntry。
/// Java LockEntry 持有 threadsQueue / threadId2counter / threadId2lockName；
/// Rust 中 owner_id 已编码 clientId:taskId，等价于 Java lockName（nodeId:threadId）。
/// Rust 的 cancelExpirationRenewal 仅在完全释放锁时调用一次（Java 每次 unlock 都调用），
/// 所以无需 threadId2counter，只需持有 owner_id 即可。
pub struct LockEntry {
    /// 锁持有者标识，对应 Java threadId2lockName 里的 lockName（格式 clientId:taskId）
    pub(crate) owner_id: String,
}

impl LockEntry {
    pub fn new(owner_id: String) -> Self {
        Self { owner_id }
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }
}
