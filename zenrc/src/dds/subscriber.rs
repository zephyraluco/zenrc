use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use zenrc_dds::{
    DDS_ANY_STATE, RawMessageBridge, dds_entity_t, dds_instance_handle_t, dds_sample_info_t,
    dds_time_t,
};

use super::error::{DdsError, Result, check_entity, check_ret};
use super::topic::Topic;

// ─── SampleInfo ────────────────────────────────────────────────────────────────

/// 样本元信息，对应 `dds_sample_info_t`
#[derive(Debug, Clone)]
pub struct SampleInfo {
    /// 是否已被读取过（`DDS_SST_READ`）
    pub was_read: bool,
    /// 是否为首次看到该实例（`DDS_VST_NEW`）
    pub is_new_view: bool,
    /// 实例是否存活
    pub is_alive: bool,
    /// 样本数据是否有效（false 表示纯状态变化通知）
    pub valid_data: bool,
    /// 源端时间戳（纳秒）
    pub source_timestamp: dds_time_t,
    /// 实例句柄
    pub instance_handle: dds_instance_handle_t,
    /// 发布者句柄
    pub publication_handle: dds_instance_handle_t,
}

impl From<dds_sample_info_t> for SampleInfo {
    fn from(raw: dds_sample_info_t) -> Self {
        Self {
            was_read: raw.sample_state == zenrc_dds::dds_sample_state_DDS_SST_READ,
            is_new_view: raw.view_state == zenrc_dds::dds_view_state_DDS_VST_NEW,
            is_alive: raw.instance_state == zenrc_dds::dds_instance_state_DDS_IST_ALIVE,
            valid_data: raw.valid_data,
            source_timestamp: raw.source_timestamp,
            instance_handle: raw.instance_handle,
            publication_handle: raw.publication_handle,
        }
    }
}

// ─── Sample<T> ─────────────────────────────────────────────────────────────────

/// 对从 DDS 取回的样本的 RAII 包装。
///
/// T 是安全的 Rust 类型（实现 RawMessageBridge）。
/// Drop 时自动调用 T::free_contents()，释放内存。
pub struct Sample<T: RawMessageBridge> {
    inner: T,
    info: SampleInfo,
}

impl<T: RawMessageBridge> Sample<T> {
    /// 获取样本元信息
    pub fn info(&self) -> &SampleInfo {
        &self.info
    }

    /// 消费 Sample，返回消息和元信息
    pub fn into_parts(self) -> (T, SampleInfo) {
        let info = self.info.clone();
        let inner = unsafe { std::ptr::read(&self.inner as *const T) };
        std::mem::forget(self);
        (inner, info)
    }
}

