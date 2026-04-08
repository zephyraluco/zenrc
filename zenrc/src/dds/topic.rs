use std::marker::PhantomData;

use zenrc_dds::dds_entity_t;

/// 已类型化的 DDS Topic 句柄（仅内部使用）。
///
/// 用于关联 C 原始消息类型与其 DDS topic 实体。
pub struct Topic<T> {
    pub(crate) entity: dds_entity_t,
    _marker: PhantomData<T>,
}

impl<T> Topic<T> {
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

impl<T> Drop for Topic<T> {
    fn drop(&mut self) {
        unsafe { zenrc_dds::dds_delete(self.entity) };
    }
}

// SAFETY: dds_entity_t 只是一个 i32，DDS 内部线程安全
unsafe impl<T> Send for Topic<T> {}
unsafe impl<T> Sync for Topic<T> {}
