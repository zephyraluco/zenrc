# zenrc Agent Instructions

zenrc 是一个面向机器人控制系统的 Rust 工具集，采用 Cargo workspace 组织。
代码注释和文档统一使用**简体中文**。

## 构建与测试

```bash
# 构建所有模块
cargo build --workspace

# 运行测试
cargo test --workspace

# 运行示例
cargo run --example printonde -p zenrc-bt
cargo run --example span -p zenrc-log
cargo run --example shmpub -p zenrc-shm   # 需配套 shmsub
```

> **注意**：`zenrc-dds` 构建需要系统安装 CycloneDDS（通过 `pkg-config` 探测）。
> 若未安装，构建会失败。其他模块（bt、shm、log、macros）无此依赖，可单独构建：
> `cargo build -p zenrc-bt`

## 工作区结构

| Crate | 职责 |
|---|---|
| `zenrc` | 主应用；DDS pub/sub 的安全封装（`zenrc/src/dds/`） |
| `zenrc-dds` | CycloneDDS FFI 绑定，通过 bindgen 自动生成 |
| `zenrc-bt` | 纯 Rust 行为树框架，含黑板机制 |
| `zenrc-shm` | POSIX 共享内存 + 环形缓冲区 + 同步原语 |
| `zenrc-log` | 基于 tracing 的日志库，支持按时间滚动 |
| `zenrc-macros` | 过程宏，提供参数注册等元编程能力 |

Rust edition: **2024**

## 核心约定

### 错误处理

- 各 crate 定义自己的 `#[derive(Error)] enum` 错误类型（`thiserror`），并提供 `type Result<T> = std::result::Result<T, XxxError>` 别名
- `anyhow` 用于应用层（跨 crate 错误聚合），`thiserror` 用于库层（结构化错误）
- FFI 返回值通过辅助函数校验，例如 `check_entity(entity: dds_entity_t) -> Result<dds_entity_t>`

### Unsafe 代码

- FFI 调用和 `std::mem::zeroed()` 等操作必须包裹在 `unsafe {}` 块中
- 每处 `unsafe impl Send/Sync` 必须附带 `// SAFETY: ...` 注释说明线程安全原因

### DDS 模块（`zenrc/src/dds/`）

关键文件：[`error.rs`](zenrc/src/dds/error.rs) · [`qos.rs`](zenrc/src/dds/qos.rs) · [`domain.rs`](zenrc/src/dds/domain.rs) · [`topic.rs`](zenrc/src/dds/topic.rs) · [`publisher.rs`](zenrc/src/dds/publisher.rs) · [`subscriber.rs`](zenrc/src/dds/subscriber.rs) · [`waitset.rs`](zenrc/src/dds/waitset.rs)

- `DomainParticipant` 持有 `Arc<ParticipantInner>`，`Publisher<T>` 和 `Subscription<T>` 持有该 Arc，确保 **销毁顺序**：Writer/Reader → Topic → Participant
- 所有消息类型须实现 `RawMessageBridge` trait（定义在 `zenrc-dds/src/lib.rs`），提供 `to_raw()` / `from_raw()` / `free_contents()` 转换
- 消息类型在构建时由 `msg_gen.rs` 从 IDL 自动生成，勿手动修改生成代码

### 行为树（`zenrc-bt`）

- 节点实现 `Node` trait，必须覆写 `update()` 方法返回 `Status`
- 复合节点实现 `Composite: Node`；黑板通过 `BlackboardPtr`（`Arc<RefCell<HashMap<String, Box<dyn Any>>>>>`）共享数据
- 参考示例：[`zenrc-bt/examples/printonde.rs`](zenrc-bt/examples/printonde.rs)

### 共享内存（`zenrc-shm`）

- `MemoryHandle` 封装 POSIX `shm_open` + `mmap`；所有者用 `new()` 创建，其他进程用 `open()` 只读挂载
- 通过 `Drop` 自动清理；并发访问使用 `SharedMutex`（基于 POSIX `pthread_mutex`）

## 构建环境变量（zenrc-dds）

| 变量 | 用途 |
|---|---|
| `AMENT_PREFIX_PATH` | ROS2 安装路径，用于 IDL 文件发现 |
| `ROS_DISTRO` | ROS2 版本（humble/jazzy/rolling 等） |
| `CMAKE_PREFIX_PATH` | pkg-config 查找 CycloneDDS 的路径 |
| `CMAKE_IDL_PACKAGES` | 限定需要绑定的 IDL 包（可选） |
| `IDL_PACKAGE_FILTER` | IDL 包过滤器（可选） |
| `DDS_IDL_PATH` | 自定义 IDL 文件路径（可选） |

构建系统对这 6 个变量的 SHA256 哈希做缓存，变量未变则跳过重新生成绑定。

## 参考文档

- [DDS 绑定 API 参考](zenrc-dds/DDS_BINDINGS_API.md) — 326 个 CycloneDDS 函数分类说明
- [README](README.md) — 各模块功能概述与路线图
