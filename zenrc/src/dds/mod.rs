//! DDS 封装模块。
//!
//! 通过 [`context::DdsContext`] 统一管理域参与者和后台 WaitSet 轮询线程，
//! 使用 [`context::DdsContext::create_publisher`] / [`context::DdsContext::create_subscriber`]
//! 创建类型化的发布者/订阅者。

pub mod common;
pub mod error;
pub mod log;
pub mod qos;
pub mod publisher;
pub mod subscriber;
pub mod topic;
pub mod waitset;
pub mod context;
pub mod service;