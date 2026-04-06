use std::collections::HashMap;
use std::fs::{self};
use std::path::Path;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

const PRIMS: &[&str] = &[
    "bool", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
];
const MSG_BINDINGS_FILE: &str = "msg_bindings.rs";
const WARPPER_TYPES_FILE: &str = "generate_types.rs";

#[derive(Clone)]
enum SeqType {
    /// 基本数值类型，存储类型名称字符串（如 `"f64"`）。
    Prim(String),
    /// 字符串序列（`*mut *mut c_char` 或 `*mut c_char`）。
    Str,
    /// 嵌套消息序列，存储 `(pkg, cat, name)`。
    Msg(String, String, String),
    /// 无法识别的元素类型，生成器将跳过此字段。
    Unknown,
}

/// C 结构体字段
#[derive(Clone)]
enum CFieldType {
    /// 基本数值类型（bool、整数、浮点），存储 Rust 类型名（如 `"i32"`）。
    Prim(String),
    /// 堆分配字符串指针 `*mut c_char`，在 Rust 侧表示为 `String`。
    /// 反向转换时通过 `CString::into_raw()` 产生堆分配指针，所有权移交给 DDS。
    OwnedStr,
    /// 固定长度字节数组 `[c_char; N]`，在 Rust 侧表示为 `String`。
    /// TokenStream 参数为数组长度的常量表达式（用于生成数组初始化代码）。
    BoundedStr(proc_macro2::TokenStream),
    /// 固定长度基本类型数组 `[T; N]`，在 Rust 侧表示为 `Vec<T>`。
    /// 参数：(元素类型名, 数组长度常量表达式)。
    ArrPrim(String, proc_macro2::TokenStream),
    /// 动态序列（基本数值类型），对应 `dds_sequence_*` 结构体，Rust 侧为 `Vec<T>`。
    /// 参数：(元素类型名, `dds_sequence_*` 类型名)。
    SeqPrim(String, String),
    /// 动态字符串序列，`_buffer` 为 `*mut *mut c_char`，Rust 侧为 `Vec<String>`。
    /// 参数：`dds_sequence_*` 类型名。
    SeqStr(String),
    /// 动态消息序列，`_buffer` 为 `*mut SomeCMsg`，Rust 侧为 `Vec<SafeMsg>`。
    /// 参数：(pkg, cat, name, `dds_sequence_*` 类型名)。
    SeqMsg(String, String, String, String),
    /// 直接嵌套的消息结构体（非指针、非序列），Rust 侧为对应的安全包装类型。
    /// 参数：(pkg, cat, name)。
    NestedMsg(String, String, String),
}

struct MsgStruct {
    /// 包名、类别（msg/srv/action）和消息名，如 ("std_msgs", "msg", "String")。
    pkg: String,
    prefix: String,
    name: String,
    /// 字段列表，包含字段名和类型信息。
    fields: Vec<(String, CFieldType)>,
}

/// 例如 `std::os::raw::c_char` → `"c_char"`。
fn last_segment(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// 判断 `syn::Type` 是否为 `c_char`（即 `*mut c_char` 字符串指针的元素类型）。
fn is_c_char(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => last_segment(&p.path) == "c_char",
        _ => false,
    }
}

/// 解析消息类型名称，提取包名、类别（msg/srv/action）和消息名。
fn parse_msg_name(s: &str) -> Option<(String, String, String)> {
    for (pat, cat) in &[("_msg_", "msg"), ("_srv_", "srv"), ("_action_", "action")] {
        if let Some((pkg, name)) = s.split_once(pat) {
            // 检查拆分后的两部分是否都不为空
            if !pkg.is_empty() && !name.is_empty() {
                return Some((pkg.to_string(), cat.to_string(), name.to_string()));
            }
        }
    }
    None
}

