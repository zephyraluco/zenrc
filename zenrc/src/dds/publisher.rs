use std::ffi::c_void;
use std::marker::PhantomData;
use std::time::Duration;

use super::error::{check_entity, check_ret, Result};
use super::qos::duration_to_nanos;
use super::topic::Topic;
use zenrc_dds::{CdrSample, RawMessageBridge};
use zenrc_dds::{dds_entity_t, dds_instance_handle_t};
use super::common::{forward_cdr, write_cdr};

/// 类型化 DDS 写者（Publisher）。
///
/// 对应 ROS2 的 `rclcpp::Publisher`。持有 DDS writer 实体和对应的 Topic 实体；
/// Drop 时按顺序删除 writer → topic。
///
/// 通过 [`DdsContext::create_publisher`](super::context::DdsContext::create_publisher) 创建。
/// 生命周期须短于创建它的 [`DdsContext`](super::context::DdsContext)。
pub struct Publisher<T: RawMessageBridge> {
    writer: dds_entity_t,
    topic: Topic<T>,
    _marker: PhantomData<T>,
}

impl<T: RawMessageBridge> Publisher<T> {
    pub(crate) fn new(
        writer: dds_entity_t,
        topic: Topic<T>,
    ) -> Self {
        Self {
            writer,
            topic,
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

    /// 发布消息并立即刷新（确保消息被发送到网络层，便于性能测试）
    pub fn flush(&self) -> Result<()> {
        check_ret(unsafe {
            zenrc_dds::dds_write_flush(self.writer)
        })
    }

    /// 发布消息并附带 `Duration`（从系统启动计算，便于与 `std::time::SystemTime` 结合）
    pub fn publish_with_duration(&self, msg: T, timestamp: Duration) -> Result<()> {
        let raw = msg.to_raw();
        check_ret(unsafe {
            zenrc_dds::dds_write_ts(self.writer, &raw as *const _ as *const c_void, duration_to_nanos(timestamp))
        })
    }

    /// 返回底层 DDS writer 实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.writer
    }

    /// 返回关联 Topic 的实体句柄
    pub fn topic_entity(&self) -> dds_entity_t {
        self.topic.entity
    }

    /// 写入 CDR 序列化数据（不需要反序列化，直接传递 ddsi_serdata）
    pub fn write_cdr(&self, cdr: CdrSample) -> Result<()> {
        write_cdr(self.writer, cdr)
    }

    /// 转发 CDR 序列化数据（网关/代理场景，保留原始时间戳）
    pub fn forward_cdr(&self, cdr: CdrSample) -> Result<()> {
        forward_cdr(self.writer, cdr)
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
    }
}

unsafe impl<T: RawMessageBridge> Send for Publisher<T> {}
unsafe impl<T: RawMessageBridge> Sync for Publisher<T> {}
