//! 自动生成的 ROS2 消息和服务类型
//!
//! 此模块包含从 ROS2 消息定义自动生成的 Rust 类型。
//! 所有类型都实现了 `TypesupportWrapper` trait，可以与 ROS2 C API 互操作。

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_imports)]
#![allow(clippy::all)]

use std::ffi::{CStr, CString};

use serde::{Deserialize, Serialize};

use crate::msg_wrapper::{NativeMsgWrapper, ServiceTypeSupportWrapper, TypesupportWrapper};
use crate::*;

/// 为 ROS2 原始类型序列生成辅助方法的宏
///
/// 此宏为 ROS2 C API 中的原始类型序列（如 `rosidl_runtime_c__float32__Sequence`）
/// 生成 Rust 友好的辅助方法，用于在 Rust 切片和 C 序列之间进行转换。
///
/// # 参数
///
/// * `$ctype` - ROS2 C 类型的前缀（如 `rosidl_runtime_c__float32`）
/// * `$element_type` - 对应的 Rust 类型（如 `f32`）
///
/// # 生成的方法
///
/// * `update(&mut self, values: &[$element_type])` - 从 Rust 切片更新序列内容
/// * `to_vec(&self) -> Vec<$element_type>` - 将序列转换为 Rust Vec
macro_rules! primitive_sequence {
    ($ctype:ident, $element_type:ident) => {
        paste::item! {
            // 拼接生成新的类型名称，如 rosidl_runtime_c__float32__Sequence
            impl [<$ctype __Sequence>] {
                /// 从 Rust 切片更新序列内容
                ///
                /// 此方法会先释放现有序列，然后重新初始化并复制新数据。
                ///
                /// # 参数
                ///
                /// * `values` - 要复制到序列中的 Rust 切片
                ///
                /// # Safety
                ///
                /// 内部使用 unsafe 代码调用 ROS2 C API 的 fini/init 函数和内存复制
                pub fn update(&mut self, values: &[$element_type]) {
                    // 释放现有序列
                    unsafe { [<$ctype __Sequence__fini>] (self as *mut _); }
                    // 重新初始化序列，分配新的内存
                    unsafe { [<$ctype __Sequence__init>] (self as *mut _, values.len()); }
                    // 如果内存分配成功，复制数据
                    if self.data != std::ptr::null_mut() {
                        unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), self.data, values.len()); }
                    }
                }

                /// 将序列转换为 Rust Vec
                ///
                /// 此方法会复制序列中的所有元素到一个新的 Vec 中。
                ///
                /// # 返回值
                ///
                /// 包含序列所有元素的 Vec。如果序列为空或未初始化，返回空 Vec。
                ///
                /// # Safety
                ///
                /// 内部使用 unsafe 代码进行内存复制
                pub fn to_vec(&self) -> Vec<$element_type> {
                    // 如果序列未初始化，返回空 Vec
                    if self.data == std::ptr::null_mut() {
                        return Vec::new();
                    }
                    // 预分配足够的容量
                    let mut target = Vec::with_capacity(self.size);
                    unsafe {
                        // 从 C 序列复制数据到 Vec
                        std::ptr::copy_nonoverlapping(self.data, target.as_mut_ptr(), self.size);
                        // 设置 Vec 的长度
                        target.set_len(self.size);
                    }
                    target
                }
            }
        }
    };
}
primitive_sequence!(rosidl_runtime_c__float32, f32);
primitive_sequence!(rosidl_runtime_c__float64, f64);
primitive_sequence!(rosidl_runtime_c__long_double, u128);
primitive_sequence!(rosidl_runtime_c__char, i8);
primitive_sequence!(rosidl_runtime_c__wchar, u16);
primitive_sequence!(rosidl_runtime_c__boolean, bool);
primitive_sequence!(rosidl_runtime_c__octet, u8);
primitive_sequence!(rosidl_runtime_c__uint8, u8);
primitive_sequence!(rosidl_runtime_c__int8, i8);
primitive_sequence!(rosidl_runtime_c__uint16, u16);
primitive_sequence!(rosidl_runtime_c__int16, i16);
primitive_sequence!(rosidl_runtime_c__uint32, u32);
primitive_sequence!(rosidl_runtime_c__int32, i32);
primitive_sequence!(rosidl_runtime_c__uint64, u64);
primitive_sequence!(rosidl_runtime_c__int64, i64);

impl rosidl_runtime_c__String {
    pub fn to_str(&self) -> &str {
        let s = unsafe { CStr::from_ptr(self.data) };
        s.to_str().unwrap_or("")
    }

    pub fn assign(&mut self, data: &str) {
        // 将 Rust 字符串转换为 C 字符串
        let q = CString::new(data).unwrap();
        unsafe {
            // 将 C 字符串的内容复制到 rosidl_runtime_c__String 结构体中
            rosidl_runtime_c__String__assign(self, q.as_ptr());
        }
    }
}
//TODO: rosidl_runtime_c__U16String 需要处理 UTF-16 编码
impl rosidl_runtime_c__U16String {
    pub fn to_str(&self) -> String {
        let slice = unsafe { std::slice::from_raw_parts(self.data, self.size) };
        String::from_utf16_lossy(slice)
    }

    pub fn assign(&mut self, data: &str) {
        // 将 Rust 字符串转换为 UTF-16（含结尾 0）并复制到 rosidl_runtime_c__U16String
        let mut utf16: Vec<u16> = data.encode_utf16().collect();
        // utf16.push(0);
        unsafe {
            rosidl_runtime_c__U16String__assignn(self, utf16.as_ptr(), utf16.len());
        }
    }
}
// 包含所有生成的消息类型
pub mod builtin_interfaces {
    use serde::{Deserialize, Serialize};
    use crate::msg_wrapper::{NativeMsgWrapper, TypesupportWrapper};
    use crate::*;

    // 为了让生成的代码能找到其他包的类型
    use crate::generated::builtin_interfaces;

    include!("../generated_types/builtin_interfaces.rs");
}

pub mod std_msgs {
    use serde::{Deserialize, Serialize};
    use crate::msg_wrapper::{NativeMsgWrapper, TypesupportWrapper};
    use crate::*;

    // 为了让生成的代码能找到其他包的类型
    use crate::generated::{builtin_interfaces, std_msgs};

    include!("../generated_types/std_msgs.rs");
}
