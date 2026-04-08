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
