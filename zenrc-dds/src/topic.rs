use std::marker::PhantomData;

use crate::{dds_entity_t, dds_topic_descriptor_t};

/// DDS 消息类型的 Trait，由代码生成器（或用户）为每个消息结构实现。
///
/// # Safety
/// 实现者必须保证：
/// - `descriptor()` 返回的指针指向一个与 `Self` 内存布局**完全匹配**的
///   `dds_topic_descriptor_t`，且该指针的生命周期为 `'static`。
/// - `Self` 的内存布局符合 CDR 序列化的要求。
/// - `free_contents` 的默认实现使用 `dds_sample_free(DDS_FREE_CONTENTS)`，
///   会释放 DDS 在反序列化时为字符串/序列分配的堆内存。
///   若消息中不含指针成员，实现空函数即可。
pub unsafe trait DdsMsg: Sized {
    /// 返回指向 `dds_topic_descriptor_t` 的不可变裸指针（静态生命周期）
    fn descriptor() -> *const dds_topic_descriptor_t;

    /// 释放 DDS 在 `dds_take` / `dds_read` 期间为该样本分配的内部堆内存
    /// （字符串、动态序列等），但**不**释放 `Self` 本身。
    ///
    /// 默认实现调用 `dds_sample_free(..., DDS_FREE_CONTENTS)`，
    /// 对于纯 POD 类型（无指针成员）可覆盖为空实现以避免 unsafe 调用。
    unsafe fn free_contents(&mut self) {
        unsafe {
            crate::dds_sample_free(
                self as *mut Self as *mut std::ffi::c_void,
                Self::descriptor(),
                crate::dds_free_op_t_DDS_FREE_CONTENTS,
            );
        }
    }
}

/// 已类型化的 DDS Topic 句柄。
///
/// `Topic<T>` 持有 DDS topic 实体；Drop 时自动调用 `dds_delete`。
pub struct Topic<T: DdsMsg> {
    pub(crate) entity: dds_entity_t,
    _marker: PhantomData<T>,
}

impl<T: DdsMsg> Topic<T> {
    /// 从已有的 topic 实体句柄创建（仅限本 crate 内部使用）
    pub(crate) fn from_entity(entity: dds_entity_t) -> Self {
        Self {
            entity,
            _marker: PhantomData,
        }
    }

    /// 返回底层 DDS 实体句柄（用于高级场景）
    pub fn entity(&self) -> dds_entity_t {
        self.entity
    }
}

impl<T: DdsMsg> Drop for Topic<T> {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.entity) };
    }
}

// SAFETY: dds_entity_t 只是一个 i32，DDS 内部线程安全
unsafe impl<T: DdsMsg> Send for Topic<T> {}
unsafe impl<T: DdsMsg> Sync for Topic<T> {}
