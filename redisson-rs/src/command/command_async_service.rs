use super::command_async_executor::CommandAsyncExecutor;
use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::error::Error;
use fred::interfaces::{ClientLike, KeysInterface, LuaInterface};
use fred::prelude::{Expiration, Pool};
use fred::types::{ClusterHash, CustomCommand, FromValue, MultipleKeys, MultipleValues, Value};
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

    pub async fn set_value(
        &self,
        key: &str,
        value: String,
        expire: Option<Expiration>,
    ) -> Result<()> {
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

    fn read_async<T, K, V>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: V,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let key_value = key.into().into_values().into_iter().next().unwrap_or(Value::Null);
        let args_result = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", e.into()));
        async move {
            let args_vec = args_result?.into_array();
            let cmd = CustomCommand::new_static(command.name, ClusterHash::FirstKey, false);
            let mut all_args = Vec::new();
            if let Some(sub) = command.sub_name {
                all_args.push(sub.into());
            }
            all_args.push(key_value);
            all_args.extend(args_vec);
            if let Some(conv) = command.convertor {
                let raw: Value = pool.custom(cmd, all_args).await?;
                conv.convert(raw)
            } else {
                Ok(pool.custom(cmd, all_args).await?)
            }
        }
    }

    fn write_async<T, K, V>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: V,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let key_value = key.into().into_values().into_iter().next().unwrap_or(Value::Null);
        let args_result = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", e.into()));
        async move {
            let args_vec = args_result?.into_array();
            let cmd = CustomCommand::new_static(command.name, ClusterHash::FirstKey, false);
            let mut all_args = Vec::new();
            if let Some(sub) = command.sub_name {
                all_args.push(sub.into());
            }
            all_args.push(key_value);
            all_args.extend(args_vec);
            if let Some(conv) = command.convertor {
                let raw: Value = pool.custom(cmd, all_args).await?;
                conv.convert(raw)
            } else {
                Ok(pool.custom(cmd, all_args).await?)
            }
        }
    }

    fn eval_write_async<T, K, V>(
        &self,
        script: &str,
        keys: K,
        args: V,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let script = script.to_string();
        let keys: Vec<String> = keys.into().inner().into_iter().map(|k| k.as_str_lossy().into_owned()).collect();
        let args_result = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", e.into()));
        async move {
            let args_vec = args_result?.into_array();
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            Ok(pool.eval(script, key_refs, args_vec).await?)
        }
    }

    fn eval_read_async<T, K, V>(
        &self,
        script: &str,
        keys: K,
        args: V,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let script = script.to_string();
        let keys: Vec<String> = keys.into().inner().into_iter().map(|k| k.as_str_lossy().into_owned()).collect();
        let args_result = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", e.into()));
        async move {
            let args_vec = args_result?.into_array();
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            Ok(pool.eval(script, key_refs, args_vec).await?)
        }
    }
}
