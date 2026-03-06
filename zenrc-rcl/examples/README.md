# zenrc-rcl 示例

这个目录包含使用 zenrc-rcl 的示例程序。

## std_msgs::String 发布者和订阅者

这是一个简单的发布者-订阅者示例，演示如何使用 RCL 的原始 FFI 绑定。

### 运行示例

首先，确保已经 source ROS2 环境：

```bash
source /opt/ros/<your-distro>/setup.bash  # 例如: humble, jazzy 等
```

#### 运行发布者

在一个终端中运行：

```bash
cargo run --example publisher
```

发布者会每秒发布一条消息到 `chatter` 话题。

#### 运行订阅者

在另一个终端中运行：

```bash
cargo run --example subscriber
```

订阅者会监听 `chatter` 话题并打印接收到的消息。

### 示例说明

- **publisher.rs**: 创建一个 RCL 节点和发布者，每秒发布一条 `std_msgs::String` 消息
- **subscriber.rs**: 创建一个 RCL 节点和订阅者，接收并打印 `std_msgs::String` 消息

这些示例直接使用 zenrc-rcl 提供的 RCL FFI 绑定，不依赖任何其他库。
