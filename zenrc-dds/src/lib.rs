#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]

// ─── 原始 C 绑定（由 bindgen 自动生成）────────────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// ─── 消息代码生成工具 ─────────────────────────────────────────────────────────
pub mod msg_gen;

// ─── 安全 Rust API ────────────────────────────────────────────────────────────

/// 错误类型与 Result 别名
pub mod error;

/// QoS 策略（Builder 模式 + ROS2 预设 Profile）
pub mod qos;

/// DdsMsg trait 与类型化 Topic 句柄
pub mod topic;

/// 域参与者（Domain Participant）——发布者/订阅者的工厂
pub mod domain;

/// 类型化 DDS 写者
pub mod publisher;

/// 类型化 DDS 读者 + 样本包装 + 样本元信息
pub mod subscriber;

/// 等待集（WaitSet）与守护条件（GuardCondition）
pub mod waitset;

// ─── 常用类型的顶层重导出 ──────────────────────────────────────────────────────

pub use domain::DomainParticipant;
pub use error::{DdsError, Result};
pub use publisher::Publisher;
pub use qos::{Durability, History, Liveliness, Ownership, Qos, Reliability};
pub use subscriber::{Sample, SampleInfo, Subscription};
pub use topic::DdsMsg;
pub use waitset::{GuardCondition, WaitSet};

