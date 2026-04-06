use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::ParticipantInner;
use crate::error::{check_ret, Result};
use crate::qos::duration_to_nanos;
use crate::topic::{DdsMsg, Topic};
use crate::dds_entity_t;

/// 类型化 DDS 写者（Publisher）。
///
/// 对应 ROS2 的 `rclcpp::Publisher`。持有 DDS writer 实体和对应的 Topic 实体；
/// Drop 时按顺序删除 writer → topic。
///
/// 通过 [`crate::domain::DomainParticipant::create_publisher`] 创建。
pub struct Publisher<T: DdsMsg> {
    writer: dds_entity_t,
    topic: Topic<T>,
    _participant: Arc<ParticipantInner>,
    _marker: PhantomData<T>,
}

impl<T: DdsMsg> Publisher<T> {
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
    ///
    /// # 安全性
    /// - `msg` 的内存布局必须与 topic 描述符完全匹配。
    pub fn publish(&self, msg: &T) -> Result<()> {
        check_ret(unsafe {
            crate::dds_write(self.writer, msg as *const T as *const c_void)
        })
    }

    /// 发布消息并附带自定义时间戳（纳秒，相对 DDS 纪元）
    pub fn publish_with_timestamp(&self, msg: &T, timestamp_ns: i64) -> Result<()> {
        check_ret(unsafe {
            crate::dds_write_ts(self.writer, msg as *const T as *const c_void, timestamp_ns)
        })
    }

    /// 发布消息并附带 `Duration`（从系统启动计算，便于与 `std::time::SystemTime` 结合）
    pub fn publish_with_duration(&self, msg: &T, timestamp: Duration) -> Result<()> {
        self.publish_with_timestamp(msg, duration_to_nanos(timestamp))
    }

    /// 写入数据并同时处置实例（write + dispose）
    pub fn writedispose(&self, msg: &T) -> Result<()> {
        check_ret(unsafe {
            crate::dds_writedispose(self.writer, msg as *const T as *const c_void)
        })
    }

    /// 处置实例（dispose），通知订阅者该实例已不再有效
    pub fn dispose(&self, msg: &T) -> Result<()> {
        check_ret(unsafe {
            crate::dds_dispose(self.writer, msg as *const T as *const c_void)
        })
    }

    /// 刷新批量写缓冲区（仅在开启 batch 模式后有效）
    pub fn flush(&self) -> Result<()> {
        check_ret(unsafe { crate::dds_write_flush(self.writer) })
    }

    /// 手动断言写者活跃性（适用于 ManualByTopic Liveliness）
    pub fn assert_liveliness(&self) -> Result<()> {
        check_ret(unsafe { crate::dds_assert_liveliness(self.writer) })
    }

    // ── 实例管理 ──────────────────────────────────────────────────────────────

    /// 注册实例，预分配实例句柄（大量相同键值写入时可提升性能）
    pub fn register_instance(&self, msg: &T) -> Result<u64> {
        let mut handle: crate::dds_instance_handle_t = 0;
        check_ret(unsafe {
            crate::dds_register_instance(
                self.writer,
                &mut handle,
                msg as *const T as *const c_void,
            )
        })?;
        Ok(handle)
    }

    /// 注销实例（按数据键值）
    pub fn unregister_instance(&self, msg: &T) -> Result<()> {
        check_ret(unsafe {
            crate::dds_unregister_instance(self.writer, msg as *const T as *const c_void)
        })
    }

    // ── 状态查询 ──────────────────────────────────────────────────────────────

    /// 获取发布匹配状态（有多少订阅者与该写者匹配）
    pub fn publication_matched_status(
        &self,
    ) -> Result<crate::dds_publication_matched_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe {
            crate::dds_get_publication_matched_status(self.writer, &mut status)
        })?;
        Ok(status)
    }

    /// 获取匹配的订阅者句柄列表
    pub fn matched_subscriptions(&self) -> Result<Vec<crate::dds_instance_handle_t>> {
        const MAX: usize = 64;
        let mut handles = vec![0u64; MAX];
        let ret = unsafe {
            crate::dds_get_matched_subscriptions(
                self.writer,
                handles.as_mut_ptr(),
                MAX,
            )
        };
        let n = crate::error::check_entity(ret)? as usize;
        handles.truncate(n);
        Ok(handles)
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

impl<T: DdsMsg> Drop for Publisher<T> {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.writer) };
        // topic 由 self.topic (Topic<T>) 的 Drop 自动删除
    }
}

unsafe impl<T: DdsMsg> Send for Publisher<T> {}
unsafe impl<T: DdsMsg> Sync for Publisher<T> {}
