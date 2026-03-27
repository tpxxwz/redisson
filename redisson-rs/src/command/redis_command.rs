use std::marker::PhantomData;

// ============================================================
// RedisCommand<T> — 对应 Java org.redisson.client.protocol.RedisCommand<T>
// ============================================================

/// Redis 命令描述符，对应 Java RedisCommand<T>。
/// PhantomData<fn() -> T> 在编译期携带返回类型信息，运行时零开销。
pub struct RedisCommand<T> {
    /// Redis 命令名，对应 Java RedisCommand.name
    pub(crate) name: &'static str,
    _phantom: PhantomData<fn() -> T>,
}

impl<T> RedisCommand<T> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _phantom: PhantomData,
        }
    }
}

// ============================================================
// commands — 对应 Java org.redisson.client.protocol.RedisCommands（固定常量集合）
// ============================================================

/// 预定义 Redis 命令常量，对应 Java org.redisson.client.protocol.RedisCommands。
pub mod commands {
    use super::RedisCommand;

    /// 对应 Java RedisCommands.EXISTS
    pub const EXISTS: RedisCommand<bool> = RedisCommand::new("EXISTS");
    /// 对应 Java RedisCommands.HEXISTS
    pub const HEXISTS: RedisCommand<bool> = RedisCommand::new("HEXISTS");
    /// 对应 Java RedisCommands.PTTL
    pub const PTTL: RedisCommand<i64> = RedisCommand::new("PTTL");
    /// 对应 Java RedisCommands.DEL
    pub const DEL: RedisCommand<i64> = RedisCommand::new("DEL");
    /// 对应 Java RedisCommands.GET
    pub const GET: RedisCommand<Option<String>> = RedisCommand::new("GET");
    /// 对应 Java RedisCommands.SET
    pub const SET: RedisCommand<()> = RedisCommand::new("SET");
    /// 对应 Java RedisCommands.STRLEN
    pub const STRLEN: RedisCommand<i64> = RedisCommand::new("STRLEN");
}
