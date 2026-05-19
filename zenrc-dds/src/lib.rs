#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]

use std::ffi::c_void;

// ─── 原始 C 绑定（由 bindgen 自动生成）────────────────────────────────────────
#[cfg(not(docsrs))]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// ─── ROS2 消息类型 C 绑定（由 IDL 编译 + bindgen 自动生成）──────────────────
#[cfg(not(docsrs))]
include!(concat!(env!("OUT_DIR"), "/msg_bindings.rs"));

// ─── 安全的 ROS2 消息 Rust 包装类型（由 msg_gen 自动生成）───────────────────
#[cfg(not(docsrs))]
include!(concat!(env!("OUT_DIR"), "/generate_types.rs"));

// ─── docs.rs 构建桩（无 CycloneDDS 环境时提供最小类型定义）──────────────────
#[cfg(docsrs)]
pub use docsrs_stubs::*;
#[cfg(docsrs)]
mod docsrs_stubs {
    use std::ffi::c_void;

    pub type dds_entity_t          = i32;
    pub type dds_time_t            = i64;
    pub type dds_instance_handle_t = u64;
    pub type dds_sample_state_t    = u32;
    pub type dds_view_state_t      = u32;
    pub type dds_instance_state_t  = u32;

    pub const dds_sample_state_DDS_SST_READ:    dds_sample_state_t   = 1;
    pub const dds_view_state_DDS_VST_NEW:       dds_view_state_t     = 1;
    pub const dds_instance_state_DDS_IST_ALIVE: dds_instance_state_t = 1;

    #[repr(C)]
    pub struct dds_topic_descriptor_t { _private: [u8; 0] }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct dds_sample_info_t {
        pub sample_state:       dds_sample_state_t,
        pub view_state:         dds_view_state_t,
        pub instance_state:     dds_instance_state_t,
        pub valid_data:         bool,
        pub source_timestamp:   dds_time_t,
        pub instance_handle:    dds_instance_handle_t,
        pub publication_handle: dds_instance_handle_t,
    }

    #[repr(C)]
    pub struct ddsi_serdata { _private: [u8; 0] }

    pub unsafe extern "C" fn dds_return_loan(
        _reader: dds_entity_t,
        _buf: *mut *mut c_void,
        _bufsz: i32,
    ) -> i32 { 0 }
}

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


// ── wl（租借）样本 ─────────────────────────────────────────────────────────────

/// 持有 DDS 单条租借内存的 RAII 守卫，提供对借用数据的零拷贝引用访问。
///
/// 通过 [`get`](LoanedSample::get) 访问底层 `T::CStruct` 引用。
/// Drop 时自动调用 `dds_return_loan` 归还该条借用内存。
pub struct LoanedSample<T: RawMessageBridge> {
    reader: dds_entity_t,
    ptr: *mut c_void,
    pub info: SampleInfo,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: RawMessageBridge> LoanedSample<T> {
    pub fn new(reader: dds_entity_t, ptr: *mut c_void, info: SampleInfo) -> Self {
        Self {
            reader,
            ptr,
            info,
            _phantom: std::marker::PhantomData,
        }
    }

    /// 返回借用数据的引用。当内部指针为 null 时返回 `None`。
    pub fn get(&self) -> Option<&T::CStruct> {
        if self.ptr.is_null() {
            None
        } else {
            Some(unsafe { &*(self.ptr as *const T::CStruct) })
        }
    }
}

impl<T: RawMessageBridge> Drop for LoanedSample<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dds_return_loan(self.reader, &mut self.ptr, 1);
            }
        }
    }
}

unsafe impl<T: RawMessageBridge> Send for LoanedSample<T> {}
unsafe impl<T: RawMessageBridge> Sync for LoanedSample<T> {}

/// CDR 样本及其元信息。
///
/// 持有 `ddsi_serdata` 指针，所有权由该对象管理。
/// 注意：成功调用 write_cdr/forward_cdr 后，所有权转移给 DDS，不应再手动释放。
#[derive(Debug)]
pub struct CdrSample {
    serdata: *mut ddsi_serdata,
    pub info: SampleInfo,
}

impl CdrSample {
    /// 从原始 ddsi_serdata 指针和样本信息创建 CdrSample（获得所有权）
    pub fn new(serdata: *mut ddsi_serdata, info: SampleInfo) -> Self {
        Self { serdata, info }
    }

    /// 获取内部指针（用于传递给 FFI）
    pub fn as_ptr(&mut self) -> *mut ddsi_serdata {
        self.serdata
    }
}

unsafe impl Send for CdrSample {}
unsafe impl Sync for CdrSample {}