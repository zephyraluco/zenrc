use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::domain::ParticipantInner;
use crate::error::{check_entity, check_ret, DdsError, Result};
use crate::topic::{DdsMsg, Topic};
use crate::{
    dds_entity_t, dds_instance_handle_t, dds_sample_info_t, dds_time_t,
    DDS_ANY_STATE,
};

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
            was_read: raw.sample_state == crate::dds_sample_state_DDS_SST_READ,
            is_new_view: raw.view_state == crate::dds_view_state_DDS_VST_NEW,
            is_alive: raw.instance_state == crate::dds_instance_state_DDS_IST_ALIVE,
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
/// Drop 时自动调用 [`DdsMsg::free_contents`]，释放 DDS 在反序列化期间
/// 为字符串/序列等分配的内部堆内存。
///
/// 可通过 `Deref`/`DerefMut` 透明访问内部消息类型 `T`。
pub struct Sample<T: DdsMsg> {
    inner: T,
    info: SampleInfo,
}

impl<T: DdsMsg> Sample<T> {
    /// 获取样本元信息
    pub fn info(&self) -> &SampleInfo {
        &self.info
    }

    /// 消费 Sample，返回消息和元信息（调用者负责通过 `DdsMsg::free_contents` 释放内存）
    ///
    /// # Safety
    /// 若消息中含有 DDS 分配的字符串/序列，调用者必须手动调用
    /// `unsafe { msg.free_contents() }` 或确保 msg 随后被正确销毁。
    pub fn into_parts(self) -> (T, SampleInfo) {
        let info = self.info.clone();
        // 取出内部数据（阻止 drop 再次释放）
        let inner =
            unsafe { std::ptr::read(&self.inner as *const T) };
        // 让 Sample 的 drop 不再运行（因为内存已经被 move 走了）
        std::mem::forget(self);
        (inner, info)
    }
}

impl<T: DdsMsg> Deref for Sample<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: DdsMsg> DerefMut for Sample<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: DdsMsg> Drop for Sample<T> {
    fn drop(&mut self) {
        unsafe { self.inner.free_contents() };
    }
}

// ─── Subscription<T> ───────────────────────────────────────────────────────────

/// 类型化 DDS 读者（Subscription）。
///
/// 对应 ROS2 的 `rclcpp::Subscription`。
/// 通过 [`crate::domain::DomainParticipant::create_subscription`] 创建。
pub struct Subscription<T: DdsMsg> {
    reader: dds_entity_t,
    topic: Topic<T>,
    _participant: Arc<ParticipantInner>,
    _marker: PhantomData<T>,
}

