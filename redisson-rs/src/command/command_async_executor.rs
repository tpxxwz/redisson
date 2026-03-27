use crate::command::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::types::FromValue;
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor
// ============================================================

pub trait CommandAsyncExecutor: Send + Sync + 'static {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    fn service_manager(&self) -> &Arc<ServiceManager>;

    fn read_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static;

    fn write_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static;

    fn eval_write_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static;

    fn eval_read_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static;
}
