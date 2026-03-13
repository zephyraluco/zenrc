#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/rcl_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/introspection_maps.rs"));

mod rust_types;

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
            let type_support = unsafe { TypeSupport::from_ptr(ts_ptr) };

            // 解析类型信息
            let introspection = unsafe { type_support.to_introspection() };

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
            let type_support = unsafe { TypeSupport::from_ptr(ts_ptr) };
            let introspection = unsafe { type_support.to_introspection() };

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

    #[test]
    fn test_generate_all_available_messages() {
        use std::collections::HashMap;

        // 遍历所有可用的消息类型并生成
        let output_dir = Path::new("generated_types_all");
        if !output_dir.exists() {
            fs::create_dir_all(output_dir).expect("无法创建输出目录");
        }

        let mut generated_count = 0;
        let mut failed_count = 0;
        let mut modules: HashMap<String, Vec<String>> = HashMap::new();

        // 第一步：按模块分组并生成代码
        for (type_name, _) in FUNCTIONS_MAP.entries() {
            // 解析 key: module__prefix__name
            let parts: Vec<&str> = type_name.split("__").collect();
            if parts.len() != 3 {
                println!("跳过无效的键格式: {}", type_name);
                continue;
            }

            let (module, prefix, name) = (parts[0], parts[1], parts[2]);

            // 只处理 msg 类型
            if prefix != "msg" {
                continue;
            }

            match std::panic::catch_unwind(|| generate_rust_msg(module, prefix, name)) {
                Ok(tokens) => {
                    let rust_code = tokens.to_string();
                    modules
                        .entry(module.to_string())
                        .or_insert_with(Vec::new)
                        .push(rust_code);
                    generated_count += 1;
                    println!("✓ 已生成: {}", type_name);
                }
                Err(e) => {
                    failed_count += 1;
                    println!("✗ 生成失败 {}: {:?}", type_name, e);
                }
            }
        }

        // 第二步：合并同一模块的所有类型并格式化
        for (module, codes) in modules {
            let combined_code = codes.join("\n\n");

            // 使用 prettyplease 格式化代码
            let formatted_code = match syn::parse_file(&combined_code) {
                Ok(syntax_tree) => prettyplease::unparse(&syntax_tree),
                Err(_) => {
                    println!("⚠ 格式化失败，使用原始代码: {}", module);
                    combined_code
                }
            };

            let file_path = output_dir.join(format!("{}.rs", module));
            fs::write(&file_path, formatted_code).expect("无法写入文件");
        }

        println!("\n总结:");
        println!("  已生成: {}", generated_count);
        println!("  失败: {}", failed_count);
        // println!("  模块数: {}", modules.len());
        println!("  输出目录: {}", output_dir.display());
    }
}
