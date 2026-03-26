#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ops::{Deref, DerefMut};

use crate::{rosidl_message_type_support_t, rosidl_service_type_support_t};

/// ROS2 消息类型支持的包装 trait
///
/// 为 ROS2 消息类型提供统一的类型支持接口，包括：
/// - 获取类型支持句柄
/// - 创建和销毁原生消息
/// - 在 Rust 类型和原生 C 类型之间转换
pub trait TypeSupportWrapper
where
    Self: Sized,
{
    /// 对应的 C 结构体类型
    type CStruct;

    /// 获取消息类型支持句柄
    fn get_ts() -> &'static rosidl_message_type_support_t;

    /// 创建原生消息实例
    fn create_msg() -> *mut Self::CStruct;

    /// 销毁原生消息实例
    fn destroy_msg(msg: *mut Self::CStruct);

    /// 从原生 C 结构体转换为 Rust 类型
    fn from_native(msg: &Self::CStruct) -> Self;

    /// 将 Rust 类型复制到原生 C 结构体
    fn copy_to_native(&self, msg: &mut Self::CStruct);
}

/// ROS2 服务类型支持的包装 trait
///
/// 为 ROS2 服务类型提供统一的类型支持接口
pub trait ServiceTypeSupportWrapper {
    /// 服务请求类型
    type Request: TypeSupportWrapper;
    /// 服务响应类型
    type Response: TypeSupportWrapper;

    /// 获取服务类型支持句柄
    fn get_ts() -> &'static rosidl_service_type_support_t;
}

/// 原生消息的 RAII 包装器
///
/// 自动管理原生 ROS2 消息的生命周期，在创建时分配内存，在销毁时释放内存
pub struct NativeMsg<T: TypeSupportWrapper> {
    msg: *mut T::CStruct,
}

/// 基本方法实现
///
/// 提供创建消息实例和获取指针的核心功能
impl<T: TypeSupportWrapper> NativeMsg<T> {
    pub fn new() -> Self {
        Self {
            msg: T::create_msg(),
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut T::CStruct {
        self.msg
    }

    pub fn as_ptr(&self) -> *const T::CStruct {
        self.msg
    }
}

/// Default trait 实现
///
/// 提供默认构造方式，等同于调用 `new()`
impl<T: TypeSupportWrapper> Default for NativeMsg<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Deref trait 实现
///
/// 允许通过 `*` 运算符或方法调用语法直接访问底层 C 结构体的字段。
/// 这使得可以像使用普通引用一样使用包装器。
impl<T: TypeSupportWrapper> Deref for NativeMsg<T> {
    type Target = T::CStruct;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.msg }
    }
}

/// DerefMut trait 实现
///
/// 允许通过可变引用修改底层 C 结构体的字段。
/// 配合 Deref 实现，提供完整的字段访问能力。
impl<T: TypeSupportWrapper> DerefMut for NativeMsg<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.msg }
    }
}

/// Drop trait 实现
///
/// 实现 RAII 模式，在包装器离开作用域时自动释放原生消息内存。
/// 这确保了即使发生 panic 或提前返回，内存也能被正确清理。
impl<T: TypeSupportWrapper> Drop for NativeMsg<T> {
    fn drop(&mut self) {
        if !self.msg.is_null() {
            T::destroy_msg(self.msg);
        }
    }
}

/// Send trait 实现
unsafe impl<T: TypeSupportWrapper> Send for NativeMsg<T> where T: Send {}

/// Sync trait 实现
unsafe impl<T: TypeSupportWrapper> Sync for NativeMsg<T> where T: Sync {}
