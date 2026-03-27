use std::{env, fs, path::{Path, PathBuf}, process::Command};

fn main() {
    // 使用 pkg-config 查找 dds
    let dds = match pkg_config::Config::new().probe("CycloneDDS") {
        Ok(lib) => lib,
        Err(e) => {
            panic!("Failed to find CycloneDDS via pkg-config: {}", e);
        }
    };

    // 告诉 cargo 链接库的位置
    for path in &dds.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }

    // 告诉 cargo 需要链接的库
    for lib in &dds.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 生成 Rust 绑定
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
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("Couldn't write bindings!");
    // 编译 ROS2 IDL 文件
    compile_ros2_idl_file(&dds, &PathBuf::from(env::var("OUT_DIR").unwrap()));
}

/// 在 link_paths 中推导安装前缀，查找 idlc 可执行文件。
/// 若未找到则回退到 PATH 中的 `idlc`。
fn find_idlc(dds: &pkg_config::Library) -> PathBuf {
    for link_path in &dds.link_paths {
        if let Some(prefix) = link_path.parent() {
            let candidate = prefix.join("bin").join("idlc");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from("idlc")
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

fn compile_ros2_idl_file(dds: &pkg_config::Library, out_dir: &Path) {
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

    let idlc = find_idlc(dds);

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