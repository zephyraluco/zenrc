#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use core::slice;
use std::{borrow::Cow, ffi::CStr, mem};
use crate::{FUNCTIONS_MAP, rosidl_message_type_support_t, rosidl_typesupport_introspection_c__MessageMember, rosidl_typesupport_introspection_c__MessageMember_s, rosidl_typesupport_introspection_c__MessageMembers, rosidl_typesupport_introspection_c_field_types::{self, *}};

use quote::{format_ident, quote};


/// 解析类型支持句柄中的成员信息
pub struct Introspection<'a> {
    pub module: &'a str,
    pub prefix: &'a str,
    pub name: &'a str,
    pub members: &'a [rosidl_typesupport_introspection_c__MessageMember],
}
impl<'a> Introspection<'a> {
    pub fn name(&self) -> String {
        format!("{}__{}__{}", self.module, self.prefix, self.name)
    }
}

/// 包装类型支持句柄
#[repr(transparent)]
pub struct TypeSupport(rosidl_message_type_support_t);
impl TypeSupport {
    pub unsafe fn from_ptr(ptr: *const rosidl_message_type_support_t) -> Self {
        TypeSupport(*ptr)
    }

    pub unsafe fn to_introspection(&self) -> Introspection {
        let type_support_members = self.0.data as *const rosidl_typesupport_introspection_c__MessageMembers;
        let namespace = CStr::from_ptr((*type_support_members).message_namespace_).to_str().unwrap();
        let name = CStr::from_ptr((*type_support_members).message_name_).to_str().unwrap();
        let (module, prefix) = namespace.split_once("__").expect("Invalid namespace format");
        let member_slice = slice::from_raw_parts((*type_support_members).members_, (*type_support_members).member_count_ as usize);
        Introspection { module, prefix, name, members: member_slice }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberType {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    //? ROS2 中没有直接对应的 128 位无符号整数类型，但我们可以将其映射到 Rust 的 `u128` 类型
    U128,
    F32,
    F64,
    Char,
    WChar,
    String,
    WString,
    Message,
}

impl MemberType {
    pub fn from_type_id(id: u8) -> Option<Self> {
        Some(match id {
            1 => Self::F32,
            2 => Self::F64,
            3 => Self::U128,
            4 => Self::Char,
            5 => Self::WChar,
            6 => Self::Bool,
            7 | 8 => Self::U8,
            9 => Self::I8,
            10 => Self::U16,
            11 => Self::I16,
            12 => Self::U32,
            13 => Self::I32,
            14 => Self::U64,
            15 => Self::I64,
            16 => Self::String,
            17 => Self::WString,
            18 => Self::Message,
            _ => return None,
        })
    }
}
impl From<rosidl_typesupport_introspection_c_field_types> for MemberType {
    fn from(value: rosidl_typesupport_introspection_c_field_types) -> Self {
        match value {
            rosidl_typesupport_introspection_c__ROS_TYPE_BOOLEAN => MemberType::Bool,
            rosidl_typesupport_introspection_c__ROS_TYPE_INT8 => MemberType::I8,
            rosidl_typesupport_introspection_c__ROS_TYPE_INT16 => MemberType::I16,
            rosidl_typesupport_introspection_c__ROS_TYPE_INT32 => MemberType::I32,
            rosidl_typesupport_introspection_c__ROS_TYPE_INT64 => MemberType::I64,
            rosidl_typesupport_introspection_c__ROS_TYPE_UINT8 | rosidl_typesupport_introspection_c__ROS_TYPE_OCTET => MemberType::U8,
            rosidl_typesupport_introspection_c__ROS_TYPE_UINT16 => MemberType::U16,
            rosidl_typesupport_introspection_c__ROS_TYPE_UINT32 => MemberType::U32,
            rosidl_typesupport_introspection_c__ROS_TYPE_UINT64 => MemberType::U64,
            rosidl_typesupport_introspection_c__ROS_TYPE_LONG_DOUBLE => MemberType::U128,
            rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT => MemberType::F32,
            rosidl_typesupport_introspection_c__ROS_TYPE_DOUBLE => MemberType::F64,
            rosidl_typesupport_introspection_c__ROS_TYPE_CHAR => MemberType::Char,
            rosidl_typesupport_introspection_c__ROS_TYPE_WCHAR => MemberType::WChar,
            rosidl_typesupport_introspection_c__ROS_TYPE_STRING => MemberType::String,
            rosidl_typesupport_introspection_c__ROS_TYPE_WSTRING => MemberType::WString,
            rosidl_typesupport_introspection_c__ROS_TYPE_MESSAGE => MemberType::Message,
        }
    }
}

impl Into<rosidl_typesupport_introspection_c_field_types> for MemberType {
    fn into(self) -> rosidl_typesupport_introspection_c_field_types {
        match self {
            MemberType::Bool => rosidl_typesupport_introspection_c__ROS_TYPE_BOOLEAN,
            MemberType::I8 => rosidl_typesupport_introspection_c__ROS_TYPE_INT8,
            MemberType::I16 => rosidl_typesupport_introspection_c__ROS_TYPE_INT16,
            MemberType::I32 => rosidl_typesupport_introspection_c__ROS_TYPE_INT32,
            MemberType::I64 => rosidl_typesupport_introspection_c__ROS_TYPE_INT64,
            MemberType::U8 => rosidl_typesupport_introspection_c__ROS_TYPE_UINT8,
            MemberType::U16 => rosidl_typesupport_introspection_c__ROS_TYPE_UINT16,
            MemberType::U32 => rosidl_typesupport_introspection_c__ROS_TYPE_UINT32,
            MemberType::U64 => rosidl_typesupport_introspection_c__ROS_TYPE_UINT64,
            MemberType::U128 => rosidl_typesupport_introspection_c__ROS_TYPE_LONG_DOUBLE,
            MemberType::F32 => rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT,
            MemberType::F64 => rosidl_typesupport_introspection_c__ROS_TYPE_DOUBLE,
            MemberType::Char => rosidl_typesupport_introspection_c__ROS_TYPE_CHAR,
            MemberType::WChar => rosidl_typesupport_introspection_c__ROS_TYPE_WCHAR,
            MemberType::String => rosidl_typesupport_introspection_c__ROS_TYPE_STRING,
            MemberType::WString => rosidl_typesupport_introspection_c__ROS_TYPE_WSTRING,
            MemberType::Message => rosidl_typesupport_introspection_c__ROS_TYPE_MESSAGE,
        }
    }
}

#[repr(transparent)]
pub struct MessageMeta(rosidl_typesupport_introspection_c__MessageMembers);

impl MessageMeta {
    pub fn message_namespace(&self) -> &str {
        unsafe { CStr::from_ptr(self.0.message_namespace_).to_str().unwrap() }
    }
    pub fn message_name(&self) -> &str {
        unsafe { CStr::from_ptr(self.0.message_name_).to_str().unwrap() }
    }
    pub fn member_count(&self) -> usize {
        self.0.member_count_ as usize
    }
    pub fn size_of(&self) -> usize {
        self.0.size_of_
    }
    pub fn members(&self) -> &[MessageMember] {
        unsafe { 
            let member= slice::from_raw_parts(self.0.members_, self.member_count());
            mem::transmute(member)
        }
    }
}

/// rosidl_typesupport_introspection_c__MessageMember 的安全包装类
#[repr(transparent)]
pub struct MessageMember(rosidl_typesupport_introspection_c__MessageMember);

impl MessageMember {
    /// 成员名称
    pub fn name(&self) -> &str {
        unsafe { CStr::from_ptr(self.0.name_).to_str().unwrap() }
    }
    /// Rust 版本的成员名称
    pub fn rust_name(&self) -> Cow<'_, str> {
        rust_mangle(self.name())
    }

    pub fn type_id(&self) -> MemberType {
        MemberType::from_type_id(self.0.type_id_).unwrap()
    }

    pub fn string_upper_bound(&self) -> Option<usize> {
        if self.type_id() ==  MemberType::String {
            Some(self.0.string_upper_bound_ as usize)
        } else {
            None
        }
    }

    pub fn is_array(&self) -> bool {
        self.0.is_array_
    }

    pub fn array_size(&self) -> Option<usize> {
        if self.0.is_array_ {
            Some(self.0.array_size_)
        } else {
            None
        }
    }

    /// 是否有最大长度限制
    pub fn is_upper_bound(&self) -> bool {
        self.0.is_upper_bound_
    }

    /// 字节偏移
    pub fn offset(&self) -> usize {
        self.0.offset_ as usize
    }
}


/// 获取消息类型支持句柄并解析成员信息
unsafe fn get_message_type_support_handle<'a>(ptr: *const rosidl_message_type_support_t) -> (String,&'a [rosidl_typesupport_introspection_c__MessageMember]) {
    let type_support_members = (*ptr).data as *const rosidl_typesupport_introspection_c__MessageMembers;
    let namespace = CStr::from_ptr((*type_support_members).message_namespace_).to_str().unwrap();
    let name = CStr::from_ptr((*type_support_members).message_name_).to_str().unwrap();
    let (module, prefix) = namespace.split_once("__").expect("Invalid namespace format");
    let c_struct = format!("{module}__{prefix}__{name}");
    let member_slice = slice::from_raw_parts((*type_support_members).members_, (*type_support_members).member_count_ as usize);
    (c_struct, member_slice)
}

