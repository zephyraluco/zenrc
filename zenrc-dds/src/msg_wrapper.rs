use crate::dds_topic_descriptor_t;

/// 内部 trait：连接安全消息类型与其原始 C 类型及 DDS 描述符。
/// 由代码生成器自动实现，不应该用户手工实现。
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
