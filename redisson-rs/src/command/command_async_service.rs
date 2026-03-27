use super::command_async_executor::CommandAsyncExecutor;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::Result;
use async_trait::async_trait;
use fred::interfaces::{HashesInterface, KeysInterface, LuaInterface};
use fred::prelude::Expiration;
use fred::types::{FromValue, Value};
use std::sync::Arc;

// ============================================================
// CommandAsyncService — 对应 Java org.redisson.command.CommandAsyncService
// ============================================================

/// 命令执行器，持有 ConnectionManager 并负责向 Redis 发送命令。
/// 对应 Java CommandAsyncService（CommandAsyncExecutor 接口的主要实现）。
/// 所有 Redis 操作都经由此类路由，而非直接持有 Pool 引用。
pub struct CommandAsyncService {
    connection_manager: Arc<FredConnectionManager>,
}

impl CommandAsyncService {
    pub fn new(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self { connection_manager }
    }

    /// 执行 Lua 脚本，泛型版本供内部或具体类型场景使用
    pub async fn eval<R>(&self, script: &str, keys: Vec<&str>, args: Vec<&str>) -> Result<R>
    where
        R: FromValue + Send,
    {
        Ok(self
            .connection_manager
            .pool
            .eval(script, keys, args)
            .await?)
    }

}

#[async_trait]
impl CommandAsyncExecutor for CommandAsyncService {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.connection_manager.clone()
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        self.connection_manager.service_manager()
    }

    fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        self.connection_manager.subscribe_service()
    }

    fn publish_command(&self) -> &'static str {
        self.connection_manager.service_manager().publish_command()
    }

    fn calc_unlock_latch_timeout_ms(&self) -> u64 {
        self.connection_manager
            .service_manager()
            .calc_unlock_latch_timeout_ms()
    }

    async fn eval_raw(&self, script: &str, keys: Vec<&str>, args: Vec<&str>) -> Result<Value> {
        Ok(self
            .connection_manager
            .pool
            .eval(script, keys, args)
            .await?)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.connection_manager.pool.exists(key).await?)
    }

    async fn hexists(&self, key: &str, field: &str) -> Result<bool> {
        Ok(self.connection_manager.pool.hexists(key, field).await?)
    }

    async fn pttl(&self, key: &str) -> Result<i64> {
        Ok(self.connection_manager.pool.pttl(key).await?)
    }

    async fn del(&self, key: &str) -> Result<()> {
        let _: i64 = self.connection_manager.pool.del(key).await?;
        Ok(())
    }

    async fn get_str(&self, key: &str) -> Result<Option<String>> {
        Ok(self.connection_manager.pool.get(key).await?)
    }

    async fn set_str(&self, key: &str, value: &str, expire_secs: Option<i64>) -> Result<()> {
        let expiry = expire_secs.filter(|&s| s > 0).map(Expiration::EX);
        let _: () = self
            .connection_manager
            .pool
            .set(key, value, expiry, None, false)
            .await?;
        Ok(())
    }
}
