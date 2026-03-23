#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/rcl_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/introspection_maps.rs"));

mod rust_types;
mod msg_wrapper;
pub mod generated;

pub use msg_wrapper::{TypesupportWrapper, NativeMsgWrapper, ServiceTypeSupportWrapper};

// 测试
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::rust_types::{TypeSupport, generate_rust_msg};

    #[test]
    fn test_print_all_message_types() {
        use std::io::Write;

        let output_path = "message_types_output.txt";
        let mut file = std::fs::File::create(output_path).expect("无法创建输出文件");

        writeln!(file, "\n=== 所有 ROS 2 消息类型 ===\n").unwrap();

        // 遍历所有注册的消息类型
        for (type_name, get_type_support) in FUNCTIONS_MAP.entries() {
            writeln!(file, "类型: {}", type_name).unwrap();

            // 调用函数获取类型支持句柄
            let ts_ptr = unsafe { get_type_support() };
            let type_support = TypeSupport::from_ptr(ts_ptr);

            // 解析类型信息
            let introspection = type_support.to_introspection();

            writeln!(file, "  模块: {}", introspection.module).unwrap();
            writeln!(file, "  前缀: {}", introspection.prefix).unwrap();
            writeln!(file, "  名称: {}", introspection.name).unwrap();
            writeln!(file, "  完整名称: {}", introspection.name()).unwrap();
            writeln!(file, "  成员数量: {}", introspection.members.len()).unwrap();

            // 打印每个成员的详细信息
            if !introspection.members.is_empty() {
                writeln!(file, "  成员:").unwrap();
                for member in introspection.members {
                    let member_wrapper = member;

                    writeln!(file, "    - {}", member_wrapper.name()).unwrap();
                    writeln!(file, "      类型: {:?}", member_wrapper.type_id()).unwrap();
                    writeln!(file, "      偏移: {} 字节", member_wrapper.offset()).unwrap();

                    if member_wrapper.is_array() {
                        if let Some(size) = member_wrapper.array_size() {
                            writeln!(file, "      数组大小: {}", size).unwrap();
                        } else {
                            writeln!(file, "      动态数组").unwrap();
                        }
                    }

                    if let Some(bound) = member_wrapper.string_upper_bound() {
                        writeln!(file, "      字符串上限: {}", bound).unwrap();
                    }
                }
            }

            writeln!(file).unwrap();
        }

        writeln!(file, "总共 {} 个消息类型", FUNCTIONS_MAP.len()).unwrap();

        println!("输出已保存到: {}", output_path);
    }

    #[test]
    fn test_print_specific_type() {
        // 测试特定类型（如果存在 std_msgs/String）
        if let Some(get_type_support) = FUNCTIONS_MAP.get("std_msgs__msg__String") {
            println!("\n=== std_msgs/String 详细信息 ===\n");

            let ts_ptr = unsafe { get_type_support() };
            let type_support = TypeSupport::from_ptr(ts_ptr);
            let introspection = type_support.to_introspection();

            println!("完整类型名: {}", introspection.name());
            println!("成员:");

            for member in introspection.members {
                let member_wrapper = member; // 直接使用成员，不进行额外包装
                println!(
                    "  {} ({:?})",
                    member_wrapper.name(),
                    member_wrapper.type_id()
                );
            }
        } else {
            println!("std_msgs__msg__String 类型未找到");
        }
    }

    /// 同时生成 msg 和 srv 的 Rust 类型文件，
    /// 每个 ROS 包对应一个 .rs 文件，内部用 `pub mod msg` 和 `pub mod srv` 分别组织。
    /// 对于 srv，每个服务名对应一个子模块，包含 Request、Response 结构体和 Service 类型支持包装。
    #[test]
    fn test_generate_msg_and_srv_types() {
        use std::collections::HashMap;

        use crate::rust_types::{generate_rust_msg, generate_rust_service};

        let output_dir = Path::new("generated_types");
        if !output_dir.exists() {
            fs::create_dir_all(output_dir).expect("无法创建输出目录");
        }

        // module -> msg 代码片段列表
        let mut msg_modules: HashMap<String, Vec<String>> = HashMap::new();
        // module -> service_name -> [Request代码, Response代码]
        let mut srv_modules: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

        let mut msg_generated = 0;
        let mut msg_failed = 0;
        let mut srv_generated = 0;
        let mut srv_failed = 0;

        for (type_name, _) in FUNCTIONS_MAP.entries() {
            let parts: Vec<&str> = type_name.split("__").collect();
            if parts.len() != 3 {
                continue;
            }
            let (module, prefix, name) = (parts[0], parts[1], parts[2]);

            match prefix {
                "msg" => {
                    match std::panic::catch_unwind(|| generate_rust_msg(module, prefix, name)) {
                        Ok(tokens) => {
                            msg_modules
                                .entry(module.to_string())
                                .or_default()
                                .push(tokens.to_string());
                            msg_generated += 1;
                            println!("✓ msg: {}", type_name);
                        }
                        Err(e) => {
                            msg_failed += 1;
                            println!("✗ msg 失败 {}: {:?}", type_name, e);
                        }
                    }
                }
                "srv" => {
                    // name 形如 "SetBool_Request"，rsplit_once 得到 service_name="SetBool"
                    if let Some((svc_name, _)) = name.rsplit_once('_') {
                        match std::panic::catch_unwind(|| generate_rust_msg(module, prefix, name)) {
                            Ok(tokens) => {
                                srv_modules
                                    .entry(module.to_string())
                                    .or_default()
                                    .entry(svc_name.to_string())
                                    .or_default()
                                    .push(tokens.to_string());
                                srv_generated += 1;
                                println!("✓ srv: {}", type_name);
                            }
                            Err(e) => {
                                srv_failed += 1;
                                println!("✗ srv 失败 {}: {:?}", type_name, e);
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        // 合并所有涉及的模块名（去重后排序）
        let mut all_modules: Vec<String> = msg_modules
            .keys()
            .chain(srv_modules.keys())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        all_modules.sort();

        let mut written = 0usize;
        for module in &all_modules {
            let mut module_code = String::new();

            // 生成 pub mod msg { ... }
            if let Some(msg_codes) = msg_modules.get(module) {
                let combined = msg_codes.join("\n\n");
                module_code.push_str(&format!("pub mod msg {{use super::*;\n{}\n}}\n\n", combined));
            }

            // 生成 pub mod srv { pub mod ServiceName { ... } ... }
            if let Some(svc_map) = srv_modules.get(module) {
                let mut sorted_svcs: Vec<_> = svc_map.iter().collect();
                sorted_svcs.sort_by_key(|(k, _)| k.as_str());

                let mut svc_mod_items = Vec::new();
                for (svc_name, req_res_codes) in sorted_svcs {
                    let combined = req_res_codes.join("\n\n");
                    // 生成 Service 类型支持包装（只生成 TokenStream，不实际链接）
                    let service_code =
                        std::panic::catch_unwind(|| generate_rust_service(module, "srv", svc_name))
                            .map(|ts| ts.to_string())
                            .unwrap_or_default();
                    svc_mod_items.push(format!(
                        "pub mod {} {{\n{}\n{}\n}}",
                        svc_name, combined, service_code
                    ));
                }
                module_code.push_str(&format!(
                    "pub mod srv {{use super::*;\n{}\n}}\n",
                    svc_mod_items.join("\n\n")
                ));
            }

            // 使用 prettyplease 格式化，失败时保留原始代码
            let formatted = match syn::parse_file(&module_code) {
                Ok(tree) => prettyplease::unparse(&tree),
                Err(e) => {
                    println!("⚠ 格式化失败 {}: {}", module, e);
                    module_code
                }
            };

            let file_path = output_dir.join(format!("{}.rs", module));
            fs::write(&file_path, &formatted).expect("无法写入文件");
            println!("📄 已写入: {}", file_path.display());
            written += 1;
        }

        println!("\n总结:");
        println!("  msg 已生成: {}", msg_generated);
        println!("  msg 失败:   {}", msg_failed);
        println!("  srv 已生成: {}", srv_generated);
        println!("  srv 失败:   {}", srv_failed);
        println!("  写入文件数: {}", written);
        println!("  输出目录:   {}", output_dir.display());

        assert_eq!(msg_failed, 0, "有 {} 个 msg 类型生成失败", msg_failed);
        assert!(written > 0, "没有生成任何文件");
    }
}