impl<T: DdsMsg> Subscription<T> {
    pub(crate) fn new(
        reader: dds_entity_t,
        topic: Topic<T>,
        participant: Arc<ParticipantInner>,
    ) -> Self {
        Self {
            reader,
            topic,
            _participant: participant,
            _marker: PhantomData,
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
        let mut samples: Vec<T> =
            (0..max).map(|_| unsafe { std::mem::zeroed() }).collect();
        let mut ptrs: Vec<*mut c_void> = samples
            .iter_mut()
            .map(|s| s as *mut T as *mut c_void)
            .collect();
        let mut infos: Vec<dds_sample_info_t> =
            vec![unsafe { std::mem::zeroed() }; max];

        let n = unsafe {
            crate::dds_peek(
                self.reader,
                ptrs.as_mut_ptr(),
                infos.as_mut_ptr(),
                max,
                max as u32,
            )
        };

        self.collect_samples(n, samples, infos)
    }

    // ── 等待有数据 ─────────────────────────────────────────────────────────────

    /// 阻塞等待直到有数据可读（超时后返回 Ok(false)）
    pub fn wait_for_data(&self, timeout: std::time::Duration) -> Result<bool> {
        // 创建临时等待集（父实体必须为参与者，不能为 reader）
        let ws = unsafe { crate::dds_create_waitset(self._participant.entity) };
        let ws = check_entity(ws)?;
        let rc = unsafe { crate::dds_waitset_attach(ws, self.reader, self.reader as isize) };
        check_ret(rc)?;

        let timeout_ns = crate::qos::duration_to_nanos(timeout);
        let mut attach: crate::dds_attach_t = 0;
        let n =
            unsafe { crate::dds_waitset_wait(ws, &mut attach, 1, timeout_ns) };

        unsafe { crate::dds_delete(ws) };

        if n == 0 {
            Ok(false)
        } else if n > 0 {
            Ok(true)
        } else {
            Err(DdsError::RetCode(n as i32, "waitset_wait failed".into()))
        }
    }

    // ── 状态查询 ──────────────────────────────────────────────────────────────

    /// 获取订阅匹配状态（有多少发布者与该读者匹配）
    pub fn subscription_matched_status(
        &self,
    ) -> Result<crate::dds_subscription_matched_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe {
            crate::dds_get_subscription_matched_status(self.reader, &mut status)
        })?;
        Ok(status)
    }

    /// 获取样本丢失状态
    pub fn sample_lost_status(&self) -> Result<crate::dds_sample_lost_status_t> {
        let mut status = unsafe { std::mem::zeroed() };
        check_ret(unsafe {
            crate::dds_get_sample_lost_status(self.reader, &mut status)
        })?;
        Ok(status)
    }

    /// 获取匹配的发布者句柄列表
    pub fn matched_publications(&self) -> Result<Vec<dds_instance_handle_t>> {
        const MAX: usize = 64;
        let mut handles = vec![0u64; MAX];
        let ret = unsafe {
            crate::dds_get_matched_publications(self.reader, handles.as_mut_ptr(), MAX)
        };
        let n = check_entity(ret)? as usize;
        handles.truncate(n);
        Ok(handles)
    }

    /// 等待历史数据到达（对 TransientLocal/Transient/Persistent 持久性有效）
    pub fn wait_for_historical_data(
        &self,
        max_wait: std::time::Duration,
    ) -> Result<()> {
        check_ret(unsafe {
            crate::dds_reader_wait_for_historical_data(
                self.reader,
                crate::qos::duration_to_nanos(max_wait),
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

        // 在栈/堆上分配样本缓冲区（zeroed 保证内部指针为 null）
        let mut samples: Vec<T> =
            (0..max).map(|_| unsafe { std::mem::zeroed() }).collect();
        // 收集指向各样本的 *mut c_void 指针
        let mut ptrs: Vec<*mut c_void> = samples
            .iter_mut()
            .map(|s| s as *mut T as *mut c_void)
            .collect();
        let mut infos: Vec<dds_sample_info_t> =
            vec![unsafe { std::mem::zeroed() }; max];

        let n = unsafe {
            if take {
                crate::dds_take_mask(
                    self.reader,
                    ptrs.as_mut_ptr(),
                    infos.as_mut_ptr(),
                    max,
                    max as u32,
                    mask,
                )
            } else {
                crate::dds_read_mask(
                    self.reader,
                    ptrs.as_mut_ptr(),
                    infos.as_mut_ptr(),
                    max,
                    max as u32,
                    mask,
                )
            }
        };

        self.collect_samples(n, samples, infos)
    }

    fn collect_samples(
        &self,
        n: i32,
        samples: Vec<T>,
        infos: Vec<dds_sample_info_t>,
    ) -> Result<Vec<Sample<T>>> {
        if n < 0 {
            // dds_take/read 返回的负值实际上不会发生（0 表示没有样本），
            // 但为了完整性仍处理
            return Err(DdsError::RetCode(n, "dds_take/read failed".into()));
        }
        let n = n as usize;

        let result = samples
            .into_iter()
            .zip(infos.into_iter())
            .take(n)
            .filter_map(|(inner, raw_info)| {
                if raw_info.valid_data {
                    Some(Sample {
                        inner,
                        info: SampleInfo::from(raw_info),
                    })
                } else {
                    // 无效样本：DDS 仍可能为其分配了内存，需要释放
                    let mut inner = inner;
                    unsafe { inner.free_contents() };
                    None
                }
            })
            .collect();

        Ok(result)
    }
}

impl<T: DdsMsg> Drop for Subscription<T> {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.reader) };
        // topic 由 self.topic 的 Drop 自动删除
    }
}

unsafe impl<T: DdsMsg> Send for Subscription<T> {}
unsafe impl<T: DdsMsg> Sync for Subscription<T> {}
