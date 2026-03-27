use super::batch_handle::BatchHandle;
use super::command_async_executor::CommandAsyncExecutor;
use super::command_async_service::CommandAsyncService;
use crate::command::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::{Result, anyhow};
use fred::interfaces::ClientLike;
use fred::prelude::Value;
use fred::types::{ClusterHash, CustomCommand, FromValue};
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
        key:      String,
        args:     Vec<String>,
        tx:       oneshot::Sender<Result<Value>>,
    },
    Eval {
        script: String,
        keys:   Vec<String>,
        args:   Vec<String>,
        tx:     oneshot::Sender<Result<Value>>,
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
                BatchEntry::Command { cmd_name, key, args, .. } => {
                    let cmd = CustomCommand::new_static(cmd_name, ClusterHash::FirstKey, false);
                    let mut all_args: Vec<&str> = vec![key.as_str()];
                    all_args.extend(args.iter().map(|s| s.as_str()));
                    let _: Value = pipeline.custom(cmd, all_args).await?;
                }
                BatchEntry::Eval { script, keys, args, .. } => {
                    let cmd = CustomCommand::new_static("EVAL", ClusterHash::Random, false);
                    let numkeys = keys.len().to_string();
                    let mut all_args: Vec<&str> = vec![script.as_str(), numkeys.as_str()];
                    all_args.extend(keys.iter().map(|s| s.as_str()));
                    all_args.extend(args.iter().map(|s| s.as_str()));
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

    fn read_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Command {
            cmd_name: command.name,
            key:      key.to_string(),
            args:     args.into_iter().map(|s| s.to_string()).collect(),
            tx,
        });
        BatchHandle::new(rx)
    }

    fn write_async<T: FromValue + Send + 'static>(
        &self,
        key: &str,
        command: RedisCommand<T>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Command {
            cmd_name: command.name,
            key:      key.to_string(),
            args:     args.into_iter().map(|s| s.to_string()).collect(),
            tx,
        });
        BatchHandle::new(rx)
    }

    fn eval_write_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Eval {
            script: script.to_string(),
            keys:   keys.into_iter().map(|s| s.to_string()).collect(),
            args:   args.into_iter().map(|s| s.to_string()).collect(),
            tx,
        });
        BatchHandle::new(rx)
    }

    fn eval_read_async<T: FromValue + Send + 'static>(
        &self,
        script: &str,
        keys: Vec<&str>,
        args: Vec<&str>,
    ) -> impl Future<Output = Result<T>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Eval {
            script: script.to_string(),
            keys:   keys.into_iter().map(|s| s.to_string()).collect(),
            args:   args.into_iter().map(|s| s.to_string()).collect(),
            tx,
        });
        BatchHandle::new(rx)
    }
}
