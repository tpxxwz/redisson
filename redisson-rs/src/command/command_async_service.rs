use super::command_async_executor::CommandAsyncExecutor;
use crate::command::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::interfaces::{ClientLike, KeysInterface, LuaInterface};
use fred::prelude::{Expiration, Pool};
use fred::types::{ClusterHash, CustomCommand, FromValue};
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncService — 对应 Java org.redisson.command.CommandAsyncService
// ============================================================

pub struct CommandAsyncService {
    pub(crate) connection_manager: Arc<FredConnectionManager>,
}

impl CommandAsyncService {
    pub fn new(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self { connection_manager }
    }

    pub(crate) fn pool(&self) -> &Pool {
        &self.connection_manager.pool
    }

    pub async fn set_value(&self, key: &str, value: String, expire: Option<Expiration>) -> Result<()> {
        self.connection_manager
            .pool
            .set::<(), _, _>(key, value, expire, None, false)
            .await?;
        Ok(())
    }

    pub async fn get_str(&self, key: &str) -> Result<Option<String>> {
        Ok(self.connection_manager.pool.get(key).await?)
    }
}

// ============================================================
// CommandAsyncExecutor impl
// ============================================================

impl CommandAsyncExecutor for CommandAsyncService {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.connection_manager.clone()
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        self.connection_manager.service_manager()
    }

    fn read_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let pool = self.connection_manager.pool.clone();
        let key = key.to_string();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();
        async move {
            let cmd = CustomCommand::new_static(command.name, ClusterHash::FirstKey, false);
            let mut all_args: Vec<&str> = vec![key.as_str()];
            all_args.extend(args.iter().map(|s| s.as_str()));
            Ok(pool.custom(cmd, all_args).await?)
        }
    }

    fn write_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let pool = self.connection_manager.pool.clone();
        let key = key.to_string();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();
        async move {
            let cmd = CustomCommand::new_static(command.name, ClusterHash::FirstKey, false);
            let mut all_args: Vec<&str> = vec![key.as_str()];
            all_args.extend(args.iter().map(|s| s.as_str()));
            Ok(pool.custom(cmd, all_args).await?)
        }
    }

    fn eval_write_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let pool = self.connection_manager.pool.clone();
        let script = script.to_string();
        let keys: Vec<String> = keys.into_iter().map(|s| s.to_string()).collect();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();
        async move {
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            Ok(pool.eval(script, key_refs, arg_refs).await?)
        }
    }

    fn eval_read_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let pool = self.connection_manager.pool.clone();
        let script = script.to_string();
        let keys: Vec<String> = keys.into_iter().map(|s| s.to_string()).collect();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();
        async move {
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            Ok(pool.eval(script, key_refs, arg_refs).await?)
        }
    }
}
