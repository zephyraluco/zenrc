#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use core::slice;
use std::borrow::Cow;
use std::ffi::CStr;
use std::mem;

use quote::{format_ident, quote};

use crate::rosidl_typesupport_introspection_c_field_types::{self, *};
use crate::{
    CONSTANTS_MAP, FUNCTIONS_MAP, rosidl_message_type_support_t,
    rosidl_typesupport_introspection_c__MessageMember,
    rosidl_typesupport_introspection_c__MessageMembers,
};

/// 解析类型支持句柄中的成员信息
pub struct Introspection<'a> {
    pub module: &'a str,
    pub prefix: &'a str,
    pub name: &'a str,
    pub members: &'a [MessageMember],
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
    pub fn from_ptr<'a>(ptr: *const rosidl_message_type_support_t) -> &'a Self {
        unsafe { &*(ptr as *const TypeSupport) }
    }

    pub fn to_introspection(&self) -> Introspection<'_> {
        let type_support_members = MessageMeta::from_ptr(
            self.0.data as *const rosidl_typesupport_introspection_c__MessageMembers,
        );
        let namespace = type_support_members.message_namespace();
        let name = type_support_members.message_name();
        let (module, prefix) = namespace
            .split_once("__")
            .expect("Invalid namespace format");
        Introspection {
            module,
            prefix,
            name,
            members: type_support_members.members(),
        }
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
    pub fn to_rust_type(&self) -> proc_macro2::TokenStream {
        match self {
            MemberType::Bool => quote! { bool },
            MemberType::I8 => quote! { i8 },
            MemberType::I16 => quote! { i16 },
            MemberType::I32 => quote! { i32 },
            MemberType::I64 => quote! { i64 },
            MemberType::U8 => quote! { u8 },
            MemberType::U16 => quote! { u16 },
            MemberType::U32 => quote! { u32 },
            MemberType::U64 => quote! { u64 },
            MemberType::U128 => quote! { u128 },
            MemberType::F32 => quote! { f32 },
            MemberType::F64 => quote! { f64 },
            MemberType::Char => quote! { std::ffi::c_char },
            MemberType::WChar => quote! { u16 },
            MemberType::String => quote! { std::string::String },
            MemberType::WString => quote! { std::string::String },
            MemberType::Message => quote! { message },
        }
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
            rosidl_typesupport_introspection_c__ROS_TYPE_UINT8
            | rosidl_typesupport_introspection_c__ROS_TYPE_OCTET => MemberType::U8,
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
    pub fn from_ptr<'a>(
        ptr: *const rosidl_typesupport_introspection_c__MessageMembers,
    ) -> &'a Self {
        unsafe { &*(ptr as *const MessageMeta) }
    }
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
            let member = slice::from_raw_parts(self.0.members_, self.member_count());
            mem::transmute(member)
        }
    }
}

/// rosidl_typesupport_introspection_c__MessageMember 的安全包装类
#[repr(transparent)]
pub struct MessageMember(rosidl_typesupport_introspection_c__MessageMember);

