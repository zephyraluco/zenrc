## zenrc

zenrc 是一个面向机器人控制系统的 Rust 工具集，提供行为树、日志管理、进程间通信和数据分发等核心功能模块。

## 功能

### zenrc
DDS 封装层，基于 `zenrc-dds` 提供符合人体工程学的高层接口。

- `DdsContext`：统一管理 DDS 实体的生命周期
- `Publisher<T>` / `Subscriber<T>`：类型安全的发布/订阅
- `ServiceServer<Req, Res>` / `ServiceClient<Req, Res>`：请求-响应服务模式
- 基于 `async`（tokio）的事件回调与异步 stream
- `Qos` builder 提供常用默认策略

### zenrc-dds
CycloneDDS 的底层 Rust FFI 绑定。

- 使用 `bindgen` 自动生成 C API 绑定
- `build.rs` 驱动 IDL 编译与 ROS2 消息绑定生成
- 通过环境变量（`AMENT_PREFIX_PATH`、`CMAKE_PREFIX_PATH` 等）自动发现消息包
- 基于 SHA256 的构建缓存，避免重复生成

### zenrc-msgen
IDL / ROS2 消息代码生成的可复用构建工具库。

- `compile_idl_libs`：将 `idlc` 生成的 `.c` 文件编译为静态库
- `generate_msg_bindings`：为 IDL 头文件生成 Rust `bindgen` 绑定
- `generate_rust_wrappers`：生成类型安全的 Rust 包装代码

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

## 依赖

主要依赖项：

- `nix` - POSIX API 绑定
- `arrow` - Apache Arrow 数据格式
- `tracing` / `tracing-subscriber` - 日志追踪
- `tokio` - 异步运行时
- `crossbeam-channel` - 并发通道
- `bindgen` - C/C++ 绑定生成
- `thiserror` / `anyhow` - 错误处理

## 路线图

- [x] 实现基于 CycloneDDS 的 DDS 封装层
- [x] 实现`sub`/`pub`、`service`/`client`通信方式
- [ ] 完善 zenrc DDS 封装层的 API
- [ ] 为各模块添加完整的文档和示例
- [ ] 支持更多行为树节点类型
- [ ] 优化共享内存的零拷贝性能

## 构建

```bash
# 构建所有模块
cargo build --workspace

# 运行测试
cargo test --workspace

# 运行示例
cargo run --example pub_sub -p zenrc          # DDS 发布/订阅
cargo run --example service_client -p zenrc   # 服务/客户端模式
cargo run --example cdr_bridge -p zenrc       # CDR 序列化桥接
cargo run --example loan_read -p zenrc        # 借用读取 API
cargo run --example printonde -p zenrc-bt     # 行为树示例

# 性能基准测试
cargo bench -p zenrc
```

## CycloneDDS 配置

如果要启用基于 iceoryx2 的共享内存消息通道，则需要声明一个XML 配置文件，内容如下：

```xml
<?xml version="1.0" encoding="UTF-8" ?>
<CycloneDDS xmlns="https://cdds.io/config"
			xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
			xsi:schemaLocation="https://cdds.io/config https://raw.githubusercontent.com/eclipse-cyclonedds/cyclonedds/iceoryx/etc/cyclonedds.xsd">
	<Domain id="any">
		<General>
			<Interfaces>
				<PubSubMessageExchange type="iox2" library="psmx_iox2" config="LOG_LEVEL=INFO;"/>
			</Interfaces>
		</General>
	</Domain>
	<Tracing>
		<Verbosity>config</Verbosity>
		<OutputFile>/absolute/path/to/cyclonedds.log</OutputFile>
	</Tracing>
</CycloneDDS>
```

配置项说明：

- `Domain id="any"`：允许 CycloneDDS 在任意 domain id 下复用这份配置。
- `PubSubMessageExchange`：启用 `psmx_iox2` 插件，把支持的 pub/sub 数据路径切到 iceoryx2 共享内存通道。
- `config="LOG_LEVEL=INFO;"`：设置该共享内存插件的日志级别，便于观察初始化过程。
- `Tracing/Verbosity=config`：输出配置级别日志，确认 XML 是否被正确解析。
- `Tracing/OutputFile`：把 CycloneDDS 的追踪日志写到仓库根目录的 `cyclonedds.log`。

请把 `OutputFile` 改成当前机器上的有效绝对路径。

使用方法：

1. 先确认本机已安装 CycloneDDS，并且 `psmx_iox2` 对应插件可被运行时加载。
2. 在运行你的应用、测试或 benchmark 之前，导出配置文件路径：

```bash
export CYCLONEDDS_URI="file:///absolute/path/to/cyclonedds.xml"
```

3. 再执行需要 DDS 环境的命令，例如：

```bash
cargo run -p <your-crate>
```

4. 如果需要确认配置是否生效，查看 `OutputFile` 对应路径下的日志，其中应能看到 `psmx_iox2`、`iox2` 和 `OutputFile` 等配置项被解析的记录。
