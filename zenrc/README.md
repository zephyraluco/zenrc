# zenrc

基于 [CycloneDDS](https://github.com/eclipse-cyclonedds/cyclonedds) 的高层发布/订阅与服务框架。

该 crate 在 `zenrc-dds` 提供的原始 FFI 绑定之上封装了更符合 Rust 惯用法的接口，包括类型化的发布者、订阅者、服务端/客户端以及异步事件回调机制。

## 功能概览

| 模块              | 说明                          |
| ----------------- | ----------------------------- |
| `dds::context`    | 域参与者与共享 WaitSet 上下文 |
| `dds::publisher`  | 类型化写者                    |
| `dds::subscriber` | 类型化读者及异步事件回调      |
| `dds::service`    | 请求/应答服务端与客户端       |
| `dds::qos`        | QoS 策略 Builder              |
| `dds::error`      | 错误类型                      |
| `msg`             | 重导出的 ROS2 消息类型        |

## 快速上手

### 发布/订阅

```rust
use zenrc::dds::context::{DdsContext, DOMAIN_DEFAULT};
use zenrc::dds::qos::Qos;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> zenrc::dds::error::Result<()> {
    let ctx = DdsContext::new(DOMAIN_DEFAULT)?;
    let pub_ = ctx.create_publisher::<std_msgs::msg::String>("chatter", Qos::default())?;
    let sub  = ctx.create_subscriber::<std_msgs::msg::String>("chatter", Qos::default())?;

    sub.set_event(|sample| {
        println!("Received: {}", sample.data);
    })?;

    pub_.publish(std_msgs::msg::String { data: "hello".into() })?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    Ok(())
}
```

### 服务/客户端

```rust
use std::time::Duration;
use zenrc::dds::context::DdsContext;
use zenrc::dds::qos::Qos;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    let server = ctx.create_service::<std_msgs::msg::String, std_msgs::msg::String>(
        "echo_service",
        Qos::services_default(),
    )?;
    let client = ctx.create_client::<std_msgs::msg::String, std_msgs::msg::String>(
        "echo_service",
        Qos::services_default(),
    )?;

    server.set_event(|sample| {
        std_msgs::msg::String { data: sample.data.to_uppercase() }
    })?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let req = std_msgs::msg::String { data: "hello".into() };
    if let Some(reply) = client.call(req, Duration::from_secs(3))? {
        println!("Reply: {}", reply.data);
    }
    Ok(())
}
```

## 示例

```bash
cargo run --example pub_sub      -p zenrc   # 发布/订阅
cargo run --example service_client -p zenrc # 服务/客户端
cargo run --example cdr_bridge   -p zenrc   # CDR 序列化桥接
cargo run --example loan_read    -p zenrc   # 借用读取 API
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
