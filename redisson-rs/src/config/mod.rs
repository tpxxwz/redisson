pub mod equal_jitter_delay;
pub mod name_mapper;
pub mod server_mode;
pub mod sharded_subscription_mode;

pub use name_mapper::NameMapper;

use crate::config::equal_jitter_delay::EqualJitterDelay;
use crate::config::server_mode::ServerMode;
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use anyhow::Result;
use fred::prelude::*;
use fred::types::config::ClusterDiscoveryPolicy;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

// ============================================================
// RedisNode
// ============================================================

#[derive(Deserialize, Clone)]
pub struct RedisNode {
    pub host: String,
    pub port: u16,
}

// ============================================================
// RedisConfig — 用于配置文件（YAML 等）反序列化，所有字段为基础类型
// ============================================================

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct RedisConfig {
    // ── 连接 ──
    /// "standalone" | "cluster" | "sentinel"
    pub mode: String,
    pub host: String,
    pub port: u16,
    pub nodes: Vec<RedisNode>,
    pub sentinel_service_name: String,
    pub sentinel_username: String,
    pub sentinel_password: String,

    // ── 认证 ──
    pub username: String,
    pub password: String,
    pub db: u8,

    // ── 连接池 ──
    pub pool_size: usize,

    // ── 超时 ──
    pub connect_timeout_secs: u64,
    pub command_timeout_secs: u64,

    // ── 重试 ──
    pub max_command_attempts: u32,
    pub max_redirections: u32,

    // ── 重连 ──
    pub reconnect_max_attempts: u32,
    pub reconnect_min_delay_ms: u32,
    pub reconnect_max_delay_ms: u32,
    pub reconnect_multiplier: u32,

    // ── 分布式锁 ──
    pub lock_watchdog_timeout: u64,
    pub subscription_timeout: u64,
    pub command_timeout_ms: u64,
    pub retry_attempts: u32,
    pub retry_delay_base_ms: u64,
    pub retry_delay_max_ms: u64,

    // ── Pub/Sub ──
    /// "auto" | "on" | "off"
    pub sharded_subscription_mode: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            mode: "standalone".to_string(),
            host: "localhost".to_string(),
            port: 6379,
            nodes: Vec::new(),
            sentinel_service_name: "mymaster".to_string(),
            sentinel_username: String::new(),
            sentinel_password: String::new(),
            username: String::new(),
            password: String::new(),
            db: 0,
            pool_size: 5,
            connect_timeout_secs: 5,
            command_timeout_secs: 0,
            max_command_attempts: 3,
            max_redirections: 5,
            reconnect_max_attempts: 0,
            reconnect_min_delay_ms: 1,
            reconnect_max_delay_ms: 30_000,
            reconnect_multiplier: 2,
            lock_watchdog_timeout: 30,
            subscription_timeout: 7_500,
            command_timeout_ms: 3_000,
            retry_attempts: 4,
            retry_delay_base_ms: 1_000,
            retry_delay_max_ms: 2_000,
            sharded_subscription_mode: "auto".to_string(),
        }
    }
}

// ============================================================
// RedissonConfig — 程序内部使用，字段类型明确（枚举代替字符串）
// ============================================================

#[derive(Clone)]
pub struct RedissonConfig {
    // ── 名称映射 ──
    /// 对应 Java Config.nameMapper，默认 DefaultNameMapper（直接透传）
    pub name_mapper: Arc<dyn name_mapper::NameMapper>,

    // ── 连接 ──
    pub mode: ServerMode,
    pub host: String,
    pub port: u16,
    pub nodes: Vec<RedisNode>,
    pub sentinel_service_name: String,
    pub sentinel_username: String,
    pub sentinel_password: String,

    // ── 认证 ──
    pub username: String,
    pub password: String,
    pub db: u8,

    // ── 连接池 ──
    pub pool_size: usize,

    // ── 超时 ──
    pub connect_timeout_secs: u64,
    pub command_timeout_secs: u64,

    // ── 重试 ──
    pub max_command_attempts: u32,
    pub max_redirections: u32,

    // ── 重连 ──
    pub reconnect_max_attempts: u32,
    pub reconnect_min_delay_ms: u32,
    pub reconnect_max_delay_ms: u32,
    pub reconnect_multiplier: u32,

    // ── 分布式锁 ──
    pub lock_watchdog_timeout: u64,
    pub subscription_timeout: u64,
    pub command_timeout_ms: u64,
    pub retry_attempts: u32,
    /// 对应 Java BaseConfig.retryDelay（DelayStrategy 实现类）
    pub retry_delay: EqualJitterDelay,

    // ── Pub/Sub ──
    pub sharded_subscription_mode: ShardedSubscriptionMode,
}

