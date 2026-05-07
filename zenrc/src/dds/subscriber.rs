use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use zenrc_dds::{
    RawMessageBridge, Sample, dds_entity_t, dds_instance_handle_t,
};

use super::common::{
    take_one,
};
use super::error::{DdsError, Result, check_entity, check_ret};
use super::topic::Topic;

// ─── Subscription<T> ───────────────────────────────────────────────────────────

/// 类型化 DDS 读者（Subscription），使用安全类型 T。
///
/// T 是一个实现 RawMessageBridge 的 Rust 类型。
/// 内部工作于 T::CStruct（C 原始类型），对用户透明地转换为 T。
pub struct Subscription<T: RawMessageBridge> {
    reader: dds_entity_t,
    topic: Topic<T>,
    _marker: PhantomData<T>,
    /// 异步通知句柄；None 表示该订阅不属于任何 DdsContext
    #[cfg(feature = "async")]
    notify: Option<Arc<tokio::sync::Notify>>,
}

impl<T: RawMessageBridge> Subscription<T> {
    pub(crate) fn new(reader: dds_entity_t, topic: Topic<T>) -> Self {
        Self {
            reader,
            topic,
            _marker: PhantomData,
            #[cfg(feature = "async")]
            notify: None,
        }
    }

    /// 创建订阅者并附加到指定 DdsContext 的 WaitSet，支持事件回调。
    ///
    /// 由 [`DdsContext::create_subscription`](super::context::DdsContext::create_subscription) 调用。
    pub(crate) fn with_context(
        &mut self,
        context: &super::context::DdsContext,
    ) {
        #[cfg(feature = "async")]
        let notify = Some(context.attach(self.reader));
        self.notify = notify;
    }

    // ── 状态查询 ──────────────────────────────────────────────────────────────

    /// 获取订阅匹配状态（有多少发布者与该读者匹配）
    pub fn subscription_matched_status(
        &self,
    ) -> Result<zenrc_dds::dds_subscription_matched_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe {
            zenrc_dds::dds_get_subscription_matched_status(self.reader, &mut status)
        })?;
        Ok(status)
    }

    /// 获取样本丢失状态
    pub fn sample_lost_status(&self) -> Result<zenrc_dds::dds_sample_lost_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe { zenrc_dds::dds_get_sample_lost_status(self.reader, &mut status) })?;
        Ok(status)
    }

    /// 获取匹配的发布者句柄列表
    pub fn matched_publications(&self) -> Result<Vec<dds_instance_handle_t>> {
        const MAX: usize = 64;
        let mut handles = vec![0u64; MAX];
        let ret = unsafe {
            zenrc_dds::dds_get_matched_publications(self.reader, handles.as_mut_ptr(), MAX)
        };
        let n = check_entity(ret)? as usize;
        handles.truncate(n);
        Ok(handles)
    }

    /// 等待历史数据到达（对 TransientLocal/Transient/Persistent 持久性有效）
    pub fn wait_for_historical_data(&self, max_wait: std::time::Duration) -> Result<()> {
        check_ret(unsafe {
            zenrc_dds::dds_reader_wait_for_historical_data(
                self.reader,
                super::qos::duration_to_nanos(max_wait),
            )
        })
    }

    /// 返回底层 DDS reader 实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.reader
    }

    /// 返回关联 Topic 的实体句柄
    pub fn topic_entity(&self) -> dds_entity_t {
        self.topic.entity
    }

    /// 在给定超时时间内等待下一条样本并返回。
    pub async fn next(&self, timeout: Duration) -> Result<Sample<T>> {
        let notify = match self.notify.clone() {
            Some(n) => n,
            None => {
                return Err(DdsError::NullPtr(
                    "订阅未附加到任何 DdsContext，无法等待下一条样本".into(),
                ));
            }
        };

        // 先快路径尝试一次，避免错过已到达但尚未消费的数据。
        if let Some(sample) = take_one::<T>(self.reader)? {
            return Ok(sample);
        }

        let reader = self.reader;
        let wait_fut = async {
            loop {
                notify.notified().await;

                if let Some(sample) = take_one::<T>(reader)? {
                    return Ok(sample);
                }
            }
        };

        match tokio::time::timeout(timeout, wait_fut).await {
            Ok(res) => res,
            Err(_) => Err(DdsError::Timeout("等待订阅样本超时".into())),
        }
    }

}

// ─── 异步扩展（feature = "async"）─────────────────────────────────────────────

#[cfg(feature = "async")]
impl<T: RawMessageBridge + Send + 'static> Subscription<T> {
    /// 注册事件回调：当共享 WaitSet 的 notify 被唤醒时，在 tokio 任务中处理所有新样本。
    pub fn set_event<F>(&self, handler: F) -> Result<tokio::task::JoinHandle<()>>
    where
        F: Fn(Sample<T>) + Send + Sync + 'static,
    {
        let notify = match self.notify.clone() {
            Some(n) => n,
            None => {
                return Err(DdsError::NullPtr(
                    "订阅未附加到任何 DdsContext，无法设置事件回调".into(),
                ));
            }
        };

        let reader = self.reader;
        let handler = Arc::new(handler);

        Ok(tokio::task::spawn(async move {
            loop {
                notify.notified().await;

                loop {
                    match take_one::<T>(reader) {
                        Ok(Some(sample)) => (handler)(sample),
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }))
    }
}

impl<T: RawMessageBridge> Drop for Subscription<T> {
    fn drop(&mut self) {
        // 直接删除 reader 实体；后台线程会在下一轮循环检测到 reader 已失效，
        // 自动将对应 ReadCondition 从 WaitSet 上移除并释放
        unsafe { zenrc_dds::dds_delete(self.reader) };
    }
}

unsafe impl<T: RawMessageBridge> Send for Subscription<T> {}
unsafe impl<T: RawMessageBridge> Sync for Subscription<T> {}
