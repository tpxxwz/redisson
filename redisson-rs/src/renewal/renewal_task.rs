// ============================================================
// RenewalTask — 对应 Java org.redisson.renewal.RenewalTask（抽象类）
// ============================================================

/// 续约任务接口，对应 Java abstract class RenewalTask implements TimerTask。
/// Java 用抽象类 + 模板方法（execute() 由子类实现）；
/// Rust 用 trait，LockTask / ReadLockTask / FastMultilockTask 各自实现。
pub trait RenewalTask: Send + Sync {
    /// 注册锁续约，对应 Java RenewalTask.add(rawName, lockName, threadId, entry)
    fn add(&self, name: String, lock_name: String);

    /// 注销锁续约，对应 Java RenewalTask.cancelExpirationRenewal(name, threadId)
    fn cancel_expiration_renewal(&self, name: &str);

    /// 停止续约循环
    fn shutdown(&self);
}