impl RedissonConfig {
    /// 对应 Java Config.setNameMapper(NameMapper)
    pub fn set_name_mapper(&mut self, mapper: Arc<dyn name_mapper::NameMapper>) -> &mut Self {
        self.name_mapper = mapper;
        self
    }
}

impl TryFrom<RedisConfig> for RedissonConfig {
    type Error = anyhow::Error;

    fn try_from(c: RedisConfig) -> Result<Self> {
        let mode = match c.mode.to_lowercase().as_str() {
            "standalone" => ServerMode::Standalone,
            "cluster" => ServerMode::Cluster,
            "sentinel" => ServerMode::Sentinel,
            other => anyhow::bail!(
                "Invalid mode '{}'. Expected: standalone, cluster, sentinel",
                other
            ),
        };
        let sharded_subscription_mode = match c.sharded_subscription_mode.to_lowercase().as_str() {
            "auto" => ShardedSubscriptionMode::Auto,
            "on" => ShardedSubscriptionMode::On,
            "off" => ShardedSubscriptionMode::Off,
            other => anyhow::bail!(
                "Invalid sharded_subscription_mode '{}'. Expected: auto, on, off",
                other
            ),
        };
        Ok(Self {
            name_mapper: name_mapper::direct(),
            mode,
            host: c.host,
            port: c.port,
            nodes: c.nodes,
            sentinel_service_name: c.sentinel_service_name,
            sentinel_username: c.sentinel_username,
            sentinel_password: c.sentinel_password,
            username: c.username,
            password: c.password,
            db: c.db,
            pool_size: c.pool_size,
            connect_timeout_secs: c.connect_timeout_secs,
            command_timeout_secs: c.command_timeout_secs,
            max_command_attempts: c.max_command_attempts,
            max_redirections: c.max_redirections,
            reconnect_max_attempts: c.reconnect_max_attempts,
            reconnect_min_delay_ms: c.reconnect_min_delay_ms,
            reconnect_max_delay_ms: c.reconnect_max_delay_ms,
            reconnect_multiplier: c.reconnect_multiplier,
            lock_watchdog_timeout: c.lock_watchdog_timeout,
            subscription_timeout: c.subscription_timeout,
            command_timeout_ms: c.command_timeout_ms,
            retry_attempts: c.retry_attempts,
            retry_delay: EqualJitterDelay::new(c.retry_delay_base_ms, c.retry_delay_max_ms),
            sharded_subscription_mode,
        })
    }
}

// ============================================================
// fred 配置构建函数（接收 RedissonConfig）
// ============================================================

pub(crate) fn build_fred_config(config: &RedissonConfig) -> Result<Config> {
    let server = match &config.mode {
        ServerMode::Standalone => ServerConfig::Centralized {
            server: Server::new(&config.host, config.port),
        },
        ServerMode::Cluster => {
            let hosts: Vec<Server> = config
                .nodes
                .iter()
                .map(|n| Server::new(&n.host, n.port))
                .collect();
            anyhow::ensure!(
                !hosts.is_empty(),
                "Cluster mode requires at least one node in 'nodes'"
            );
            ServerConfig::Clustered {
                hosts,
                policy: ClusterDiscoveryPolicy::default(),
            }
        }
        ServerMode::Sentinel => {
            let hosts: Vec<Server> = config
                .nodes
                .iter()
                .map(|n| Server::new(&n.host, n.port))
                .collect();
            anyhow::ensure!(
                !hosts.is_empty(),
                "Sentinel mode requires at least one node in 'nodes'"
            );
            ServerConfig::Sentinel {
                hosts,
                service_name: config.sentinel_service_name.clone(),
                username: if config.sentinel_username.is_empty() {
                    None
                } else {
                    Some(config.sentinel_username.clone())
                },
                password: if config.sentinel_password.is_empty() {
                    None
                } else {
                    Some(config.sentinel_password.clone())
                },
            }
        }
    };

    Ok(Config {
        server,
        username: if config.username.is_empty() {
            None
        } else {
            Some(config.username.clone())
        },
        password: if config.password.is_empty() {
            None
        } else {
            Some(config.password.clone())
        },
        database: match config.mode {
            ServerMode::Cluster => None,
            _ => Some(config.db),
        },
        ..Default::default()
    })
}

pub(crate) fn build_perf_config(config: &RedissonConfig) -> PerformanceConfig {
    let mut perf = PerformanceConfig::default();
    if config.command_timeout_secs > 0 {
        perf.default_command_timeout = Duration::from_secs(config.command_timeout_secs);
    }
    perf
}

pub(crate) fn build_connection_config(config: &RedissonConfig) -> ConnectionConfig {
    let mut conn = ConnectionConfig::default();
    if config.connect_timeout_secs > 0 {
        conn.connection_timeout = Duration::from_secs(config.connect_timeout_secs);
    }
    conn.max_command_attempts = config.max_command_attempts;
    conn.max_redirections = config.max_redirections;
    conn
}
