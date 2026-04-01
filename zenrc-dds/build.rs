use std::{collections::HashMap, env, fs::{self, OpenOptions}, path::{Path, PathBuf}, process::Command};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use sha2::{Digest, Sha256};

const WATCHED_ENV_VARS: &[&str] = &[
    "AMENT_PREFIX_PATH",
    "CMAKE_PREFIX_PATH",
    "CMAKE_IDL_PACKAGES",
    "IDL_PACKAGE_FILTER",
    "ROS_DISTRO",
];
const ROS2_MSGS_LIB_NAME: &str = "ros2_msgs";

/// 递归收集 dir 下所有 .c 文件的绝对路径。
fn collect_c_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(collect_c_files(&path));
            } else if path.extension().map_or(false, |e| e == "c") {
                result.push(path);
            }
        }
    }
    result
}

/// 编译 idlc 生成的所有 .c 文件并打包为静态库，供链接使用。
fn compile_idl_c_files(dds: &pkg_config::Library, idl_out_dir: &Path) {
    let c_files = collect_c_files(idl_out_dir);
    if c_files.is_empty() {
        return;
    }

    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();

    let mut build = cc::Build::new();
    for inc in &dds.include_paths {
        build.include(inc);
    }
    build.include(idl_out_dir);
    for f in &c_files {
        build.file(f);
    }
    build.compile(ROS2_MSGS_LIB_NAME);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static={ROS2_MSGS_LIB_NAME}");
}

fn touch(path: &Path) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .unwrap_or_else(|_| panic!("Unable to create directory '{}'", dir.display()));
    }
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to create file '{}'", path.display()));
}

fn get_env_hash() -> String {
    let mut hasher = Sha256::new();
    for var in WATCHED_ENV_VARS {
        hasher.update(var.as_bytes());
        hasher.update("=");

        if let Ok(value) = env::var(var) {
            hasher.update(value);
        }

        hasher.update("\n");
    }
    let hash = hasher.finalize();
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        write!(s, "{:02x}", b).unwrap();
        s
    })
}

fn print_cargo_watches() {
    for var in WATCHED_ENV_VARS {
        println!("cargo:rerun-if-env-changed={}", var);
    }
}
fn main() {
    // 使用 pkg-config 查找 dds
    let dds = match pkg_config::Config::new().probe("CycloneDDS") {
        Ok(lib) => lib,
        Err(e) => {
            panic!("Failed to find CycloneDDS via pkg-config: {}", e);
        }
    };
    print_cargo_watches();
    for path in &dds.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for lib in &dds.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    let env_hash = get_env_hash();
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let env_dir = out_dir.join(env_hash);
    let mark_file = env_dir.join("done");
    if !mark_file.exists() {
        // 生成绑定文件
        gen_bindings(&dds, &out_dir);
        compile_ros2_idl_files(&dds, &env_dir);
        gen_msg_bindings(&dds, &env_dir, &out_dir);
        compile_idl_c_files(&dds, &env_dir);
        gen_safe_wrappers(&out_dir);
        // 创建标记文件，表示绑定文件已生成
        touch(&mark_file);
    } else {
        println!("cargo:warning=Environment variables unchanged, skipping bindgen");
        // 即使 mark 已存在，仍需告知 cargo 链接已编译的静态库
        let lib_path = out_dir.join(format!("lib{ROS2_MSGS_LIB_NAME}.a"));
        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static={ROS2_MSGS_LIB_NAME}");
        }
    }
}

fn gen_bindings(dds: &pkg_config::Library,output: &Path) {
    
    let bindings = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_args(
            dds.include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        )
        // 让 bindgen 在构建过程中重新运行，如果头文件发生变化
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // 只生成以 dds_ 开头的函数、类型和以 DDS_ 开头的变量
        .allowlist_function("dds_.*")
        .allowlist_type("dds_.*")
        .allowlist_var("DDS_.*")
        .size_t_is_usize(true)
        .merge_extern_blocks(true)
        .generate_comments(true)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(output.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

/// 递归收集 dir 下所有 .h 文件的绝对路径。
fn collect_h_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(collect_h_files(&path));
            } else if path.extension().map_or(false, |e| e == "h") {
                result.push(path);
            }
        }
    }
    result
}

