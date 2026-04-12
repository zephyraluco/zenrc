use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use super::domain::ParticipantInner;
use super::error::{check_entity, check_ret, Result};
use super::qos::duration_to_nanos;
use super::topic::Topic;
use zenrc_dds::RawMessageBridge;
use zenrc_dds::{dds_entity_t, dds_instance_handle_t};

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
            zenrc_dds::dds_write(self.writer, &raw as *const _ as *const c_void)
        })
    }

    /// 发布消息并附带自定义时间戳（纳秒，相对 DDS 纪元）
    pub fn publish_with_timestamp(&self, msg: T, timestamp_ns: i64) -> Result<()> {
        let raw = msg.to_raw();
        check_ret(unsafe {
            zenrc_dds::dds_write_ts(self.writer, &raw as *const _ as *const c_void, timestamp_ns)
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

    // ── 状态查询 ──────────────────────────────────────────────────────────────

    /// 获取发布匹配状态（有多少订阅者与该写者匹配）
    pub fn get_publication_status(
        &self,
    ) -> Result<zenrc_dds::dds_publication_matched_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe {
            zenrc_dds::dds_get_publication_matched_status(self.writer, &mut status)
        })?;
        Ok(status)
    }

    /// 检查是否有匹配的读者
    pub fn has_readers(&self) -> Result<bool> {
        Ok(self.get_publication_status()?.current_count > 0)
    }

    /// 获取匹配的订阅者句柄列表
    pub fn get_subscriptions(&self) -> Result<Vec<dds_instance_handle_t>> {
        const MAX: usize = 64;
        let mut handles = vec![0u64; MAX];
        let ret = unsafe {
            zenrc_dds::dds_get_matched_subscriptions(self.writer, handles.as_mut_ptr(), MAX)
        };
        let n = check_entity(ret)? as usize;
        handles.truncate(n);
        Ok(handles)
    }
}

impl<T: RawMessageBridge> Drop for Publisher<T> {
    fn drop(&mut self) {
        unsafe { zenrc_dds::dds_delete(self.writer) };
        // topic 由 self.topic (Topic<T>) 的 Drop 自动删除
    }
}

unsafe impl<T: RawMessageBridge> Send for Publisher<T> {}
unsafe impl<T: RawMessageBridge> Sync for Publisher<T> {}
