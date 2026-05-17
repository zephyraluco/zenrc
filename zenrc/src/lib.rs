//! # zenrc
//!
//! 基于 [CycloneDDS](https://github.com/eclipse-cyclonedds/cyclonedds) 的高层发布/订阅与服务框架。
//!
//! 该 crate 在 `zenrc-dds` 提供的原始 FFI 绑定之上封装了更符合 Rust 惯用法的接口，
//! 包括类型化的发布者（[`dds::publisher::Publisher`]）、订阅者（[`dds::subscriber::Subscription`]）、
//! 服务端/客户端（[`dds::service`]）以及异步事件回调机制。
//!
//! ## 快速上手
//!
//! ```no_run
//! use zenrc::dds::context::{DdsContext, DOMAIN_DEFAULT};
//! use zenrc::dds::qos::Qos;
//! use zenrc::msg::std_msgs::StringMsg;
//!
//! # fn main() -> zenrc::dds::error::Result<()> {
//! let ctx = DdsContext::new(DOMAIN_DEFAULT)?;
//! let pub_ = ctx.create_publisher::<StringMsg>("chatter", Qos::default())?;
//! let sub  = ctx.create_subscriber::<StringMsg>("chatter", Qos::default())?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 模块概览
//!
//! | 模块 | 说明 |
//! |------|------|
//! | [`dds::context`] | 域参与者与共享 WaitSet 上下文 |
//! | [`dds::publisher`] | 类型化写者 |
//! | [`dds::subscriber`] | 类型化读者及异步事件回调 |
//! | [`dds::service`] | 请求/应答服务端与客户端 |
//! | [`dds::qos`] | QoS 策略 Builder |
//! | [`dds::error`] | 错误类型 |
//! | [`msg`] | 重导出的 ROS2 消息类型 |

pub mod dds;
pub mod msg;
