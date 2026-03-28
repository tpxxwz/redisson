use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::error::Error;
use fred::types::{FromValue, MultipleKeys, MultipleValues};
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor
// ============================================================

pub trait CommandAsyncExecutor: Send + Sync + 'static {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    fn service_manager(&self) -> &Arc<ServiceManager>;

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
        V::Error: Into<Error> + Send;

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
        V::Error: Into<Error> + Send;

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
        V::Error: Into<Error> + Send;

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
        V::Error: Into<Error> + Send;
}
