use rayon::prelude::*;
use regex::Regex;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    iter::chain,
    path::{Path, PathBuf},
};
const SRV_SUFFICES: &[&str] = &["Request", "Response"];
const ACTION_SUFFICES: &[&str] = &["Goal", "Result", "Feedback", "FeedbackMessage"];

#[derive(Debug)]
pub struct RosMsg {
    pub module: String, // e.g. std_msgs
    pub prefix: String, // e.g. "msg" or "srv"
    pub name: String,   // e.g. "String"
}

/// 收集系统中所有已安装的 ROS2 消息包
pub fn collect_ros_msgs() -> Vec<RosMsg> {
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
                    if let Some((prefix, name_with_ext)) = line.split_once('/') {
                        if let Some(name) = name_with_ext.strip_suffix(".idl") {
                            msgs.push(RosMsg {
                                module: package_name_str.to_string(),
                                prefix: prefix.to_string(),
                                name: name.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    // 对消息列表进行排序，确保生成的代码顺序稳定
    msgs.sort_by(|a, b| {
        a.module
            .cmp(&b.module)
            .then(a.prefix.cmp(&b.prefix))
            .then(a.name.cmp(&b.name))
    });
    // 应用过滤器（如果设置了 IDL_PACKAGE_FILTER）
    if let Ok(filter) = env::var("IDL_PACKAGE_FILTER") {
        let filters: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
        msgs.retain(|msg| filters.contains(&msg.module.as_str()));
    }

    msgs
}

/// 将 CamelCase 转换为 snake_case
/// 转换规则：
/// 1. 在小写字母和大写字母之间插入下划线，例如 "StringMessage" -> "String_Message"
/// 2. 在连续的大写字母之间插入下划线，例如 "HTTPResponse" -> "HTTP_Response"
pub fn camel_to_snake(s: &str) -> String {
    static UPPERCASE_BEFORE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(.)([A-Z][a-z]+)").unwrap());
    static UPPERCASE_AFTER: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").unwrap());

    let s = UPPERCASE_BEFORE.replace_all(s, "${1}_${2}");
    let s = UPPERCASE_AFTER.replace_all(&s, "${1}_${2}");
    s.to_lowercase()
}

/// 生成 C 头文件包含语句，写入 msg_includes.h
pub fn generate_includes(file_name: &str, msgs: &[RosMsg]) {
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let includes_file = out_dir.join(file_name);

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&includes_file)
        .unwrap_or_else(|_| panic!("Unable to create file '{}'", includes_file.display()));

    writeln!(file, "// 自动生成的消息包含文件").unwrap();

    for msg in msgs {
        let snake_name = camel_to_snake(&msg.name);

        // 消息头文件
        writeln!(
            file,
            "#include <{}/{}/{}.h>",
            msg.module, msg.prefix, snake_name
        )
        .unwrap();

        // introspection 头文件
        writeln!(
            file,
            "#include <{}/{}/detail/{}__rosidl_typesupport_introspection_c.h>",
            msg.module, msg.prefix, snake_name
        )
        .unwrap();
    }

    println!("cargo:rerun-if-changed={}", includes_file.display());
    eprintln!("Generated includes file: {}", includes_file.display());
}

/// 生成 introspection 函数映射表
/// 根据消息列表生成编译时完美哈希表
pub fn generate_introspection_map(file_name: &str, msg_list: &[RosMsg]) {
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let map_file = out_dir.join(file_name);

    // 收集所有映射条目（使用并行迭代器）
    let entries: Vec<_> = msg_list
        .par_iter()
        .flat_map(|msg| {
            let RosMsg {
                module,
                prefix,
                name,
            } = msg;

            match prefix.as_str() {
                "msg" => {
                    // 对于消息：生成单个映射条目
                    let key = format!("{}__{}__{}", module, prefix, name);
                    let func_name = format!(
                        "rosidl_typesupport_introspection_c__get_message_type_support_handle__{}__{}__{}",
                        module, prefix, name
                    );
                    vec![(key, func_name)]
                }
                "srv" => {
                    // 对于服务：遍历 Request 和 Response 后缀
                    SRV_SUFFICES
                        .iter()
                        .map(|suffix| {
                            let key = format!("{}__{}__{}_{}", module, prefix, name, suffix);
                            let func_name = format!(
                                "rosidl_typesupport_introspection_c__get_message_type_support_handle__{}__{}__{}_{}",
                                module, prefix, name, suffix
                            );
                            (key, func_name)
                        })
                        .collect()
                }
                "action" => {
                    // 对于动作：生成标准后缀（Goal, Result, Feedback, FeedbackMessage）
                    let iter1 = ACTION_SUFFICES.iter().map(|suffix| {
                        let key = format!("{}__{}__{}_{}", module, prefix, name, suffix);
                        let func_name = format!(
                            "rosidl_typesupport_introspection_c__get_message_type_support_handle__{}__{}__{}_{}",
                            module, prefix, name, suffix
                        );
                        (key, func_name)
                    });

                    // 生成动作底层的内部服务通信结构
                    let service_suffixes = ["SendGoal_Request", "SendGoal_Response", "GetResult_Request", "GetResult_Response"];
                    let iter2 = service_suffixes.iter().map(|suffix| {
                        let key = format!("{}__{}__{}_{}", module, prefix, name, suffix);
                        let func_name = format!(
                            "rosidl_typesupport_introspection_c__get_message_type_support_handle__{}__{}__{}_{}",
                            module, prefix, name, suffix
                        );
                        (key, func_name)
                    });

                    // 合并两个迭代器的结果
                    chain(iter1, iter2).collect()
                }
                _ => {
                    // 未知的消息类型，输出警告
                    eprintln!("Warning: Unknown message prefix type '{}' for {}/{}", prefix, module, name);
                    unreachable!()
                }
            }
        })
        .collect();

    // 生成映射表文件
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&map_file)
        .unwrap_or_else(|_| panic!("Unable to create file '{}'", map_file.display()));

    writeln!(file, "// 自动生成的 introspection 函数映射表").unwrap();
    writeln!(
        file,
        "type IntrospectionFn = unsafe extern \"C\" fn() -> *const rosidl_message_type_support_t;"
    )
    .unwrap();
    writeln!(
        file,
        "pub static INTROSPECTION_MAP: phf::Map<&'static str, IntrospectionFn> = phf::phf_map! {{"
    )
    .unwrap();

    for (key, func_name) in &entries {
        writeln!(file, "    \"{}\" => {} as IntrospectionFn,", key, func_name).unwrap();
    }

    writeln!(file, "}};").unwrap();

    eprintln!(
        "Generated introspection map with {} entries: {}",
        entries.len(),
        map_file.display()
    );
}

/// 为所有消息模块生成 Cargo 链接库指令
pub fn print_msg_link_libs(ros_msgs: &[RosMsg]) {
    let mut modules_vec: Vec<String> = ros_msgs
        .iter()
        .map(|m| m.module.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    modules_vec.sort();

    for module in modules_vec {
        println!(
            "cargo:rustc-link-lib=dylib={}__rosidl_typesupport_c",
            module
        );
        println!(
            "cargo:rustc-link-lib=dylib={}__rosidl_typesupport_introspection_c",
            module
        );
        println!("cargo:rustc-link-lib=dylib={}__rosidl_generator_c", module);
    }
}
