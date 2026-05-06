#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]

// ─── 原始 C 绑定（由 bindgen 自动生成）────────────────────────────────────────
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// ─── ROS2 消息类型 C 绑定（由 IDL 编译 + bindgen 自动生成）──────────────────
include!(concat!(env!("OUT_DIR"), "/msg_bindings.rs"));

// ─── 安全的 ROS2 消息 Rust 包装类型（由 msg_gen 自动生成）───────────────────
include!(concat!(env!("OUT_DIR"), "/generate_types.rs"));

// ─── 消息桥接 trait（内部使用，供生成的安全类型实现）───────────────────────────

pub trait RawMessageBridge: Sized {
    /// 对应的原始 C 消息类型。
    type CStruct;

    /// 获取 DDS 主题描述符指针。
    fn descriptor() -> *const dds_topic_descriptor_t;

    /// 转换为原始类型（消费 self）。
    fn to_raw(self) -> Self::CStruct;

    /// 从原始类型转换回安全类型，并消费该原始值。
    fn from_raw(raw: Self::CStruct) -> Self;

    /// 释放内存（由 DDS 在反序列化时分配的字符串/序列等）。
    fn free_contents(&mut self);
}

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
            was_read: raw.sample_state == dds_sample_state_DDS_SST_READ,
            is_new_view: raw.view_state == dds_view_state_DDS_VST_NEW,
            is_alive: raw.instance_state == dds_instance_state_DDS_IST_ALIVE,
            valid_data: raw.valid_data,
            source_timestamp: raw.source_timestamp,
            instance_handle: raw.instance_handle,
            publication_handle: raw.publication_handle,
        }
    }
}

/// 对从 DDS 取回的样本的 RAII 包装。
///
/// T 是安全的 Rust 类型（实现 RawMessageBridge）。
/// Drop 时自动调用 T::free_contents()，释放内存。
pub struct Sample<T: RawMessageBridge> {
    pub inner: T,
    pub info: SampleInfo,
}

impl<T: RawMessageBridge> Sample<T> {
    /// 消费 Sample，返回消息和元信息
    pub fn into_parts(self) -> (T, SampleInfo) {
        let info = self.info.clone();
        let inner = unsafe { std::ptr::read(&self.inner as *const T) };
        std::mem::forget(self);
        (inner, info)
    }
}

impl<T: RawMessageBridge> std::ops::Deref for Sample<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: RawMessageBridge> std::ops::DerefMut for Sample<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: RawMessageBridge> Drop for Sample<T> {
    fn drop(&mut self) {
        self.inner.free_contents();
    }
}