impl<T: RawMessageBridge> Deref for Sample<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: RawMessageBridge> DerefMut for Sample<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: RawMessageBridge> Drop for Sample<T> {
    fn drop(&mut self) {
        self.inner.free_contents();
    }
}

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

    /// 创建订阅者并附加到指定 DdsContext 的 WaitSet，支持异步流。
    ///
    /// 由 [`DdsContext::create_subscription`](super::context::DdsContext::create_subscription) 调用。
    pub(crate) fn with_context(
        reader: dds_entity_t,
        topic: Topic<T>,
        context: &super::context::DdsContext,
    ) -> Self {
        #[cfg(feature = "async")]
        let notify = Some(context.attach(reader));
        Self {
            reader,
            topic,
            _marker: PhantomData,
            #[cfg(feature = "async")]
            notify,
        }
    }

    // ── Take：取出并从缓存中移除 ───────────────────────────────────────────────

    /// 取出最多 `max` 条新样本（移除出读者缓存）
    ///
    /// 只返回 `valid_data = true` 的样本。
    pub fn take(&self, max: usize) -> Result<Vec<Sample<T>>> {
        self.take_with_mask(max, DDS_ANY_STATE)
    }

    /// 取出单条最新样本，若无可用样本则返回 `None`
    pub fn take_one(&self) -> Result<Option<Sample<T>>> {
        Ok(self.take(1)?.into_iter().next())
    }

    /// 带状态掩码的 take（`mask` 是 `DDS_*_STATE` 常量的组合）
    pub fn take_with_mask(&self, max: usize, mask: u32) -> Result<Vec<Sample<T>>> {
        self.read_or_take(max, mask, true)
    }

    // ── Read：读取但不移除（标记为已读）────────────────────────────────────────

    /// 读取最多 `max` 条样本（标记为已读，不从缓存中移除）
    pub fn read(&self, max: usize) -> Result<Vec<Sample<T>>> {
        self.read_with_mask(max, DDS_ANY_STATE)
    }

    /// 读取单条最新样本，若无可用样本则返回 `None`
    pub fn read_one(&self) -> Result<Option<Sample<T>>> {
        Ok(self.read(1)?.into_iter().next())
    }

    /// 带状态掩码的 read
    pub fn read_with_mask(&self, max: usize, mask: u32) -> Result<Vec<Sample<T>>> {
        self.read_or_take(max, mask, false)
    }

    // ── Peek：取出但不改变状态 ──────────────────────────────────────────────────

    /// 读取最多 `max` 条样本但不改变样本/实例状态（peek）
    pub fn peek(&self, max: usize) -> Result<Vec<Sample<T>>> {
        let mut raw_samples: Vec<T::CStruct> =
            (0..max).map(|_| unsafe { std::mem::zeroed() }).collect();
        let mut ptrs: Vec<*mut c_void> = raw_samples
            .iter_mut()
            .map(|s| s as *mut T::CStruct as *mut c_void)
            .collect();
        let mut infos: Vec<dds_sample_info_t> = vec![unsafe { std::mem::zeroed() }; max];

        let n = unsafe {
            zenrc_dds::dds_peek(
                self.reader,
                ptrs.as_mut_ptr(),
                infos.as_mut_ptr(),
                max,
                max as u32,
            )
        };

        self.collect_samples(n, raw_samples, infos)
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

    // ── 内部实现 ──────────────────────────────────────────────────────────────

    fn read_or_take(&self, max: usize, mask: u32, take: bool) -> Result<Vec<Sample<T>>> {
        if max == 0 {
            return Ok(Vec::new());
        }

        let mut raw_samples: Vec<T::CStruct> =
            (0..max).map(|_| unsafe { std::mem::zeroed() }).collect();
        let mut ptrs: Vec<*mut c_void> = raw_samples
            .iter_mut()
            .map(|s| s as *mut T::CStruct as *mut c_void)
            .collect();
        let mut infos: Vec<dds_sample_info_t> = vec![unsafe { std::mem::zeroed() }; max];

        let n = unsafe {
            if take {
                zenrc_dds::dds_take_mask(
                    self.reader,
                    ptrs.as_mut_ptr(),
                    infos.as_mut_ptr(),
                    max,
                    max as u32,
                    mask,
                )
            } else {
                zenrc_dds::dds_read_mask(
                    self.reader,
                    ptrs.as_mut_ptr(),
                    infos.as_mut_ptr(),
                    max,
                    max as u32,
                    mask,
                )
            }
        };

        self.collect_samples(n, raw_samples, infos)
    }

    fn collect_samples(
        &self,
        n: i32,
        raw_samples: Vec<T::CStruct>,
        infos: Vec<dds_sample_info_t>,
    ) -> Result<Vec<Sample<T>>> {
        if n < 0 {
            return Err(DdsError::RetCode(n, "dds_take/read failed".into()));
        }
        let n = n as usize;

        let mut result = Vec::with_capacity(n);
        for (raw, raw_info) in raw_samples.into_iter().zip(infos.into_iter()).take(n) {
            if raw_info.valid_data {
                let inner = T::from_raw(raw);
                result.push(Sample {
                    inner,
                    info: SampleInfo::from(raw_info),
                });
            } else {
                let _ = T::from_raw(raw);
            }
        }
        Ok(result)
    }
}

// ─── 异步扩展（feature = "async"）─────────────────────────────────────────────

#[cfg(feature = "async")]
impl<T: RawMessageBridge + Send + 'static> Subscription<T> {
    /// 将订阅转换为异步流，每次有新样本时产出 `Result<Sample<T>>`。
    ///
    /// 调用后 `Subscription` 所有权转移至后台 tokio 任务，流被 drop 时后台任务自动退出。
    /// 由共享 WaitSet（[`DdsContext::init`](super::context::DdsContext::init) 初始化）
    /// 的 `Notify` 驱动，后台无额外轮询线程开销。
    ///
    /// # Panics
    /// 若调用前未执行 `DdsContext::init`，则流会立即结束（`notify` 为 `None`）。
    pub fn into_stream(self, size: usize) -> Result<super::async_stream::SubscriptionStream<T>> {
        use tokio::sync::mpsc;
        let (tx, rx) = mpsc::channel::<Result<Sample<T>>>(size);

        let notify = match self.notify.clone() {
            Some(n) => n,
            None => {
                // 没有 Notify 驱动，无法异步等待，立即返回 None
                return Err(DdsError::NullPtr(
                    "订阅未附加到任何 DdsContext，无法创建异步流".into(),
                ));
            }
        };

        let task = tokio::task::spawn(async move {
            loop {
                if tx.is_closed() {
                    break;
                }
                // 等待共享 WaitSet 触发通知
                notify.notified().await;
                match self.take(size) {
                    Ok(samples) => {
                        for sample in samples {
                            if tx.send(Ok(sample)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });

        Ok(super::async_stream::SubscriptionStream::new(rx, task))
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
