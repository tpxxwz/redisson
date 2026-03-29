use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::error::Error;
use fred::types::{FromValue, Key, MultipleKeys, MultipleValues, Value};
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor
// ============================================================

pub trait CommandAsyncExecutor: Send + Sync + 'static {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    fn service_manager(&self) -> &Arc<ServiceManager>;

    /// 对应 Java BatchService 类型检查，用于 checkNotBatch()
    /// CommandAsyncService 返回 false，CommandBatchService 覆盖返回 true
    fn is_batch(&self) -> bool {
        false
    }

    /// 对应 Java isEvalCacheActive()，子类可覆盖。
    /// CommandAsyncService 返回 connectionManager.config.useScriptCache（默认 true）。
    /// CommandBatchService 覆盖返回 false。
    fn is_eval_cache_active(&self) -> bool {
        self.connection_manager().config().use_script_cache
    }

    /// 对应 Java readAsync(byte[] key, ...) / readAsync(ByteBuf key, ...) / readAsync(String key, ...) 等 key 路由重载。
    /// K: Into<MultipleKeys> 统一覆盖了 Java 中 byte[] / ByteBuf / String 三类 key 重载。
    /// 注意：Java 中还有 readAsync(RedisClient, ...) / readAsync(MasterSlaveEntry, ...) 两类
    /// 节点级路由重载（绕过 key slot，直接指定节点），K: Into<MultipleKeys> 无法覆盖，
    /// 需要 readAllAsync 等单独处理。
    /// R: TryInto<Value> 直接对齐 fred pool.custom() 的参数约束，
    /// 避免绕行 MultipleValues。
    fn read_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    /// 对应 Java writeAsync(byte[] key, ...) / writeAsync(ByteBuf key, ...) / writeAsync(String key, ...) 等 key 路由重载。
    /// K / R 约束同 read_async，参见上方注释。
    fn write_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    /// 对应 Java evalWriteAsync(String key, ...) / evalWriteAsync(ByteBuf key, ...) 等 key 路由重载。
    /// command.name 区分 "EVAL" / "EVALSHA"，command.convertor 处理 Lua 返回值自定义转换
    /// （对应 Java 的 EVAL_INTEGER / EVAL_BOOLEAN_WITH_VALUES 等不同 RedisCommand 实例）。
    /// keys 是 Lua 脚本的 KEYS 数组，R: TryInto<MultipleValues> 直接对齐
    /// fred pool.eval() 的参数约束。
    /// 对应 Java evalWriteAsync(String key, ..., List<Object> keys, ...)
    /// key：路由 key，决定发到哪个节点（对应 Java 第一个 key 参数）
    /// keys：Lua 脚本的 KEYS 数组，不参与路由决策
    fn eval_write_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send,
        R::Error: Into<Error> + Send;

    /// 对应 Java evalReadAsync(String key, ..., List<Object> keys, ...)
    /// key / keys 约束同 eval_write_async，参见上方注释。
    fn eval_read_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send,
        R::Error: Into<Error> + Send;
}