/// 混淆 Rust 关键字和非法字符
 pub fn rust_mangle<'a>(name: &'a str) -> Cow<'a, str> {
        if name.contains('@') ||
            name.contains('?') ||
            name.contains('$') ||
            matches!(
                name,
                "abstract" | "alignof" | "as" | "async" | "await" | "become" |
                    "box" | "break" | "const" | "continue" | "crate" | "do" |
                    "dyn" | "else" | "enum" | "extern" | "false" | "final" |
                    "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" |
                    "macro" | "match" | "mod" | "move" | "mut" | "offsetof" |
                    "override" | "priv" | "proc" | "pub" | "pure" | "ref" |
                    "return" | "Self" | "self" | "sizeof" | "static" |
                    "struct" | "super" | "trait" | "true" | "try" | "type" | "typeof" |
                    "unsafe" | "unsized" | "use" | "virtual" | "where" |
                    "while" | "yield" | "str" | "bool" | "f32" | "f64" |
                    "usize" | "isize" | "u128" | "i128" | "u64" | "i64" |
                    "u32" | "i32" | "u16" | "i16" | "u8" | "i8" | "_"
            )
        {
            let mut s = name.to_owned();
            s = s.replace('@', "_");
            s = s.replace('?', "_");
            s = s.replace('$', "_");
            s.push('_');
            return Cow::Owned(s);
        }
        Cow::Borrowed(name)
    }

pub fn generate_rust_msg(module: &str, prefix: &str, name: &str) -> proc_macro2::TokenStream {
    let tokens = proc_macro2::TokenStream::new();
    let key = format!("{}__{}__{}", module, prefix, name);
    let function = FUNCTIONS_MAP.get(key.as_str()).expect("Message not found");
    
    let ts_ptr = unsafe { function() };
    let (c_struct, c_members) = unsafe { get_message_type_support_handle(ts_ptr) };
    
    //? 这里可以使用 `c_struct` 来验证类型支持句柄的结构是否正确
    assert!(format!("{}__{}__{}", module, prefix, name) == c_struct, "Type support handle does not match expected structure name");

    //?  暂时仅生成msg
    if prefix != "msg" {
        panic!("Only message types are supported for now");
    }
    let name = format_ident!("{name}");
    let c_struct_ident = format_ident!("{c_struct}");

    tokens
}