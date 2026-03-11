#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/rcl_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/introspection_maps.rs"));

mod rust_types;

// 测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_types::{TypeSupport, MessageMember};
    use std::mem;

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
                    let member_wrapper = unsafe {
                        mem::transmute::<&rosidl_typesupport_introspection_c__MessageMember, &MessageMember>(member)
                    };

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
                let member_wrapper = unsafe {
                    mem::transmute::<&rosidl_typesupport_introspection_c__MessageMember, &MessageMember>(member)
                };
                println!("  {} ({:?})", member_wrapper.name(), member_wrapper.type_id());
            }
        } else {
            println!("std_msgs__msg__String 类型未找到");
        }
    }
}
