use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::ParticipantInner;
use crate::error::{check_ret, Result};
use crate::qos::duration_to_nanos;
use crate::topic::Topic;
use crate::msg_wrapper::RawMessageBridge;
use crate::dds_entity_t;

/// 类型化 DDS 写者（Publisher）。
///
/// 对应 ROS2 的 `rclcpp::Publisher`。持有 DDS writer 实体和对应的 Topic 实体；
/// Drop 时按顺序删除 writer → topic。
///
/// 通过 [`crate::domain::DomainParticipant::create_publisher`] 创建。
pub struct Publisher<T: RawMessageBridge> {
    writer: dds_entity_t,
    topic: Topic<T>,
    _participant: Arc<ParticipantInner>,
    _marker: PhantomData<T>,
}

impl<T: RawMessageBridge> Publisher<T> {
    pub(crate) fn new(
        writer: dds_entity_t,
        topic: Topic<T>,
        participant: Arc<ParticipantInner>,
    ) -> Self {
        Self {
            writer,
            topic,
            _participant: participant,
            _marker: PhantomData,
        }
    }

    // ── 发布 ──────────────────────────────────────────────────────────────────

    /// 发布消息（使用当前时间作为时间戳）
    pub fn publish(&self, msg: T) -> Result<()> {
        let raw = msg.to_raw();
        check_ret(unsafe {
            crate::dds_write(self.writer, &raw as *const _ as *const c_void)
        })
    }

    /// 发布消息并附带自定义时间戳（纳秒，相对 DDS 纪元）
    pub fn publish_with_timestamp(&self, msg: T, timestamp_ns: i64) -> Result<()> {
        let raw = msg.to_raw();
        check_ret(unsafe {
            crate::dds_write_ts(self.writer, &raw as *const _ as *const c_void, timestamp_ns)
        })
    }

    /// 发布消息并附带 `Duration`（从系统启动计算，便于与 `std::time::SystemTime` 结合）
    pub fn publish_with_duration(&self, msg: T, timestamp: Duration) -> Result<()> {
        self.publish_with_timestamp(msg, duration_to_nanos(timestamp))
    }

    /// 返回底层 DDS writer 实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.writer
    }

    /// 返回关联 Topic 的实体句柄
    pub fn topic_entity(&self) -> dds_entity_t {
        self.topic.entity
    }
}

impl<T: RawMessageBridge> Drop for Publisher<T> {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.writer) };
        // topic 由 self.topic (Topic<T>) 的 Drop 自动删除
    }
}

unsafe impl<T: RawMessageBridge> Send for Publisher<T> {}
unsafe impl<T: RawMessageBridge> Sync for Publisher<T> {}