impl MessageMember {
    pub fn from_ptr<'a>(ptr: *const rosidl_typesupport_introspection_c__MessageMember) -> &'a Self {
        unsafe { &*(ptr as *const MessageMember) }
    }
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
        if self.type_id() == MemberType::String {
            Some(self.0.string_upper_bound_ as usize)
        } else {
            None
        }
    }

    pub fn get_ts_ptr(&self) -> Option<&TypeSupport> {
        if self.type_id() == MemberType::Message {
            Some(TypeSupport::from_ptr(self.0.members_))
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

/// 混淆 Rust 关键字和非法字符
pub fn rust_mangle<'a>(name: &'a str) -> Cow<'a, str> {
    if name.contains('@')
        || name.contains('?')
        || name.contains('$')
        || matches!(
            name,
            "abstract"
                | "alignof"
                | "as"
                | "async"
                | "await"
                | "become"
                | "box"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "do"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "final"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "macro"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "offsetof"
                | "override"
                | "priv"
                | "proc"
                | "pub"
                | "pure"
                | "ref"
                | "return"
                | "Self"
                | "self"
                | "sizeof"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "try"
                | "type"
                | "typeof"
                | "unsafe"
                | "unsized"
                | "use"
                | "virtual"
                | "where"
                | "while"
                | "yield"
                | "str"
                | "bool"
                | "f32"
                | "f64"
                | "usize"
                | "isize"
                | "u128"
                | "i128"
                | "u64"
                | "i64"
                | "u32"
                | "i32"
                | "u16"
                | "i16"
                | "u8"
                | "i8"
                | "_"
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

/// 生成字段类型的 TokenStream 和 serde 属性
fn generate_struct_field(member: &MessageMember) -> proc_macro2::TokenStream {
    let c_field_name = member.name();
    let field_name = member.rust_name();
    let field_type = member.type_id();
    let field_ident = format_ident!("{}", field_name);

    // 如果是嵌套消息类型，需要递归生成 Rust 结构体
    let field_type_stream = if let Some(m_ts_ptr) = member.get_ts_ptr() {
        let m_intro = m_ts_ptr.to_introspection();
        let m_module_ident = format_ident!("{}", m_intro.module);
        let m_prefix_ident = format_ident!("{}", m_intro.prefix);
        let m_name_ident = format_ident!("{}", m_intro.name);
        if m_intro.prefix == "action" {
            if let Some((r#type, suffix)) = m_intro.name.rsplit_once("_") {
                let type_ident = format_ident!("{}", r#type);
                let suffix_ident = format_ident!("{}", suffix);
                quote! { #m_module_ident :: #m_prefix_ident :: #type_ident :: #suffix_ident }
            } else {
                quote! { #m_module_ident :: #m_prefix_ident :: #m_name_ident }
            }
        } else {
            quote! { #m_module_ident :: #m_prefix_ident :: #m_name_ident }
        }
    } else {
        // 其他基本类型直接转换
        field_type.to_rust_type()
    };

    let field = if member.is_array() {
        quote! { pub #field_ident : Vec< #field_type_stream > }
    } else {
        quote! { pub #field_ident : #field_type_stream }
    };

    let attr = if field_name != c_field_name {
        Some(quote! { #[serde(rename = #c_field_name )] })
    } else {
        None
    };
    quote! {
        #attr
        #field
    }
    // (field, attr)
}

/// 生成 from_native 函数中的字段转换代码
fn generate_from_native_field(member: &MessageMember) -> proc_macro2::TokenStream {
    let field_name = member.rust_name();
    let field_type = member.type_id();
    let field_ident = format_ident!("{}", field_name);

    if let Some(size) = member.array_size() {
        // 如果是数组类型，并且有固定大小，生成固定大小数组
        if size > 0 && !member.is_upper_bound() {
            match field_type {
                MemberType::Message => {
                    let m_intro = member.get_ts_ptr().unwrap().to_introspection();
                    let m_module_ident = format_ident!("{}", m_intro.module);
                    let m_prefix_ident = format_ident!("{}", m_intro.prefix);
                    let m_name_ident = format_ident!("{}", m_intro.name);
                    quote! {
                        #field_ident: {
                            let vec: Vec<_> = msg
                                .#field_ident
                                .iter()
                                .map(|s| #m_module_ident::#m_prefix_ident::#m_name_ident::from_native(s))
                                .collect();
                            vec
                        },
                    }
                }
                MemberType::String | MemberType::WString => {
                    quote! {
                        #field_ident: msg.#field_ident.iter().map(|s| s.to_str().to_owned()).collect(),
                    }
                }
                _ => {
                    quote! {
                        #field_ident: msg.#field_ident.to_vec(),
                    }
                }
            }
        } else {
            if field_type == MemberType::Message {
                let m_intro = member.get_ts_ptr().unwrap().to_introspection();
                let m_module_ident = format_ident!("{}", m_intro.module);
                let m_prefix_ident = format_ident!("{}", m_intro.prefix);
                let m_name_ident = format_ident!("{}", m_intro.name);

                quote! {
                    #field_ident: {
                        let mut temp = Vec::with_capacity(msg.#field_ident.size);
                        if msg.#field_ident.data != std::ptr::null_mut() {
                            let slice = unsafe {
                                std::slice::from_raw_parts(
                                    msg.#field_ident.data,
                                    msg.#field_ident.size
                                )
                            };
                            for s in slice {
                                temp.push(#m_module_ident::#m_prefix_ident::#m_name_ident::from_native(s));
                            }
                        }
                        temp
                    },
                }
            } else {
                quote! {
                    #field_ident: msg.#field_ident.to_vec(),
                }
            }
        }
    } else {
        match field_type {
            MemberType::String | MemberType::WString => {
                quote! {
                    #field_ident: msg.#field_ident.to_str().to_owned(),
                }
            }
            MemberType::Message => {
                let m_intro = member.get_ts_ptr().unwrap().to_introspection();
                let m_module_ident = format_ident!("{}", m_intro.module);
                let m_prefix_ident = format_ident!("{}", m_intro.prefix);

                // same hack as above to rustify message type names
                if m_intro.prefix == "action" {
                    let (srvname, msgname) =
                        m_intro.name.rsplit_once("_").expect("ooops at from_native");
                    let srvname_ident = format_ident!("{srvname}");
                    let msgname_ident = format_ident!("{msgname}");

                    quote! {
                        #field_ident: #m_module_ident::#m_prefix_ident::#srvname_ident::#msgname_ident::from_native(&msg.#field_ident),
                    }
                } else {
                    let name_ident = format_ident!("{}", m_intro.name);

                    quote! {
                        #field_ident: #m_module_ident::#m_prefix_ident::#name_ident::from_native(&msg.#field_ident),
                    }
                }
            }
            _ => {
                quote! {
                    #field_ident: msg.#field_ident,
                }
            }
        }
    }
}

fn generate_copy_to_native_field(member: &MessageMember) -> proc_macro2::TokenStream {
    let field_name = member.rust_name();
    let field_type = member.type_id();
    let field_ident = format_ident!("{}", field_name);

    if let Some(size) = member.array_size() {
        // 如果是数组类型，并且有固定大小，生成固定大小数组
        if size > 0 && !member.is_upper_bound() {
            match field_type {
                MemberType::Message => {
                    quote! {
                        for (t, s) in msg.#field_ident.iter_mut().zip(&self.#field_ident) {
                            s.copy_to_native(t);
                        }
                    }
                }
                MemberType::String | MemberType::WString => {
                    quote! {
                        for (t, s) in msg.#field_ident.iter_mut().zip(&self.#field_ident) {
                            t.assign(&s);
                        }
                    }
                }
                _ => {
                    quote! {
                        msg.#field_ident.copy_from_slice(&self.#field_ident[..#size]);
                    }
                }
            }
        } else {
            if field_type == MemberType::Message {
                let m_intro = member.get_ts_ptr().unwrap().to_introspection();
                let c_struct = format!("{}__{}__{}", m_intro.module, m_intro.prefix, m_intro.name);
                let init_func_ident = format_ident!("{c_struct}__Sequence__init");
                let fini_func_ident = format_ident!("{c_struct}__Sequence__fini");

                quote! {
                    unsafe {
                        #fini_func_ident(&mut msg.#field_ident);
                        #init_func_ident(&mut msg.#field_ident, self.#field_ident.len());

                        if msg.#field_ident.data != std::ptr::null_mut() {
                            let slice = std::slice::from_raw_parts_mut(msg.#field_ident.data, msg.#field_ident.size);
                            for (t, s) in slice.iter_mut().zip(&self.#field_ident) {
                                s.copy_to_native(t);
                            }
                        }
                    }
                }
            } else {
                quote! {
                    msg.#field_ident.update(&self.#field_ident);
                }
            }
        }
    } else {
        match field_type {
            MemberType::String | MemberType::WString => {
                quote! {
                    msg.#field_ident.assign(&self.#field_ident);
                }
            }
            MemberType::Message => {
                quote! {
                    self.#field_ident.copy_to_native(&mut msg.#field_ident);
                }
            }
            _ => {
                quote! {
                    msg.#field_ident = self.#field_ident;
                }
            }
        }
    }
}

/// 生成 Rust 结构体定义的 TokenStream
pub fn generate_rust_msg(module_: &str, prefix_: &str, name_: &str) -> proc_macro2::TokenStream {
    let key = format!("{}__{}__{}", module_, prefix_, name_);
    let function = FUNCTIONS_MAP.get(key.as_str()).expect("Message not found");

    // 解析类型支持句柄，获取成员信息
    let ts = unsafe { TypeSupport::from_ptr(function()) };
    let Introspection {
        module,
        prefix,
        name,
        members,
    } = ts.to_introspection();

    //? 这里可以使用 `c_struct` 来验证类型支持句柄的结构是否正确
    assert!(
        format!("{}__{}__{}", module, prefix, name) == key,
        "Type support handle does not match expected structure name"
    );

    // 过滤srv和action的特殊命名
    let name = if prefix == "srv" || prefix == "action" {
        // name.rsplit_once("__").map(|(name, _)| name).unwrap_or(name);
        name.split("_")
            .last()
            .expect("Invalid service/action name format")
    } else {
        name
    };

    // 当前成员的名称和类型
    let name_ident = format_ident!("{name}");
    let c_struct_ident = format_ident!("{}__{}__{}", module, prefix, name);

    // 生成字段定义、from_native 和 copy_to_native 转换代码
    let members_data: Vec<_> = members
        .into_iter()
        // 过滤掉 ROS2 中自动添加的占位成员
        .filter(|m| m.rust_name() != "structure_needs_at_least_one_member")
        .map(|member| {
            let fields = generate_struct_field(&member);
            let from_native = generate_from_native_field(&member);
            let copy_to_native = generate_copy_to_native_field(&member);
            (fields, from_native, copy_to_native)
        })
        .collect();

    let fields_vec: Vec<_> = members_data.iter().map(|(f, _, _)| f).collect();
    let from_native_vec: Vec<_> = members_data.iter().map(|(_, fn_code, _)| fn_code).collect();
    let copy_to_native_vec: Vec<_> = members_data
        .iter()
        .map(|(_, _, ctn_code)| ctn_code)
        .collect();

    // 生成 Rust 结构体定义
    let fields = quote! { #(#fields_vec),* };
    let from_native_fields = quote! { #(#from_native_vec)* };
    let copy_to_native_fields = quote! { #(#copy_to_native_vec)* };

    // 生成类型支持包装实现
    let ts_wrapper = {
        let type_support_handle = format_ident!(
            "rosidl_typesupport_c__get_message_type_support_handle__{c_struct_ident}"
        );
        let create_func = format_ident!("{c_struct_ident}__create");
        let destroy_func = format_ident!("{c_struct_ident}__destroy");

        quote! {
            impl WrappedTypesupport for #name_ident {
                type CStruct = #c_struct_ident;

                fn get_ts() -> &'static rosidl_message_type_support_t {
                    unsafe {
                        &* #type_support_handle()
                    }
                }

                fn create_msg() -> *mut #c_struct_ident {
                    unsafe {
                        #create_func ()
                    }
                    #create_func ()
                }

                fn destroy_msg(msg: *mut #c_struct_ident) -> () {
                    unsafe {
                        #destroy_func (msg)
                    };
                    #destroy_func (msg)
                }

                fn from_native(#[allow(unused)] msg: &Self::CStruct) -> #name_ident {
                    #name_ident {
                        #from_native_fields
                    }
                }
                fn copy_to_native(&self, #[allow(unused)] msg: &mut Self::CStruct) {
                    #copy_to_native_fields
                }
            }
        }
    };

    let impl_default = quote! {
        impl Default for #name_ident {
            fn default() -> Self {
                let msg_native = WrappedNativeMsg::< #name_ident >::new();
                #name_ident :: from_native(&msg_native)
            }
        }
    };

    let constant_items: Vec<_> = CONSTANTS_MAP
        .get(&key)
        .cloned()
        .into_iter()
        .flatten()
        .map(|(const_name, typ)| {
            let const_name = format_ident!("{const_name}");
            let value = format_ident!("{key}__{const_name}");
            // 引用类型时需要添加 'static 生命周期
            if let Ok(mut typ) = syn::parse_str::<Box<syn::TypeReference>>(typ) {
                typ.lifetime = Some(syn::Lifetime::new(
                    "'static",
                    proc_macro2::Span::call_site(),
                ));
                quote! { pub const #const_name: #typ = #value; }
            } else if let Ok(typ) = syn::parse_str::<Box<syn::Type>>(typ) {
                quote! { pub const #const_name: #typ = #value; }
            } else {
                quote! {}
            }
        })
        .collect();

    let impl_constants = if constant_items.is_empty() {
        quote! {}
    } else {
        quote! {
            #[allow(non_upper_case_globals)]
            impl #name_ident {
                #(#constant_items)*
            }
        }
    };

    quote! {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct #name_ident {
            #fields
        }

        #impl_constants
        #impl_default
        #ts_wrapper
    }
}

pub fn generate_rust_service(
    module_: &str,
    prefix_: &str,
    name_: &str,
) -> proc_macro2::TokenStream {
    let ident = format_ident!(
        "rosidl_typesupport_c__\
         get_service_type_support_handle__\
         {module_}__\
         {prefix_}__\
         {name_}"
    );

    quote!(
        #[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
        pub struct Service();
        impl WrappedServiceTypeSupport for Service {
            type Request = Request;
            type Response = Response;

            fn get_ts() -> &'static rosidl_service_type_support_t {
                unsafe {
                    &* #ident ()
                }
            }
        }

    )
}
