use std::{env, fs::{self, OpenOptions}, path::{Path, PathBuf}, process::Command};

use sha2::{Digest, Sha256};

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

const WATCHED_ENV_VARS: &[&str] = &[
    "AMENT_PREFIX_PATH",
    "CMAKE_PREFIX_PATH",
    "CMAKE_IDL_PACKAGES",
    "IDL_PACKAGE_FILTER",
    "ROS_DISTRO",
];
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
        .blocklist_type("dds_type_.*");

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