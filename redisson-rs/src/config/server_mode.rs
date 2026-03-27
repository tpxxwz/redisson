// ============================================================
// ServerMode — 对应 Java 中通过不同 ConnectionManager 类型区分的连接模式
// (SingleConnectionManager / ClusterConnectionManager / SentinelConnectionManager)
// ============================================================

/// Redis 服务器连接模式。
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerMode {
    Standalone,
    Cluster,
    Sentinel,
}

impl ServerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerMode::Standalone => "standalone",
            ServerMode::Cluster => "cluster",
            ServerMode::Sentinel => "sentinel",
        }
    }
}
