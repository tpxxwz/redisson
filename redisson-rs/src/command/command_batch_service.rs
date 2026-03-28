use super::batch_handle::BatchHandle;
use super::command_async_executor::CommandAsyncExecutor;
use super::command_async_service::CommandAsyncService;
use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::{Result, anyhow};
use fred::error::Error;
use fred::interfaces::ClientLike;
use fred::prelude::Value;
use fred::types::{ClusterHash, CustomCommand, FromValue, MultipleKeys, MultipleValues};
use parking_lot::Mutex;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::oneshot;

// ============================================================
// BatchEntry
// ============================================================

enum BatchEntry {
    Command {
        cmd_name: &'static str,
        key: String,
        args: Vec<Value>,
        tx: oneshot::Sender<Result<Value>>,
    },
    Eval {
        script: String,
        keys: Vec<String>,
        args: Vec<Value>,
        tx: oneshot::Sender<Result<Value>>,
    },
}

// ============================================================
// CommandBatchService — 对应 Java CommandBatchService
// ============================================================

pub struct CommandBatchService {
    pub(crate) base: Arc<CommandAsyncService>,
    queue: Mutex<Vec<BatchEntry>>,
}

impl CommandBatchService {
    pub fn new(base: Arc<CommandAsyncService>) -> Self {
        Self {
            base,
            queue: Mutex::new(Vec::new()),
        }
    }

    /// 对应 Java RBatch.execute()
    pub async fn execute(&self) -> Result<()> {
        let entries: Vec<BatchEntry> = std::mem::take(&mut *self.queue.lock());

        if entries.is_empty() {
            return Ok(());
        }

        let client = self.base.pool().next();
        let pipeline = client.pipeline();

        for entry in &entries {
            match entry {
                BatchEntry::Command {
                    cmd_name,
                    key,
                    args,
                    ..
                } => {
                    let cmd = CustomCommand::new_static(cmd_name, ClusterHash::FirstKey, false);
                    let mut all_args: Vec<Value> = vec![key.clone().into()];
                    all_args.extend(args.iter().cloned());
                    let _: Value = pipeline.custom(cmd, all_args).await?;
                }
                BatchEntry::Eval {
                    script, keys, args, ..
                } => {
                    let cmd = CustomCommand::new_static("EVAL", ClusterHash::Random, false);
                    let numkeys = keys.len().to_string();
                    let mut all_args: Vec<Value> = vec![script.clone().into(), numkeys.into()];
                    all_args.extend(keys.iter().cloned().map(|k| k.into()));
                    all_args.extend(args.iter().cloned());
                    let _: Value = pipeline.custom(cmd, all_args).await?;
                }
            }
        }

        let results = pipeline.try_all::<Value>().await;

        for (entry, result) in entries.into_iter().zip(results.into_iter()) {
            let tx = match entry {
                BatchEntry::Command { tx, .. } => tx,
                BatchEntry::Eval { tx, .. } => tx,
            };
            let _ = tx.send(result.map_err(|e| anyhow!(e)));
        }

        Ok(())
    }
}

// ============================================================
// CommandAsyncExecutor impl — Output<T> = BatchHandle<T>
// ============================================================

impl CommandAsyncExecutor for CommandBatchService {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.base.connection_manager()
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        self.base.service_manager()
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
        let key = key.into().into_values().into_iter().next().unwrap_or(Value::Null).as_str().unwrap_or_default().to_string();
        let args = args.try_into().map(|v: Value| v.into_array()).unwrap_or_default();
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Command {
            cmd_name: command.name,
            key,
            args,
            tx,
        });
        BatchHandle::new(rx)
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
        let key = key.into().into_values().into_iter().next().unwrap_or(Value::Null).as_str().unwrap_or_default().to_string();
        let args = args.try_into().map(|v: Value| v.into_array()).unwrap_or_default();
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Command {
            cmd_name: command.name,
            key,
            args,
            tx,
        });
        BatchHandle::new(rx)
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
        let keys: Vec<String> = keys.into().inner().into_iter().map(|k| k.as_str_lossy().into_owned()).collect();
        let args = args.try_into().map(|v: Value| v.into_array()).unwrap_or_default();
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Eval {
            script: script.to_string(),
            keys,
            args,
            tx,
        });
        BatchHandle::new(rx)
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
        let keys: Vec<String> = keys.into().inner().into_iter().map(|k| k.as_str_lossy().into_owned()).collect();
        let args = args.try_into().map(|v: Value| v.into_array()).unwrap_or_default();
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Eval {
            script: script.to_string(),
            keys,
            args,
            tx,
        });
        BatchHandle::new(rx)
    }
}
