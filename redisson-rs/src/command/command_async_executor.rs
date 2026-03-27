use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::Result;
use async_trait::async_trait;
use fred::types::Value;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor（接口）
// ============================================================

/// 命令执行器接口，对应 Java org.redisson.command.CommandAsyncExecutor。
/// 实现类：CommandAsyncService
///
/// Java eval 方法是泛型的，Rust dyn trait 不支持泛型方法，
/// 故改为 eval_raw 返回 fred::types::Value，调用方按需 convert。
#[async_trait]
pub trait CommandAsyncExecutor: Send + Sync {
    /// 对应 Java CommandAsyncExecutor.getConnectionManager()
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    /// 对应 Java CommandAsyncExecutor.getServiceManager()
    fn service_manager(&self) -> &Arc<ServiceManager>;
    /// 对应 Java CommandAsyncExecutor.getSubscribeService()（via ConnectionManager）
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService>;
    /// 对应 Java getSubscribeService().getPublishCommand()
    fn publish_command(&self) -> &'static str;
    /// 对应 Java unlockInnerAsync 中的 latch timeout 计算
    fn calc_unlock_latch_timeout_ms(&self) -> u64;

    /// 执行 Lua 脚本，返回原始 Value，调用方负责 convert。
    /// 对应 Java evalWriteAsync / evalReadAsync（泛型 R 由调用方处理）
    async fn eval_raw(&self, script: &str, keys: Vec<&str>, args: Vec<&str>) -> Result<Value>;
    /// 对应 Java writeAsync(key, EXISTS, key)
    async fn exists(&self, key: &str) -> Result<bool>;
    /// 对应 Java writeAsync(key, HEXISTS, key, field)
    async fn hexists(&self, key: &str, field: &str) -> Result<bool>;
    /// 对应 Java readAsync(key, PTTL, key)
    async fn pttl(&self, key: &str) -> Result<i64>;
    /// 对应 Java CommandAsyncExecutor.writeAsync(key, DEL, key)
    async fn del(&self, key: &str) -> Result<()>;
    /// GET key → Option<String>
    async fn get_str(&self, key: &str) -> Result<Option<String>>;
    /// SET key value [EX seconds]
    async fn set_str(&self, key: &str, value: &str, expire_secs: Option<i64>) -> Result<()>;
}
