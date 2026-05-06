# AGENTS.md

面向本仓库内 AI 编码代理的最小工作说明。目标是让代理在不猜测的前提下快速构建、验证和定位模块。

## 工作区事实来源

- 工作区成员以 [Cargo.toml](Cargo.toml) 的 workspace.members 为准。
- 目前成员 crate:
  - zenrc
  - zenrc-dds
  - zenrc-shm
  - zenrc-log
  - zenrc-bt
  - zenrc-macros
- [README.md](README.md) 含历史信息（例如提到 zenrc-rcl）。当 README 与实际代码不一致时，以当前工作区文件为准。

## 环境与先决条件

- Rust edition 使用 2024（所有 crate 的 Cargo.toml）。
- 编译 zenrc-dds 需要 CycloneDDS 开发环境:
  - 可被 pkg-config 发现的 CycloneDDS
  - 可执行文件 idlc 可用（PATH 中，或可由 CycloneDDS 安装目录推导）
- 生成 ROS2 消息绑定常见环境变量（由 [zenrc-dds/build.rs](zenrc-dds/build.rs) 监控）:
  - AMENT_PREFIX_PATH
  - CMAKE_PREFIX_PATH
  - CMAKE_IDL_PACKAGES
  - IDL_PACKAGE_FILTER
  - ROS_DISTRO
  - DDS_IDL_PATH

## 常用命令

- 全量构建: cargo build --workspace
- 全量测试: cargo test --workspace
- 指定 crate 构建: cargo build -p <crate>
- 示例运行:
  - cargo run --example printonde -p zenrc-bt
  - cargo run --example span -p zenrc-log
  - cargo run --example reader -p zenrc-shm
  - cargo run --example writer -p zenrc-shm
  - cargo run --example shmpub -p zenrc-shm
  - cargo run --example shmsub -p zenrc-shm
  - cargo run -p zenrc
- Benchmark:
  - 仅编译不运行: `cargo bench -p zenrc --no-run`
  - 运行全部 benchmark: `cargo bench -p zenrc`
  - 运行指定 benchmark: `cargo bench -p zenrc -- <benchmark_name>`
  - benchmark 文件位置: [zenrc/benches/dds_bench.rs](zenrc/benches/dds_bench.rs)
  - 依赖: `criterion = "0.5"`（在 `zenrc/Cargo.toml` 的 `[dev-dependencies]` 中，并声明 `[[bench]] name = "dds_bench" harness = false`）
  - 注意: benchmark 运行需要有效的 CycloneDDS 环境（同编译要求）

## 组件边界

- [zenrc/src/main.rs](zenrc/src/main.rs): 集成示例与运行入口（tokio + dds 封装）。
- [zenrc/src/dds/mod.rs](zenrc/src/dds/mod.rs): dds 封装模块入口（context/publisher/subscriber/service/qos/waitset/topic/error）。
- [zenrc-dds/build.rs](zenrc-dds/build.rs): FFI 绑定与 IDL 代码生成管线（bindings.rs/msg_bindings.rs/包装代码）。
- [zenrc-dds/src/lib.rs](zenrc-dds/src/lib.rs): dds 低层导出与生成代码入口。
- [zenrc-shm/src/lib.rs](zenrc-shm/src/lib.rs): 共享内存模块导出（shm/sync/ringbuffer/errors）。
- [zenrc-log/src/lib.rs](zenrc-log/src/lib.rs): tracing 日志入口。
- [zenrc-bt/src/lib.rs](zenrc-bt/src/lib.rs): 行为树核心 trait 与节点语义。
- [zenrc-macros/src/lib.rs](zenrc-macros/src/lib.rs): 过程宏实现。

## zenrc 结构

- `zenrc` 是工作区里的应用层 crate，负责把 `zenrc-dds` 的底层绑定组织成更易用的发布/订阅/服务接口。
- 当前目录结构:
  - [zenrc/src/main.rs](zenrc/src/main.rs): 运行示例，展示 `DdsContext`、`ServiceServer`、`ServiceClient` 的组合方式。
  - [zenrc/src/dds/mod.rs](zenrc/src/dds/mod.rs): 本地 dds 封装的模块声明入口。
  - [zenrc/src/dds/context.rs](zenrc/src/dds/context.rs): `DomainParticipant` 与 `DdsContext`，负责实体创建、共享 WaitSet、异步通知注册。
  - [zenrc/src/dds/service.rs](zenrc/src/dds/service.rs): service/client 封装，包含同步请求获取与事件回调模式。
  - [zenrc/src/dds/publisher.rs](zenrc/src/dds/publisher.rs): publisher 封装与写入接口。
  - [zenrc/src/dds/subscriber.rs](zenrc/src/dds/subscriber.rs): subscriber 封装、take/read API 与异步 stream 支持。
  - [zenrc/src/dds/topic.rs](zenrc/src/dds/topic.rs): topic 句柄与生命周期管理。
  - [zenrc/src/dds/qos.rs](zenrc/src/dds/qos.rs): QoS builder 与默认策略。
  - [zenrc/src/dds/waitset.rs](zenrc/src/dds/waitset.rs): WaitSet/GuardCondition 的同步等待封装。
  - [zenrc/src/dds/error.rs](zenrc/src/dds/error.rs): dds 相关错误类型与返回码转换。
  - [zenrc/src/dds/async_stream.rs](zenrc/src/dds/async_stream.rs): 仅在 `async` feature 下启用，提供订阅流适配。
- 代码定位建议:
  - 改“实体如何创建/附加到上下文”时，先看 `context.rs`。
  - 改“服务/客户端行为”时，先看 `service.rs`。
  - 改“订阅读取语义或异步流”时，先看 `subscriber.rs`。
  - 改“阻塞等待或触发机制”时，先看 `waitset.rs`。

## 代理执行约定

- 优先最小改动: 只改与任务直接相关的 crate 和文件。
- 改动 zenrc-dds 生成流程相关代码时，同时检查 build.rs 中环境缓存逻辑，避免误判为“代码无效”。
- 涉及消息类型时，优先沿用现有导入方式（zenrc 中使用 zenrc_dds::std_msgs）。
- 若任务涉及接口行为变更，至少执行:
  - cargo test --workspace
  - 与改动 crate 对应的 example 或最小运行命令

## 现有测试位置（便于快速验证）

- [zenrc/src/dds/qos.rs](zenrc/src/dds/qos.rs): QoS 单元测试
- [zenrc-log/src/appender/non_blocking.rs](zenrc-log/src/appender/non_blocking.rs): 非阻塞日志 appender 测试

## 文档链接（只链接，不复制）

- 项目总览与快速命令: [README.md](README.md)
- CycloneDDS API 分类参考: [zenrc-dds/DDS_BINDINGS_API.md](zenrc-dds/DDS_BINDINGS_API.md)