> **[AI 助手]** 始终用中文回复，禁止使用韩语或其他任何语言。

# redisson-rs

Redisson 的 Rust 实现，目标是与 Java 版 Redisson 的行为保持一致，当前实现了分布式可重入锁（`RLock`）。

底层 Redis 客户端使用 [fred](https://github.com/aembke/fred.rs)（v10），以 `Pool` 管理连接，以 `SubscriberClient` 处理 Pub/Sub。

---

## Rust 文件组织约定

Rust 侧新增文件时，遵循以下约定：

1. 如果 Rust 类型明确对标 Java 中的某个类或接口，则 **Rust 的 struct / trait 名必须与 Java 类名保持一致**。
2. 对应的 Rust 文件名使用 **Rust 风格的小写下划线**，例如 Java `CommandMapper` 对应 `command_mapper.rs`，文件内类型名仍为 `CommandMapper`。
3. Rust 里的**字段名、局部变量名、参数名、模块名、文件名、函数名、方法名**都遵循 Rust 风格，使用 **小写下划线（snake_case）**。不要求与 Java 名称逐字一致，而是保持 Java 名称语义上的对应关系即可。
4. 如果 Rust 类型明确对齐 Java 类型，则 **类型名**（`struct`、`trait`、`enum`）继续保持 Java 风格的大驼峰命名；但**方法名不能照搬 Java 的 camelCase**，必须改写为 Rust 的 `snake_case`。
5. 如果某个 Rust 文件是在对齐某个 Java 类，则**字段顺序、方法顺序**应尽量与对应 Java 文件保持一致，方便逐段对照；但方法命名依然优先满足 Rust 规范。
6. 如果 Java 类型属于 `org.redisson.xxx` 包，则 Rust 文件优先放到与该包语义对应的目录下。
   例如：
   - `org.redisson.config.CommandMapper` → `src/config/command_mapper.rs`
   - `org.redisson.connection.ConnectionEventsHub` → `src/connection/connection_events_hub.rs`
   - `org.redisson.liveobject.resolver.MapResolver` → `src/liveobject/resolver/map_resolver.rs`
7. 如果某个辅助类型**没有明确的 Java 对应类**，暂时不要为了拆文件而额外新建模块，优先直接放在当前实现文件里。
   目前这类类型先允许放在 `src/connection/service_manager.rs` 中，后续再视需要整理。
8. 如果只是为了先对齐结构而补占位类型，可以先写空实现，但命名、目录层级和 Java 对齐关系要先放对。

---

## 仓库模块说明

本仓库根目录下有多个子模块，**只需关注以下两个**：

| 模块 | 说明 |
|------|------|
| `redisson/` | Java 核心库，包含所有数据结构、锁、连接管理的实现，是 Rust 移植的参考源 |
| `redisson-rs/` | 本项目，Rust 移植实现 |

以下模块是框架集成层，封装 `redisson` 以适配各框架生命周期，**与 Rust 移植无关，跳过不看**：

| 模块 | 说明 |
|------|------|
| `redisson-spring/` | Spring / Spring Boot 自动配置集成 |
| `redisson-helidon/` | Helidon 框架集成 |
| `redisson-quarkus/` | Quarkus 框架集成 |
| `redisson-micronaut/` | Micronaut 框架集成 |
| `redisson-hibernate/` | Hibernate 二级缓存集成 |
| `redisson-mybatis/` | MyBatis 缓存集成 |
| `redisson-tomcat/` | Tomcat Session 存储集成 |
| `redisson-all/` | 打包用汇总模块（shade jar），无业务逻辑 |

---

## 模块与 Java 文件对照

### 入口 / 客户端

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/redisson.rs` | `Redisson.java` | 实现 `RedissonClient` 接口；工厂方法 `Redisson.create(config)` 负责构建完整的依赖链（ConnectionManager → CommandAsyncExecutor → 各种对象） | `init(config)` 异步函数承担相同的初始化职责；`Redisson` 结构体持有 `CommandAsyncService`（具体类型，便于调用泛型方法）和 `ConnectionManager` |
| `src/api/redisson_client.rs` | `api/RedissonClient.java` | 顶层 API 接口，定义 `getLock`、`getMap` 等工厂方法 | trait `RedissonClient`，当前只有 `get_lock`；用 blanket impl 实现在 `Deref<Target = Redisson>` 上 |
| `src/lib.rs` | ——（无直接对应） | —— | crate 的 pub 重导出层，控制对外暴露的类型 |

---

### 分布式锁

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/api/rlock.rs` | `api/RLock.java` + `api/RLockAsync.java` | `RLock` 继承自 `RLockAsync`，定义锁的完整接口（lock/tryLock/unlock/forceUnlock/isLocked 等），区分同步/异步两套 | 合并为单一 async trait `RLock`，Rust 原生 async 无需区分同步/异步两套 API |
| `src/redisson_lock.rs` | `RedissonLock.java` | 可重入分布式锁实现；通过 Lua 脚本（hincrby/pexpire/del/publish）保证原子性；内部持有 `LockPubSub` 管理订阅 | 结构等价，使用相同 Lua 脚本；以组合（`base: RedissonBaseLock`）代替 Java 的继承，通过 `Deref` 暴露基类字段 |
| `src/redisson_base_lock.rs` | `RedissonBaseLock.java` | 抽象基类，持有 `commandExecutor`、`name`、`id`、`entryName`、续约调度器；提供 `scheduleExpirationRenewal` / `cancelExpirationRenewal` | 无继承，把 `RedissonObject` / `RedissonExpirable` / `RedissonBaseLock` 三层字段拍平为一个 struct；`ensure_hash_tag` 对应 Java 的 cluster hash-tag 包裹逻辑 |

---

### 命令执行层

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/command/command_async_executor.rs` | `command/CommandAsyncExecutor.java` | 核心接口，定义 `evalWriteAsync`、`evalReadAsync`、`writeAsync`、`readAsync` 等几十个重载，泛型 `<T, R>` 对应原始类型与业务类型；通过 `Codec` + `ByteBuf` 完成序列化 | async trait，仅保留 object-safe 方法（返回具体类型），可通过 `Arc<dyn CommandAsyncExecutor>` 调用；泛型返回（`R: FromValue`）由 `CommandAsyncService` 的非 trait 方法提供 |
| `src/command/command_async_service.rs` | `command/CommandAsyncService.java` | 实现 `CommandAsyncExecutor`；负责连接路由（选 master/slave/cluster slot）、重试、Codec 编解码 | 实现 `CommandAsyncExecutor` trait；路由和重连委托给 fred Pool；以 `V: TryInto<Value>` 代替 Java 的 Codec（写），以 `R: FromValue` 代替 Codec 解码（读）；提供 `read_async<R>`、`write_async<V,R>`、`eval_write_async_typed<R>`、`eval_read_async_typed<R>` 四个泛型非 trait 方法 |

---

### 连接管理

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/connection/connection_manager.rs` | `connection/ConnectionManager.java` | 接口：管理连接池、master/slave 节点发现、slot 路由、故障转移；有 `Single`、`MasterSlave`、`Cluster`、`Sentinel`、`Replicated` 五个实现类 | trait，精简为当前用到的三个方法：`subscribe_service()`、`service_manager()`、`shutdown()` |
| `src/connection/fred_connection_manager.rs` | `MasterSlaveConnectionManager.java` / `ClusterConnectionManager.java`（多合一） | 具体连接管理实现，区分拓扑；每种拓扑对应一个实现类 | 单一实现 `FredConnectionManager`，持有 fred `Pool`（支持 standalone/cluster/sentinel）；拓扑细节由 fred 内部处理，无需多个实现类 |
| `src/connection/service_manager.rs` | `connection/ServiceManager.java` | 持有节点 UUID（`id`）、续约调度器、订阅超时、命令超时、重试策略、publish 命令等全局服务配置 | 结构完全对齐；`publish_command`（`"publish"` 或 `"spublish"`）、`calc_unlock_latch_timeout_ms`、`renewal_scheduler` 均直接对应 |

---

### Pub/Sub

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/pubsub/publish_subscribe_service.rs` | `pubsub/PublishSubscribeService.java` | 管理频道订阅的生命周期（subscribe/unsubscribe）；维护 `channel → 等待者` 映射；通过 `PubSubConnectionEntry` 复用连接 | 持有 `SubscriberClient`（fred）；用 `DashMap<channel, RedissonLockEntry>` 管理等待者；`subscribe` 返回 `Arc<RedissonLockEntry>` |
| `src/pubsub/lock_pub_sub.rs` | `pubsub/LockPubSub.java` | 继承 `PublishSubscribe`，专用于锁消息（解锁通知）；收到消息后唤醒等待该锁的协程/线程 | 持有 `PublishSubscribeService`；`on_message` 收到解锁通知后调用 `entry.notify()` 唤醒等待者 |
| `src/pubsub/redisson_lock_entry.rs` | `RedissonLockEntry.java` | 等待锁释放的 entry；内部持有 `Semaphore` 让等待线程阻塞；`getLatch()` 获取信号量 | 用 `tokio::sync::Notify` 代替 Java `Semaphore`；`wait()` 异步等待，`notify()` 唤醒 |

---

### Watchdog 续约

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/renewal/lock_renewal_scheduler.rs` | `renewal/LockRenewalScheduler.java` | 持有各类续约任务（`LockTask`、`ReadLockTask`、`FastMultilockTask`）的 `AtomicReference`，惰性初始化；统一入口 `renewLock` / `cancelLockRenewal` | 用 `OnceLock<LockTask>` 代替 `AtomicReference`；目前只有 `LockTask`（标准锁），预留 `ReadLockTask` / `FastMultilockTask` 注释 |
| `src/renewal/lock_task.rs` | `renewal/LockTask.java` | 继承 `RenewalTask`；批量对所有持有的锁执行 `pexpire` 续约 Lua 脚本；通过 `name2entry` 维护 key → owner 映射；`tryRun()` CAS 启动后台定时任务 | 结构完全对齐；用 `tokio::spawn` + `tokio::select!` 代替 Java 的 `ScheduledExecutorService`；`DashMap` 代替 `ConcurrentHashMap` |
| `src/renewal/renewal_task.rs` | `renewal/RenewalTask.java` | 抽象基类，定义 `add` / `cancelExpirationRenewal` / `shutdown` | Rust trait，语义一致 |
| `src/renewal/lock_entry.rs` | `renewal/LockEntry.java` | 持有锁的 owner（threadId + lockName）信息 | 结构等价，持有 `owner_id: String` |

---

### 配置

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/config/mod.rs` | `config/Config.java` / `config/BaseConfig.java` | 配置 POJO，覆盖连接、认证、超时、重试、Pub/Sub 等；有 `SingleServerConfig`、`ClusterServersConfig` 等子类 | 两个结构体：`RedisConfig`（serde 反序列化用，字段全为基础类型）+ `RedissonConfig`（内部使用，字段为枚举）；`build_fred_config` 将后者转换为 fred 的 `Config` |
| `src/config/server_mode.rs` | `config/Config.java`（mode 枚举内联） | 连接模式通过不同 Config 子类区分 | `ServerMode` 枚举：`Standalone` / `Cluster` / `Sentinel` |
| `src/config/sharded_subscription_mode.rs` | `config/ClusterServersConfig.ShardedSubscriptionMode` | Cluster 模式下 Sharded Pub/Sub 开关 | 同名枚举：`Auto` / `On` / `Off` |
| `src/config/equal_jitter_delay.rs` | `config/BaseConfig`（`retryDelay` 字段，`DelayStrategy` 实现类） | 指数退避 + 随机 jitter 的重试延迟策略 | `EqualJitterDelay` 结构体，实现相同算法 |

---

### 扩展工具

| Rust 文件 | 对应 Java 文件 | Java 模块职责 | Rust 模块职责 |
|-----------|--------------|-------------|-------------|
| `src/ext.rs` | ——（无直接对应） | —— | `RedisKey` trait（统一 `&str`/`String` 作为 key）；`RedisExt` 提供 `get_json<T>` / `set_json<T>` 便捷方法，内部使用 serde_json 序列化后调用 `CommandAsyncService` 的泛型 `read_async`/`write_async` |

---

## 架构差异说明

### 1. 继承 → 组合 + Deref
Java 使用三层继承：`RedissonObject → RedissonExpirable → RedissonBaseLock → RedissonLock`。
Rust 将三层字段拍平到 `RedissonBaseLock`，`RedissonLock` 通过 `base: RedissonBaseLock` 组合 + `Deref` 实现字段直接访问。

### 2. Codec + ByteBuf → TryInto\<Value\> + FromValue
Java 通过 `Codec` 接口（运行时对象）完成序列化；fred 将其内嵌到类型系统：
- 写：`V: TryInto<Value>`（`String`、`Bytes`、`i64` 等均实现，支持文本和二进制）
- 读：`R: FromValue`（对应 Java 的 `<R>` 泛型返回）

### 3. dyn Trait vs 泛型
Java 接口天然支持泛型方法；Rust 的 `dyn Trait` 不支持。解决方案：
- `CommandAsyncExecutor` trait 保留 object-safe 方法（返回具体类型），供 `Arc<dyn CommandAsyncExecutor>` 调用（锁续约等内部路径使用）
- `CommandAsyncService` 额外提供泛型非 trait 方法（`read_async<R>`、`write_async<V,R>` 等），`Redisson.command_executor()` 返回具体类型以支持这些调用

### 4. 线程模型
Java 用 `threadId` 区分锁持有者，Redisson 用 `ServiceManager.getId() + ":" + threadId`。
Rust 用 `tokio::task::id()`，格式相同：`uuid:taskId`。watchdog 用 `tokio::spawn` + `tokio::select!` 代替 Java 的 `ScheduledExecutorService`。

---

## 当前实现范围

- [x] 标准可重入锁（`RLock` → `RedissonLock`）
- [x] Watchdog 自动续约（`LockRenewalScheduler` + `LockTask`）
- [x] Cluster Sharded Pub/Sub 自动探测
- [x] Standalone / Cluster / Sentinel 三种拓扑
- [ ] 读写锁（`RReadWriteLock`）
- [ ] 公平锁（`RedissonFairLock`）
- [ ] MultiLock / RedLock
- [ ] 其他 Redisson 对象（Map、Queue、Semaphore 等）