/// 从 `dds_sequence_*` 结构体的 `_buffer` 字段类型推断序列元素类型
fn seq_type_from_buffer(buffer_ty: &syn::Type) -> SeqType {
    // _buffer 字段本身是 *mut ElemType，先剥掉外层指针
    let inner = match buffer_ty {
        syn::Type::Ptr(ptr) => &*ptr.elem,
        _ => return SeqType::Unknown,
    };
    match inner {
        // *mut *mut c_char：字符串序列（每个元素是一个 C 字符串指针）
        syn::Type::Ptr(inner2) if is_c_char(&inner2.elem) => SeqType::Str,
        // *mut c_char：也视为字符串序列（部分 IDL 编译器生成此形式）
        e if is_c_char(e) => SeqType::Str,
        syn::Type::Path(p) => {
            let ty = last_segment(&p.path);
            if PRIMS.contains(&ty.as_str()) {
                SeqType::Prim(ty)
            } else if let Some((pkg, prefix, name)) = parse_msg_name(&ty) {
                SeqType::Msg(pkg, prefix, name)
            } else {
                SeqType::Unknown
            }
        }
        _ => SeqType::Unknown,
    }
}

fn convert_field_type(
    ty: &syn::Type,
    type_map: &HashMap<String, syn::Type>,
    seq_map: &HashMap<String, SeqType>,
) -> Option<CFieldType> {
    Some(match ty {
        // 可变裸指针：仅 *mut c_char 视为堆字符串，其余指针类型无法安全映射
        syn::Type::Ptr(p) if p.mutability.is_some() => {
            if is_c_char(&p.elem) {
                CFieldType::OwnedStr
            } else {
                return None;
            }
        }
        // 固定长度数组：[c_char; N] 为有界字符串，[T; N] 为基本类型数组
        syn::Type::Array(arr) => {
            let len_expr = &arr.len;
            let len_ts = quote::quote! { #len_expr };
            if is_c_char(&arr.elem) {
                CFieldType::BoundedStr(len_ts)
            } else {
                match convert_field_type(&arr.elem, type_map, seq_map)? {
                    CFieldType::Prim(t) => CFieldType::ArrPrim(t, len_ts),
                    _ => return None,
                }
            }
        }
        // 路径类型：涵盖基本类型、序列类型、type alias、嵌套消息
        syn::Type::Path(p) => {
            let name = last_segment(&p.path);
            if PRIMS.contains(&name.as_str()) {
                // bool / 整数 / 浮点
                CFieldType::Prim(name)
            } else if name.starts_with("dds_sequence_") {
                // 动态序列，查预建的 seqs 表确定元素类型
                match seq_map.get(&name) {
                    Some(SeqType::Prim(t)) => CFieldType::SeqPrim(t.clone(), name.clone()),
                    Some(SeqType::Str) => CFieldType::SeqStr(name.clone()),
                    Some(SeqType::Msg(p, c, m)) => {
                        CFieldType::SeqMsg(p.clone(), c.clone(), m.clone(), name.clone())
                    }
                    _ => return None, // SeqType::Unknown，跳过此字段
                }
            } else if let Some(alias_ty) = type_map.get(&name).cloned() {
                // typedef / type alias，递归解析其底层类型
                convert_field_type(&alias_ty, type_map, seq_map)?
            } else if let Some((pkg, cat, n)) = parse_msg_name(&name) {
                // 直接嵌套的消息类型
                CFieldType::NestedMsg(pkg, cat, n)
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

fn generate_struct_field(key: &str, ty: &CFieldType) -> TokenStream {
    let key_ident = format_ident!("{}", key);
    let ty_ts = match ty {
        CFieldType::Prim(t) => {
            let ident = format_ident!("{}", t);
            quote! { #ident }
        }
        CFieldType::OwnedStr | CFieldType::BoundedStr(..) => {
            quote! { ::std::string::String }
        }
        CFieldType::ArrPrim(t, ..) | CFieldType::SeqPrim(t, ..) => {
            let ident = format_ident!("{}", t);
            quote! { ::std::vec::Vec<#ident> }
        }
        CFieldType::SeqStr(..) => quote! { ::std::vec::Vec<::std::string::String> },
        CFieldType::SeqMsg(pkg, prefix, name, ..) => {
            let pkg_ident = format_ident!("{}", pkg);
            let prefix_ident = format_ident!("{}", prefix);
            let mp = match name.split_once('_') {
                Some((parent, sub)) => {
                    let parent_ident = format_ident!("{}", parent);
                    let sub_ident = format_ident!("{}", sub);
                    quote! { crate::#pkg_ident::#prefix_ident::#parent_ident::#sub_ident }
                }
                None => {
                    let name_ident = format_ident!("{}", name);
                    quote! { crate::#pkg_ident::#prefix_ident::#name_ident }
                }
            };
            quote! { ::std::vec::Vec<#mp> }
        }
        CFieldType::NestedMsg(pkg, prefix, name) => {
            let pkg_ident = format_ident!("{}", pkg);
            let prefix_ident = format_ident!("{}", prefix);
            match name.split_once('_') {
                Some((parent, sub)) => {
                    let parent_ident = format_ident!("{}", parent);
                    let sub_ident = format_ident!("{}", sub);
                    quote! { crate::#pkg_ident::#prefix_ident::#parent_ident::#sub_ident }
                }
                None => {
                    let name_ident = format_ident!("{}", name);
                    quote! { crate::#pkg_ident::#prefix_ident::#name_ident }
                }
            }
        }
    };
    quote! { pub #key_ident: #ty_ts }
}

fn generate_from_raw(key: &str, ty: &CFieldType) -> TokenStream {
    let f = format_ident!("{}", key);
    match ty {
        // 基本类型：直接复制
        CFieldType::Prim(..) => quote! { #f: raw.#f },
        // 堆字符串指针：null 返回默认值，否则从 C 字符串转换
        CFieldType::OwnedStr => quote! {
            #f: if raw.#f.is_null() { ::std::default::Default::default() }
                else { unsafe { ::std::ffi::CStr::from_ptr(raw.#f) }.to_string_lossy().into_owned() }
        },
        // 固定字节数组：数组首地址作为 C 字符串指针读取
        CFieldType::BoundedStr(..) => quote! {
            #f: unsafe { ::std::ffi::CStr::from_ptr(raw.#f.as_ptr()) }.to_string_lossy().into_owned()
        },
        // 固定长度基本类型数组：逐元素转型后收集为 Vec
        CFieldType::ArrPrim(..) => quote! { #f: raw.#f.iter().map(|&v| v as _).collect() },
        // 基本类型动态序列：空缓冲区返回空 Vec，否则借用后拷贝
        CFieldType::SeqPrim(..) => quote! {
            #f: if raw.#f._buffer.is_null() { ::std::vec::Vec::new() }
                else { unsafe { ::std::slice::from_raw_parts(raw.#f._buffer, raw.#f._length as usize) }.to_vec() }
        },
        // 字符串动态序列：逐元素将 *mut c_char 转为 String
        CFieldType::SeqStr(..) => quote! {
            #f: (|| {
                if raw.#f._buffer.is_null() { return ::std::vec::Vec::new(); }
                (0..raw.#f._length as usize).map(|__i| {
                    let __p = unsafe { *raw.#f._buffer.add(__i) };
                    if __p.is_null() { ::std::string::String::new() }
                    else { unsafe { ::std::ffi::CStr::from_ptr(__p) }.to_string_lossy().into_owned() }
                }).collect()
            })()
        },
        // 消息动态序列：逐元素递归调用 SafeType::from(&raw_elem)
        CFieldType::SeqMsg(pkg, prefix, name, ..) => {
            let pkg_ident = format_ident!("{}", pkg);
            let prefix_ident = format_ident!("{}", prefix);
            let st = match name.split_once('_') {
                Some((parent, sub)) => {
                    let parent_ident = format_ident!("{}", parent);
                    let sub_ident = format_ident!("{}", sub);
                    quote! { crate::#pkg_ident::#prefix_ident::#parent_ident::#sub_ident }
                }
                None => {
                    let name_ident = format_ident!("{}", name);
                    quote! { crate::#pkg_ident::#prefix_ident::#name_ident }
                }
            };
            quote! {
                #f: (|| {
                    if raw.#f._buffer.is_null() { return ::std::vec::Vec::new(); }
                    (0..raw.#f._length as usize)
                        .map(|__i| #st::from(unsafe { &*raw.#f._buffer.add(__i) }))
                        .collect()
                })()
            }
        }
        // 直接嵌套消息：递归转换
        CFieldType::NestedMsg(pkg, prefix, name) => {
            let pkg_ident = format_ident!("{}", pkg);
            let prefix_ident = format_ident!("{}", prefix);
            let st = match name.split_once('_') {
                Some((parent, sub)) => {
                    let parent_ident = format_ident!("{}", parent);
                    let sub_ident = format_ident!("{}", sub);
                    quote! { crate::#pkg_ident::#prefix_ident::#parent_ident::#sub_ident }
                }
                None => {
                    let name_ident = format_ident!("{}", name);
                    quote! { crate::#pkg_ident::#prefix_ident::#name_ident }
                }
            };
            quote! { #f: #st::from(&raw.#f) }
        }
    }
}

fn generate_into_raw(key: &str, ty: &CFieldType) -> TokenStream {
    let f = format_ident!("{}", key);
    match ty {
        // 基本类型：直接赋值（Copy 语义）
        CFieldType::Prim(..) => quote! { raw.#f = safe.#f; },

        // 堆字符串：过滤内嵌 null 字节后构造 CString，调用 into_raw() 移交所有权
        CFieldType::OwnedStr => quote! {
            raw.#f = {
                let __bytes: ::std::vec::Vec<u8> =
                    safe.#f.into_bytes().into_iter().filter(|&b| b != 0).collect();
                unsafe { ::std::ffi::CString::from_vec_unchecked(__bytes) }.into_raw()
            };
        },

        // 固定字节数组：将字符串截断填充到 [c_char; N]，末尾保留 '\0'
        CFieldType::BoundedStr(len_ts) => quote! {
            raw.#f = {
                let __cstr = ::std::ffi::CString::new(
                    safe.#f.into_bytes().into_iter().filter(|&b| b != 0).collect::<::std::vec::Vec<u8>>()
                ).unwrap_or_default();
                let __bytes = __cstr.as_bytes_with_nul();
                let mut __arr = [0 as ::std::os::raw::c_char; #len_ts];
                for (__i, &__b) in __bytes.iter().take(#len_ts).enumerate() {
                    __arr[__i] = __b as ::std::os::raw::c_char;
                }
                __arr
            };
        },

        // 固定长度基本类型数组：截取 Vec 前 N 个元素填入数组，超出部分丢弃
        CFieldType::ArrPrim(t, len_ts) => {
            let t_ident = format_ident!("{}", t);
            quote! {
                raw.#f = {
                    let mut __arr: [#t_ident; #len_ts] = [Default::default(); #len_ts];
                    for (__i, &__v) in safe.#f.iter().take(#len_ts).enumerate() {
                        __arr[__i] = __v as _;
                    }
                    __arr
                };
            }
        }

        // 基本类型动态序列：将 Vec 的堆缓冲区通过 mem::forget 移交给 DDS 序列结构体
        CFieldType::SeqPrim(t, seq_name) => {
            let t_ident = format_ident!("{}", t);
            let seq_ident = format_ident!("{}", seq_name);
            quote! {
                raw.#f = {
                    let mut __v: ::std::vec::Vec<#t_ident> =
                        safe.#f.into_iter().map(|__x| __x as _).collect();
                    let __len = __v.len() as u32;
                    let __ptr = __v.as_mut_ptr();
                    ::std::mem::forget(__v); // 放弃 Rust 所有权，交给 DDS 释放
                    crate::#seq_ident {
                        _maximum: __len,
                        _length:  __len,
                        _buffer:  __ptr,
                        _release: true, // 告知 DDS 释放消息时调用 dds_free 回收此缓冲区
                    }
                };
            }
        }

        // 字符串动态序列：逐元素 CString::into_raw()，再将指针数组的堆缓冲区移交给 DDS
        CFieldType::SeqStr(seq_name) => {
            let seq_ident = format_ident!("{}", seq_name);
            quote! {
                raw.#f = {
                    let mut __v: ::std::vec::Vec<*mut ::std::os::raw::c_char> = safe.#f
                        .into_iter()
                        .map(|__s| {
                            let __bytes: ::std::vec::Vec<u8> =
                                __s.into_bytes().into_iter().filter(|&b| b != 0).collect();
                            unsafe { ::std::ffi::CString::from_vec_unchecked(__bytes) }.into_raw()
                        })
                        .collect();
                    let __len = __v.len() as u32;
                    let __ptr = __v.as_mut_ptr();
                    ::std::mem::forget(__v);
                    crate::#seq_ident {
                        _maximum: __len,
                        _length:  __len,
                        _buffer:  __ptr,
                        _release: true,
                    }
                };
            }
        }

        // 消息动态序列：逐元素递归转换为原始 C 类型，再将缓冲区移交给 DDS
        CFieldType::SeqMsg(p, c, n, seq_name) => {
            let c_msg_ident = format_ident!("{}_{}_{}", p, c, n);
            let seq_ident = format_ident!("{}", seq_name);
            quote! {
                raw.#f = {
                    let mut __v: ::std::vec::Vec<crate::#c_msg_ident> = safe.#f
                        .into_iter()
                        .map(|__m| crate::#c_msg_ident::from(__m))
                        .collect();
                    let __len = __v.len() as u32;
                    let __ptr = __v.as_mut_ptr();
                    ::std::mem::forget(__v);
                    crate::#seq_ident {
                        _maximum: __len,
                        _length:  __len,
                        _buffer:  __ptr,
                        _release: true,
                    }
                };
            }
        }

        // 直接嵌套消息：递归调用 RawCType::from(safe_field)
        CFieldType::NestedMsg(p, c, n) => {
            let c_msg_ident = format_ident!("{}_{}_{}", p, c, n);
            quote! { raw.#f = crate::#c_msg_ident::from(safe.#f); }
        }
    }
}
/// 生成单个消息类型的安全包装代码，包括结构体定义和 From/Into 实现
fn generate_item_wrapper(s: &MsgStruct, name_ident: proc_macro2::Ident) -> TokenStream {
    let c_name_ident = format_ident!("{}_{}_{}", s.pkg, s.prefix, s.name);
    let (fields_ts, from_fields, into_stmts): (Vec<TokenStream>, Vec<TokenStream>, Vec<TokenStream>) = s
        .fields
        .iter()
        .map(|(fname, kind)| (
            generate_struct_field(fname, kind),
            generate_from_raw(fname, kind),
            generate_into_raw(fname, kind),
        ))
        .fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut fs, mut fr, mut ir), (f, r, i)| {
                fs.push(f);
                fr.push(r);
                ir.push(i);
                (fs, fr, ir)
            },
        );
    quote! {
        #[derive(Debug, Clone, Default)]
        pub struct #name_ident {
            #(#fields_ts,)*
        }
        // 订阅侧：借用原始 C 消息，转换为安全的 Rust 结构体
        impl<'__r> ::std::convert::From<&'__r crate::#c_name_ident> for #name_ident {
            fn from(raw: &'__r crate::#c_name_ident) -> Self {
                Self { #(#from_fields,)* }
            }
        }
        // 发布侧：消费安全结构体，生成原始 C 消息（通过 blanket impl 同时提供 Into）
        impl ::std::convert::From<#name_ident> for crate::#c_name_ident {
            fn from(safe: #name_ident) -> Self {
                let mut raw = unsafe {
                    ::std::mem::MaybeUninit::<crate::#c_name_ident>::zeroed()
                        .assume_init()
                };
                #(#into_stmts)*
                raw
            }
        }
    }
}

/// 生成 Rust 安全包装类型的代码
pub fn generate_rust_wrappers(out_dir: &Path) {
    use std::collections::BTreeMap;

    use syn::Fields;

    let src = match fs::read_to_string(out_dir.join(MSG_BINDINGS_FILE)) {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=Cannot read msg_bindings.rs: {e}");
            return;
        }
    };
    let file = match syn::parse_file(&src) {
        Ok(f) => f,
        Err(e) => {
            println!("cargo:warning=Cannot parse msg_bindings.rs: {e}");
            return;
        }
    };

    // alias 表用于解析如 `unique_identifier_msgs_msg_uint8__16` 之类的 typedef；
    // seqs 表记录每种 dds_sequence_* 的 _buffer 元素类型，供 classify 查询。
    let mut type_map: HashMap<String, syn::Type> = HashMap::new();
    let mut seq_map: HashMap<String, SeqType> = HashMap::new();
    for item in &file.items {
        match item {
            syn::Item::Type(t) => {
                type_map.insert(t.ident.to_string(), (*t.ty).clone());
            }
            syn::Item::Struct(s) if s.ident.to_string().starts_with("dds_sequence_") => {
                if let Fields::Named(fs) = &s.fields {
                    for f in &fs.named {
                        if f.ident.as_ref().map(|i| i == "_buffer").unwrap_or(false) {
                            seq_map.insert(s.ident.to_string(), seq_type_from_buffer(&f.ty));
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 解析消息结构体
    let mut msgs: Vec<MsgStruct> = Vec::new();
    for item in &file.items {
        if let syn::Item::Struct(s) = item {
            let sname = s.ident.to_string();
            // 跳过 dds_sequence_* 结构体，它们不是消息类型，而是序列类型的实现细节
            if sname.starts_with("dds_sequence_") {
                continue;
            }
            if let Some((pkg, prefix, name)) = parse_msg_name(&sname) {
                let mut fields = Vec::new();
                if let Fields::Named(nf) = &s.fields {
                    for f in &nf.named {
                        if let Some(fname) = &f.ident {
                            // convert_field_type 返回 None 表示字段类型无法安全映射，静默跳过
                            if let Some(kind) = convert_field_type(&f.ty, &type_map, &seq_map) {
                                fields.push((fname.to_string(), kind));
                            }
                        }
                    }
                }
                msgs.push(MsgStruct {
                    pkg,
                    prefix,
                    name,
                    fields,
                });
            }
        }
    }

    // <pkg, prefix> → [MsgStruct]，便于后续按消息类别生成模块和类型定义
    let mut by_pkg: BTreeMap<String, BTreeMap<String, Vec<MsgStruct>>> = BTreeMap::new();
    for s in msgs {
        by_pkg
            .entry(s.pkg.clone())
            .or_default()
            .entry(s.prefix.clone())
            .or_default()
            .push(s);
    }

    // 为每个消息类型生成结构体定义及双向 From impl，逐层构造 pkg::cat 模块 TokenStream
    let mut pkg_mods: Vec<TokenStream> = Vec::new();
    for (pkg, cats) in &by_pkg {
        if pkg.is_empty() {
            continue;
        }
        let pkg_ident = format_ident!("{}", pkg);
        let mut cat_mods: Vec<TokenStream> = Vec::new();
        for (msg, types) in cats {
            let msg_ident = format_ident!("{}", msg);

            // 将本 cat 下的类型按「父名」分组：
            //   - name 不含 '_' → 直接放在 cat mod 中
            //   - name 含 '_'（如 CancelGoal_Request）→ 放入 cat::CancelGoal 子模块
            let mut direct_items: Vec<TokenStream> = Vec::new();
            let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for (i, s) in types.iter().enumerate() {
                if let Some((parent, _)) = s.name.split_once('_') {
                    grouped.entry(parent.to_string()).or_default().push(i);
                } else {
                    direct_items.push(generate_item_wrapper(s, format_ident!("{}", s.name)));
                }
            }
            // 生成各父名子模块
            let sub_mods: Vec<TokenStream> = grouped
                .iter()
                .map(|(parent, indices)| {
                    let parent_ident = format_ident!("{}", parent);
                    let sub_items: Vec<TokenStream> = indices
                        .iter()
                        .map(|&i| {
                            let s = &types[i];
                            let sub = s
                                .name
                                .split_once('_')
                                .map_or(s.name.as_str(), |(_, sub)| sub);
                            generate_item_wrapper(s, format_ident!("{}", sub))
                        })
                        .collect();
                    quote! { pub mod #parent_ident { #(#sub_items)* } }
                })
                .collect();

            cat_mods.push(quote! {
                pub mod #msg_ident {
                    #(#direct_items)*
                    #(#sub_mods)*
                }
            });
        }
        pkg_mods.push(quote! { pub mod #pkg_ident { #(#cat_mods)* } });
    }

    // 将所有模块合并为单一 TokenStream，解析为 syn::File 后用 prettyplease 格式化
    let all_tokens = quote! { #(#pkg_mods)* };
    let syntax_tree = syn::parse2::<syn::File>(all_tokens)
        .expect("Failed to parse generated tokens as syn::File");
    let formatted = prettyplease::unparse(&syntax_tree);
    fs::write(out_dir.join(WARPPER_TYPES_FILE), formatted).expect("Failed to write safe_types.rs");
}