/// 为 idlc 生成的所有 .h 文件生成 Rust binding，只调用一次 bindgen，写入 out_dir/msg_bindings.rs。
fn gen_msg_bindings(dds: &pkg_config::Library, idl_out_dir: &Path, out_dir: &Path) {
    let h_files = collect_h_files(idl_out_dir);
    if h_files.is_empty() {
        return;
    }

    // 生成一个临时 wrapper 头文件，#include 所有生成的 .h
    let wrapper_path = out_dir.join("msg_bindings_wrapper.h");
    let includes: String = h_files
        .iter()
        .filter_map(|p| p.to_str())
        .map(|s| format!("#include \"{s}\"\n"))
        .collect();
    if let Err(e) = fs::write(&wrapper_path, &includes) {
        println!("cargo:warning=Failed to write wrapper header: {e}");
        return;
    }

    // 构建 bindgen：allowlist_file 只保留 idl_out_dir 下定义的类型，屏蔽系统头文件噪声
    let idl_dir_pattern: String = idl_out_dir
        .to_str()
        .unwrap_or("")
        .chars()
        .flat_map(|c| {
            if r"\^$.|?*+()[]{}".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect();

    let builder = bindgen::Builder::default()
        .header(wrapper_path.to_str().unwrap())
        .clang_args(dds.include_paths.iter().map(|p| format!("-I{}", p.display())))
        .clang_arg(format!("-I{}", idl_out_dir.display()))
        // 只保留 idl_out_dir 目录下文件中定义的符号，系统头文件内容自动过滤
        .allowlist_file(format!("{idl_dir_pattern}/.*"))
        // 精确屏蔽已在 bindings.rs 中定义的 DDS 核心类型，避免重复定义冲突
        .blocklist_type("dds_key_.*")
        .blocklist_type("dds_topic_.*")
        .blocklist_type("dds_type_.*")
        .size_t_is_usize(true)
        .merge_extern_blocks(true)
        .derive_partialeq(true)
        .generate_comments(true);

    match builder.generate() {
        Ok(b) => {
            let out_path = out_dir.join("msg_bindings.rs");
            if let Err(e) = b.write_to_file(&out_path) {
                println!("cargo:warning=Failed to write {}: {e}", out_path.display());
            }
        }
        Err(e) => println!("cargo:warning=bindgen failed for msg_bindings: {e}"),
    }
}

/// 先检查 PATH 中是否有 idlc，若无则在 dds 的 link_paths 推导出的 bin 目录中查找。
fn find_idlc(dds: &pkg_config::Library) -> Option<PathBuf> {
    // 优先使用 PATH 中的 idlc
    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = Command::new(which_cmd).arg("idlc").output() {
        if output.status.success() {
            if let Ok(s) = std::str::from_utf8(&output.stdout) {
                let path = PathBuf::from(s.lines().next().unwrap_or("").trim());
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    // 在 dds 的 link_paths 推导出的 bin 目录中查找 idlc
    for link_path in &dds.link_paths {
        if let Some(prefix) = link_path.parent() {
            let candidate = prefix.join("bin").join("idlc");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 收集系统中所有已安装的 ROS2 消息包
pub fn collect_ros_msgs() -> Vec<String> {
    let mut msgs = Vec::new();
    let mut paths = Vec::new();
    let split_char = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };
    // 检查是否设置了 CMAKE_IDL_PACKAGES
    if let Ok(cmake_idl_packages) = env::var("CMAKE_IDL_PACKAGES") {
        for package_dir in cmake_idl_packages.split(split_char) {
            let path = Path::new(package_dir);
            if path.exists() {
                paths.extend(path.to_str().map(String::from));
            }
        }
    }
    // 从 AMENT_PREFIX_PATH / CMAKE_PREFIX_PATH 扫描消息列表
    if let Ok(ament_paths) = env::var("AMENT_PREFIX_PATH") {
        paths.extend(ament_paths.split(split_char).map(String::from));
    }
    if let Ok(cmake_paths) = env::var("CMAKE_PREFIX_PATH") {
        paths.extend(cmake_paths.split(split_char).map(String::from));
    }
    // 从资源索引中读取消息列表
    for prefix_path in paths {
        let resource_index_path = Path::new(&prefix_path)
            .join("share")
            .join("ament_index")
            .join("resource_index")
            .join("rosidl_interfaces");
        if !resource_index_path.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&resource_index_path) {
            for entry in entries.flatten() {
                let package_name = entry.file_name();
                let package_name_str = package_name.to_str().unwrap_or("");
                // 读取文件内容获取消息列表
                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue; // 如果读取失败，直接跳过当前文件，消除第一层嵌套
                };
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    // 解析格式: msg/MessageName.idl 或 srv/ServiceName.idl
                    if let Some((prefix, name)) = line.split_once('/') {
                        if name.ends_with(".idl") {
                            msgs.push(std::format!("{}/{}/{}",
                                package_name_str, prefix, name));
                        }
                    }
                }
            }
        }
    }
    // 对消息列表进行排序，确保生成的代码顺序稳定
    msgs.sort_unstable();
    // 将消息列表写入 OUT_DIR/msg_list.txt，供后续代码生成使用
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("msg_list.txt"), msgs.join("\n"))
        .expect("Failed to write message list");
    msgs
}

fn compile_ros2_idl_files(dds: &pkg_config::Library, out_dir: &Path) {
    let split_char = if cfg!(target_os = "windows") { ';' } else { ':' };

    // 构建完整的 ament 前缀路径列表：AMENT_PREFIX_PATH + CMAKE_PREFIX_PATH
    let mut prefix_dirs: Vec<String> = Vec::new();
    if let Ok(ament_str) = env::var("AMENT_PREFIX_PATH") {
        prefix_dirs.extend(ament_str.split(split_char).map(String::from));
    }
    if let Ok(cmake_str) = env::var("CMAKE_PREFIX_PATH") {
        prefix_dirs.extend(cmake_str.split(split_char).map(String::from));
    }

    // 获取所有 ROS2 消息相对路径，格式为 "pkg_name/msg/Name.idl"
    let msgs = collect_ros_msgs();

    let Some(idlc) = find_idlc(dds) else {
        println!("cargo:warning=idlc not found, skipping IDL compilation");
        return;
    };

    // 遍历所有前缀路径，拼接 share/<msg>，存在则用 idlc 编译到 OUT_DIR
    for prefix in &prefix_dirs {
        let share_dir = Path::new(prefix).join("share");
        for msg in &msgs {
            let idl_path = share_dir.join(msg);
            if !idl_path.exists() {
                continue;
            }

            let mut cmd = Command::new(&idlc);
            // ROS2 IDL 中 Int32/String 等名称与 IDL 关键字大小写不同，必须开启大小写敏感
            cmd.arg("-f").arg("case-sensitive");
            // -b <share_dir> 保留 pkg/msg/Foo.h 目录层级
            cmd.arg("-b").arg(&share_dir);
            cmd.arg("-o").arg(out_dir);
            for inc in &dds.include_paths {
                cmd.arg("-I").arg(inc);
            }
            // 将 share_dir 本身也加入搜索路径，以解析跨包 #include
            cmd.arg("-I").arg(&share_dir);
            cmd.arg(&idl_path);

            match cmd.status() {
                Ok(s) if s.success() => {}
                Ok(_) => println!("cargo:warning=idlc failed for {}", idl_path.display()),
                Err(e) => println!("cargo:warning=Failed to run idlc for {}: {e}", idl_path.display()),
            }
        }
    }
}

// ===== syn-based safe wrapper generator (reads msg_bindings.rs) =====

// ============================================================
// 以下是 safe_types.rs 代码生成器的核心工具函数和数据结构。
//
// 整体流程：
//   1. 解析 bindgen 生成的 msg_bindings.rs（原始 C 类型）
//   2. 识别每个字段的 C 语义（字符串、序列、嵌套消息等）
//   3. 为每个消息类型生成安全的 Rust 包装结构体，以及双向转换 impl：
//      - From<&RawCType> for SafeType  （订阅时读取 DDS 样本）
//      - From<SafeType>  for RawCType  （发布时写入 DDS 样本）
// ============================================================

// ---- 路径/类型名辅助函数 ------------------------------------

/// 取 `syn::Path` 最后一个路径段的标识符字符串。
/// 例如 `std::os::raw::c_char` → `"c_char"`。
fn last_seg(path: &syn::Path) -> String {
    path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default()
}

/// 判断 `syn::Type` 是否为 `c_char`（即 `*mut c_char` 字符串指针的元素类型）。
fn is_c_char(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => last_seg(&p.path) == "c_char",
        _ => false,
    }
}

/// 将 bindgen 生成的 C 消息结构体名称解析为 `(pkg, cat, name)` 三元组。
///
/// bindgen 按照 `{pkg}_{cat}_{Name}` 的命名约定生成结构体，其中 `cat` 为
/// `msg`、`srv` 或 `action`。例如：
/// - `"std_msgs_msg_String"`  → `("std_msgs", "msg", "String")`
/// - `"action_msgs_srv_CancelGoal_Request"` → `("action_msgs", "srv", "CancelGoal_Request")`
///
/// 若名称不符合上述模式则返回 `None`。
fn parse_msg_name(s: &str) -> Option<(String, String, String)> {
    for (pat, cat) in &[("_msg_", "msg"), ("_srv_", "srv"), ("_action_", "action")] {
        if let Some(pos) = s.find(pat) {
            let pkg = &s[..pos];
            let name = &s[pos + pat.len()..];
            if !pkg.is_empty() && !name.is_empty() {
                return Some((pkg.to_string(), cat.to_string(), name.to_string()));
            }
        }
    }
    None
}

// ---- 类型分类枚举 --------------------------------------------

/// `dds_sequence_*` 结构体的 `_buffer` 指针所指向的元素类型。
/// 用于在 Pass 1 阶段预先索引所有已知序列类型，供 `classify` 使用。
#[derive(Clone)]
enum SeqElem {
    /// 基本数值类型，存储类型名称字符串（如 `"f64"`）。
    Prim(String),
    /// 字符串序列（`*mut *mut c_char` 或 `*mut c_char`）。
    Str,
    /// 嵌套消息序列，存储 `(pkg, cat, name)`。
    Msg(String, String, String),
    /// 无法识别的元素类型，生成器将跳过此字段。
    Unknown,
}

/// C 结构体字段的语义分类，决定安全 Rust 类型及双向转换的代码生成方式。
#[derive(Clone)]
enum CFieldKind {
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
    ArrayOfPrim(String, proc_macro2::TokenStream),
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

// ---- Pass 1 辅助：解析 dds_sequence 的元素类型 ---------------

/// 从 `dds_sequence_*::_buffer` 字段的类型推断序列元素类型。
///
/// `_buffer` 在 bindgen 中生成为 `*mut ElemType`，此函数解引用指针层，
/// 返回对应的 [`SeqElem`] 变体。支持以下情况：
/// - `*mut *mut c_char` / `*mut c_char` → [`SeqElem::Str`]
/// - `*mut u8` / `*mut f64` 等基本类型 → [`SeqElem::Prim`]
/// - `*mut some_pkg_msg_Foo` → [`SeqElem::Msg`]
fn seq_elem_from_buffer(buffer_ty: &syn::Type) -> SeqElem {
    // _buffer 字段本身是 *mut ElemType，先剥掉外层指针
    let inner = match buffer_ty {
        syn::Type::Ptr(p) => &*p.elem,
        _ => return SeqElem::Unknown,
    };
    const PRIMS: &[&str] = &["bool","u8","u16","u32","u64","i8","i16","i32","i64","f32","f64"];
    match inner {
        // *mut *mut c_char：字符串序列（每个元素是一个 C 字符串指针）
        syn::Type::Ptr(inner2) if is_c_char(&inner2.elem) => SeqElem::Str,
        // *mut c_char：也视为字符串序列（部分 IDL 编译器生成此形式）
        e if is_c_char(e) => SeqElem::Str,
        syn::Type::Path(p) => {
            let name = last_seg(&p.path);
            if PRIMS.contains(&name.as_str()) {
                SeqElem::Prim(name)
            } else if let Some((pkg, cat, n)) = parse_msg_name(&name) {
                SeqElem::Msg(pkg, cat, n)
            } else {
                SeqElem::Unknown
            }
        }
        _ => SeqElem::Unknown,
    }
}

// ---- Pass 2 辅助：将 C 字段类型分类为 CFieldKind --------------

/// 将 `syn::Type` 分类为 [`CFieldKind`]，供后续代码生成使用。
///
/// 分类规则（按优先级）：
/// 1. `*mut c_char` → `OwnedStr`；其他可变指针 → 跳过（返回 `None`）
/// 2. `[c_char; N]` → `BoundedStr(N)`；`[T; N]` → `ArrayOfPrim(T, N)`
/// 3. 基本数值类型名 → `Prim`
/// 4. `dds_sequence_*` 类型名 → 查 `seqs` 表得到 `SeqPrim`/`SeqStr`/`SeqMsg`
/// 5. type alias → 递归解析
/// 6. 消息结构体名称 → `NestedMsg`
/// 7. 其他 → `None`（字段将被忽略）
fn classify(ty: &syn::Type, aliases: &HashMap<String, syn::Type>, seqs: &HashMap<String, SeqElem>) -> Option<CFieldKind> {
    const PRIMS: &[&str] = &["bool","u8","u16","u32","u64","i8","i16","i32","i64","f32","f64"];
    Some(match ty {
        // 可变裸指针：仅 *mut c_char 视为堆字符串，其余指针类型无法安全映射
        syn::Type::Ptr(p) if p.mutability.is_some() => {
            if is_c_char(&p.elem) { CFieldKind::OwnedStr } else { return None; }
        }
        // 固定长度数组：[c_char; N] 为有界字符串，[T; N] 为基本类型数组
        syn::Type::Array(arr) => {
            let len_expr = &arr.len;
            let len_ts = quote::quote! { #len_expr };
            if is_c_char(&arr.elem) {
                CFieldKind::BoundedStr(len_ts)
            } else {
                match classify(&arr.elem, aliases, seqs)? {
                    CFieldKind::Prim(t) => CFieldKind::ArrayOfPrim(t, len_ts),
                    _ => return None,
                }
            }
        }
        // 路径类型：涵盖基本类型、序列类型、type alias、嵌套消息
        syn::Type::Path(p) => {
            let name = last_seg(&p.path);
            if PRIMS.contains(&name.as_str()) {
                // bool / 整数 / 浮点
                CFieldKind::Prim(name)
            } else if name.starts_with("dds_sequence_") {
                // 动态序列，查预建的 seqs 表确定元素类型
                match seqs.get(&name) {
                    Some(SeqElem::Prim(t)) => CFieldKind::SeqPrim(t.clone(), name.clone()),
                    Some(SeqElem::Str)     => CFieldKind::SeqStr(name.clone()),
                    Some(SeqElem::Msg(p,c,m)) => CFieldKind::SeqMsg(p.clone(),c.clone(),m.clone(),name.clone()),
                    _ => return None, // SeqElem::Unknown，跳过此字段
                }
            } else if let Some(alias_ty) = aliases.get(&name).cloned() {
                // typedef / type alias，递归解析其底层类型
                classify(&alias_ty, aliases, seqs)?
            } else if let Some((pkg,cat,n)) = parse_msg_name(&name) {
                // 直接嵌套的消息类型
                CFieldKind::NestedMsg(pkg,cat,n)
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

// ---- 代码生成辅助函数 ----------------------------------------

/// 生成指向安全包装类型的完全限定路径 TokenStream。
/// 例如 `("std_msgs", "msg", "Header")` → `crate::std_msgs::msg::Header`。
fn msg_path_ts(p: &str, c: &str, n: &str) -> TokenStream {
    let p_i = format_ident!("{}", p);
    let c_i = format_ident!("{}", c);
    let n_i = format_ident!("{}", n);
    quote! { crate::#p_i::#c_i::#n_i }
}

/// 根据 [`CFieldKind`] 生成对应的安全 Rust 字段类型 TokenStream。
///
/// | CFieldKind            | 生成的 Rust 类型              |
/// |-----------------------|-------------------------------|
/// | Prim(t)               | `t`（如 `i32`）               |
/// | OwnedStr / BoundedStr | `::std::string::String`       |
/// | ArrayOfPrim(t, ..)    | `::std::vec::Vec<t>`          |
/// | SeqPrim(t, ..)        | `::std::vec::Vec<t>`          |
/// | SeqStr(..)            | `::std::vec::Vec<String>`     |
/// | SeqMsg(p, c, n, ..)   | `::std::vec::Vec<crate::p::c::n>` |
/// | NestedMsg(p, c, n)    | `crate::p::c::n`              |
fn safe_ty_ts(kind: &CFieldKind) -> TokenStream {
    match kind {
        CFieldKind::Prim(t) => { let i = format_ident!("{}", t); quote!{ #i } }
        CFieldKind::OwnedStr | CFieldKind::BoundedStr(..) => quote!{ ::std::string::String },
        CFieldKind::ArrayOfPrim(t, ..) | CFieldKind::SeqPrim(t, ..) => {
            let i = format_ident!("{}", t); quote!{ ::std::vec::Vec<#i> }
        }
        CFieldKind::SeqStr(..) => quote!{ ::std::vec::Vec<::std::string::String> },
        CFieldKind::SeqMsg(p,c,n,..) => { let mp = msg_path_ts(p,c,n); quote!{ ::std::vec::Vec<#mp> } }
        CFieldKind::NestedMsg(p,c,n) => msg_path_ts(p, c, n),
    }
}

/// 生成 `raw → safe` 方向的单个字段初始化表达式 TokenStream。
///
/// 返回的 TokenStream 用于 `From<&RawCType> for SafeType` 的 `Self { .. }` 初始化块中，
/// 格式为 `field_name: <expr>`。
///
/// 安全性说明：
/// - `OwnedStr`：先检查 null，再通过 `CStr::from_ptr` 读取（只读借用，不转移所有权）。
/// - `BoundedStr`：固定数组保证以 `\0` 结尾，可直接调用 `as_ptr()`。
/// - `SeqPrim`：通过 `slice::from_raw_parts` 借用缓冲区后立即 `.to_vec()` 拷贝。
/// - `SeqMsg`：用指针算术逐元素递归转换，不转移序列缓冲区所有权。
fn field_from_ts(fname: &str, kind: &CFieldKind) -> TokenStream {
    let f = format_ident!("{}", fname);
    match kind {
        // 基本类型：直接复制
        CFieldKind::Prim(..) => quote!{ #f: raw.#f },
        // 堆字符串指针：null 返回默认值，否则从 C 字符串转换
        CFieldKind::OwnedStr => quote!{
            #f: if raw.#f.is_null() { ::std::default::Default::default() }
                else { unsafe { ::std::ffi::CStr::from_ptr(raw.#f) }.to_string_lossy().into_owned() }
        },
        // 固定字节数组：数组首地址作为 C 字符串指针读取
        CFieldKind::BoundedStr(..) => quote!{
            #f: unsafe { ::std::ffi::CStr::from_ptr(raw.#f.as_ptr()) }.to_string_lossy().into_owned()
        },
        // 固定长度基本类型数组：逐元素转型后收集为 Vec
        CFieldKind::ArrayOfPrim(..) => quote!{ #f: raw.#f.iter().map(|&v| v as _).collect() },
        // 基本类型动态序列：空缓冲区返回空 Vec，否则借用后拷贝
        CFieldKind::SeqPrim(..) => quote!{
            #f: if raw.#f._buffer.is_null() { ::std::vec::Vec::new() }
                else { unsafe { ::std::slice::from_raw_parts(raw.#f._buffer, raw.#f._length as usize) }.to_vec() }
        },
        // 字符串动态序列：逐元素将 *mut c_char 转为 String
        CFieldKind::SeqStr(..) => quote!{
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
        CFieldKind::SeqMsg(p,c,n,..) => {
            let st = msg_path_ts(p, c, n);
            quote!{
                #f: (|| {
                    if raw.#f._buffer.is_null() { return ::std::vec::Vec::new(); }
                    (0..raw.#f._length as usize)
                        .map(|__i| #st::from(unsafe { &*raw.#f._buffer.add(__i) }))
                        .collect()
                })()
            }
        }
        // 直接嵌套消息：递归转换
        CFieldKind::NestedMsg(p,c,n) => { let st = msg_path_ts(p,c,n); quote!{ #f: #st::from(&raw.#f) } }
    }
}

/// 生成 `safe → raw` 方向的单个字段赋值语句 TokenStream。
///
/// 返回的 TokenStream 用于 `From<SafeType> for RawCType` 的函数体中，
/// 格式为 `raw.field = <expr>;`。
///
/// 内存管理约定（字符串与序列字段）：
/// - 所有权通过 `CString::into_raw()` 或 `mem::forget` + 原始指针移交给 DDS。
/// - 序列结构体的 `_release` 字段置为 `true`，通知 DDS 在释放消息时调用 `dds_free`
///   回收缓冲区（在 Linux/glibc 下，Rust 默认分配器与 `free()` 兼容）。
/// - 调用 `dds_write` 后 DDS 会内部拷贝数据，原始结构体可安全丢弃，
///   但缓冲区的生命周期由 DDS 负责管理。
fn field_into_stmt_ts(fname: &str, kind: &CFieldKind) -> TokenStream {
    let f = format_ident!("{}", fname);
    match kind {
        // 基本类型：直接赋值（Copy 语义）
        CFieldKind::Prim(..) => quote! { raw.#f = safe.#f; },

        // 堆字符串：过滤内嵌 null 字节后构造 CString，调用 into_raw() 移交所有权
        CFieldKind::OwnedStr => quote! {
            raw.#f = {
                let __bytes: ::std::vec::Vec<u8> =
                    safe.#f.into_bytes().into_iter().filter(|&b| b != 0).collect();
                unsafe { ::std::ffi::CString::from_vec_unchecked(__bytes) }.into_raw()
            };
        },

        // 固定字节数组：将字符串截断填充到 [c_char; N]，末尾保留 '\0'
        CFieldKind::BoundedStr(len_ts) => quote! {
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
        CFieldKind::ArrayOfPrim(t, len_ts) => {
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
        },

        // 基本类型动态序列：将 Vec 的堆缓冲区通过 mem::forget 移交给 DDS 序列结构体
        CFieldKind::SeqPrim(t, seq_name) => {
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
        },

        // 字符串动态序列：逐元素 CString::into_raw()，再将指针数组的堆缓冲区移交给 DDS
        CFieldKind::SeqStr(seq_name) => {
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
        },

        // 消息动态序列：逐元素递归转换为原始 C 类型，再将缓冲区移交给 DDS
        CFieldKind::SeqMsg(p, c, n, seq_name) => {
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
        },

        // 直接嵌套消息：递归调用 RawCType::from(safe_field)
        CFieldKind::NestedMsg(p, c, n) => {
            let c_msg_ident = format_ident!("{}_{}_{}", p, c, n);
            quote! { raw.#f = crate::#c_msg_ident::from(safe.#f); }
        },
    }
}

// ---- 主生成函数 ----------------------------------------------

/// 读取 `msg_bindings.rs`（bindgen 生成的原始 C 类型），为每个 ROS 2 消息结构体生成
/// 安全的 Rust 包装，写入 `OUT_DIR/safe_types.rs`。
///
/// # 生成内容
///
/// 对每个识别到的消息类型 `{pkg}_{cat}_{Name}`，在 `{pkg}::{cat}` 模块下生成：
///
/// ```rust
/// #[derive(Debug, Clone, Default)]
/// pub struct Name { /* 安全字段 */ }
///
/// // 订阅侧：从借用的原始 C 类型转换为安全类型（zero-copy 借用 + 必要时拷贝）
/// impl<'__r> From<&'__r crate::pkg_cat_Name> for Name { ... }
///
/// // 发布侧：消费安全类型，生成原始 C 类型（字符串/序列字段的所有权移交给 DDS）
/// impl From<Name> for crate::pkg_cat_Name { ... }
/// ```
///
/// # 两遍扫描
///
/// - **Pass 1**：收集 type alias 表（用于解析 typedef）和 `dds_sequence_*` 元素类型表。
/// - **Pass 2**：收集所有消息结构体，调用 `classify` 对每个字段分类，按
///   `pkg → cat` 分组构造模块树，最后用 `prettyplease` 格式化输出。
fn gen_safe_wrappers(out_dir: &Path) {
    use std::collections::BTreeMap;
    use syn::Fields;

    let src = match fs::read_to_string(out_dir.join("msg_bindings.rs")) {
        Ok(s) => s,
        Err(e) => { println!("cargo:warning=Cannot read msg_bindings.rs: {e}"); return; }
    };
    let file = match syn::parse_file(&src) {
        Ok(f) => f,
        Err(e) => { println!("cargo:warning=Cannot parse msg_bindings.rs: {e}"); return; }
    };

    // Pass 1: 收集 type alias（typedef）表 和 dds_sequence_* 元素类型表。
    // alias 表用于解析如 `unique_identifier_msgs_msg_uint8__16` 之类的 typedef；
    // seqs 表记录每种 dds_sequence_* 的 _buffer 元素类型，供 classify 查询。
    let mut aliases: HashMap<String, syn::Type> = HashMap::new();
    let mut seqs: HashMap<String, SeqElem> = HashMap::new();
    for item in &file.items {
        match item {
            syn::Item::Type(t) => { aliases.insert(t.ident.to_string(), (*t.ty).clone()); }
            syn::Item::Struct(s) if s.ident.to_string().starts_with("dds_sequence_") => {
                if let Fields::Named(fs) = &s.fields {
                    for f in &fs.named {
                        if f.ident.as_ref().map(|i| i == "_buffer").unwrap_or(false) {
                            seqs.insert(s.ident.to_string(), seq_elem_from_buffer(&f.ty));
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: 扫描所有结构体，跳过 dds_sequence_* 辅助类型，
    // 对符合 {pkg}_{cat}_{Name} 命名规则的结构体提取字段并分类。
    struct MsgStruct { pkg: String, cat: String, name: String, fields: Vec<(String, CFieldKind)> }
    let mut msgs: Vec<MsgStruct> = Vec::new();
    for item in &file.items {
        if let syn::Item::Struct(s) = item {
            let sname = s.ident.to_string();
            if sname.starts_with("dds_sequence_") { continue; }
            if let Some((pkg, cat, name)) = parse_msg_name(&sname) {
                let mut fields = Vec::new();
                if let Fields::Named(nf) = &s.fields {
                    for f in &nf.named {
                        if let Some(fname) = &f.ident {
                            // classify 返回 None 表示字段类型无法安全映射，静默跳过
                            if let Some(kind) = classify(&f.ty, &aliases, &seqs) {
                                fields.push((fname.to_string(), kind));
                            }
                        }
                    }
                }
                msgs.push(MsgStruct { pkg, cat, name, fields });
            }
        }
    }

    // 按 pkg → cat 两级分组，保证生成的模块树稳定有序（BTreeMap 按字典序排列）
    let mut by_pkg: BTreeMap<String, BTreeMap<String, Vec<MsgStruct>>> = BTreeMap::new();
    for s in msgs { by_pkg.entry(s.pkg.clone()).or_default().entry(s.cat.clone()).or_default().push(s); }

    // 为每个消息类型生成结构体定义及双向 From impl，逐层构造 pkg::cat 模块 TokenStream
    let mut pkg_mods: Vec<TokenStream> = Vec::new();
    for (pkg, cats) in &by_pkg {
        if pkg.is_empty() { continue; }
        let pkg_ident = format_ident!("{}", pkg);
        let mut cat_mods: Vec<TokenStream> = Vec::new();
        for (cat, types) in cats {
            let cat_ident = format_ident!("{}", cat);
            let mut type_items: Vec<TokenStream> = Vec::new();
            for s in types {
                // C 类型标识符，如 `action_msgs_msg_GoalInfo`
                let c_name_ident = format_ident!("{}_{}_{}", s.pkg, s.cat, s.name);
                // 安全包装类型标识符，如 `GoalInfo`
                let name_ident = format_ident!("{}", s.name);

                // 字段定义：pub field: SafeType
                let fields_def: Vec<TokenStream> = s.fields.iter().map(|(fname, kind)| {
                    let f_id = format_ident!("{}", fname);
                    let ty = safe_ty_ts(kind);
                    quote! { pub #f_id: #ty }
                }).collect();

                // raw → safe 方向的字段初始化表达式列表
                let from_fields: Vec<TokenStream> = s.fields.iter()
                    .map(|(fname, kind)| field_from_ts(fname, kind))
                    .collect();

                // safe → raw 方向的字段赋值语句列表
                let into_stmts: Vec<TokenStream> = s.fields.iter()
                    .map(|(fname, kind)| field_into_stmt_ts(fname, kind))
                    .collect();

                type_items.push(quote! {
                    #[derive(Debug, Clone, Default)]
                    pub struct #name_ident {
                        #(#fields_def,)*
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
                            // 用零值初始化，避免未定义字段含有垃圾值
                            let mut raw = unsafe {
                                ::std::mem::MaybeUninit::<crate::#c_name_ident>::zeroed()
                                    .assume_init()
                            };
                            #(#into_stmts)*
                            raw
                        }
                    }
                });
            }
            cat_mods.push(quote! { pub mod #cat_ident { #(#type_items)* } });
        }
        pkg_mods.push(quote! { pub mod #pkg_ident { #(#cat_mods)* } });
    }

    // 将所有模块合并为单一 TokenStream，解析为 syn::File 后用 prettyplease 格式化
    let all_tokens = quote! { #(#pkg_mods)* };
    let syntax_tree = syn::parse2::<syn::File>(all_tokens)
        .expect("Failed to parse generated tokens as syn::File");
    let formatted = prettyplease::unparse(&syntax_tree);
    fs::write(out_dir.join("safe_types.rs"), formatted)
        .expect("Failed to write safe_types.rs");
}


