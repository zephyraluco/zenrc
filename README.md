## zenrc

zenrc 是一个面向机器人控制系统的 Rust 工具集，提供行为树、日志管理、进程间通信和数据分发等核心功能模块。

## 功能

### zenrc-bt
轻量级行为树库，用于实现机器人决策逻辑。

- 支持 Sequence、Selector、StatefulSequence、StatefulSelector 节点
- 黑板机制实现节点间数据共享
- 简洁的 trait 设计，易于扩展

### zenrc-log
基于 tracing 的日志管理库。

- 支持日志文件按时间滚动（分钟/小时/天/月）
- 可配置日志级别和输出路径
- 支持日志文件数量限制
- 支持按 target 过滤到不同文件

### zenrc-shm
共享内存通信库，提供高性能进程间数据传输。

- 基于 POSIX 共享内存
- 提供 SharedMutex 同步原语
- 实现无锁环形缓冲区
- 支持 Apache Arrow 数据格式

### zenrc-dds
DDS（Data Distribution Service）的 Rust 绑定。

- 使用 bindgen 自动生成 FFI 绑定
- 支持发布-订阅模式的分布式数据通信

### zenrc-rcl
ROS2 RCL（ROS Client Library）的 Rust FFI 绑定。

- 自动生成 ROS2 C API 的 Rust 绑定
- 支持多个 ROS2 发行版（Foxy、Galactic、Humble、Iron、Jazzy、Rolling）
- 智能缓存机制加速构建
- 跨平台支持（Linux/macOS/Windows）

### zenrc-macros
为其他 zenrc 模块提供过程宏支持。

## 依赖

主要依赖项：

- `nix` - POSIX API 绑定
- `arrow` - Apache Arrow 数据格式
- `tracing` / `tracing-subscriber` - 日志追踪
- `crossbeam-channel` - 并发通道
- `bindgen` - C/C++ 绑定生成
- `thiserror` / `anyhow` - 错误处理

## 路线图

- [ ] 完善 zenrc-dds 的 API 封装
- [ ] 完善 zenrc-rcl 的 API 封装
- [ ] 为各模块添加完整的文档和示例
- [ ] 添加性能基准测试
- [ ] 支持更多行为树节点类型
- [ ] 优化共享内存的零拷贝性能

## 构建

```bash
# 构建所有模块
cargo build --workspace

# 运行测试
cargo test --workspace

# 运行示例
cargo run --example printonde -p zenrc-bt
cargo run --example span -p zenrc-log
```
