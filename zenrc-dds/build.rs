mod msg_gen;

use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const WATCHED_ENV_VARS: &[&str] = &[
    "AMENT_PREFIX_PATH",
    "CMAKE_PREFIX_PATH",
    "CMAKE_IDL_PACKAGES",
    "IDL_PACKAGE_FILTER",
    "ROS_DISTRO",
    "DDS_IDL_PATH",
];
const ROS2_MSGS_LIB_NAME: &str = "ros2_msgs";

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

fn gen_bindings(dds: &pkg_config::Library, output: &Path) {
    let bindings = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_args(
            dds.include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        )
        // 仅保留 dds_* 函数/类型和 DDS_* 变量
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

fn main() {
    // pkg-config 查找 CycloneDDS
    let dds = match pkg_config::Config::new().probe("CycloneDDS") {
        Ok(lib) => lib,
        Err(e) => panic!("Failed to find CycloneDDS via pkg-config: {}", e),
    };

    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();

    for path in &dds.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for lib in &dds.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // IDL 编译与消息绑定生成（带环境哈希缓存）
    print_cargo_watches();

    let env_hash = get_env_hash();
    let env_dir = out_dir.join(&env_hash);
    let mark_file = env_dir.join("done");
    if !mark_file.exists() {
        // 生成 DDS API 绑定（bindings.rs）
        gen_bindings(&dds, &out_dir);
        compile_idl_files(&dds, &env_dir);
        compile_idl_c_files(&dds, &env_dir);
        gen_msg_bindings(&env_dir, &out_dir);
        msg_gen::generate_rust_wrappers(&out_dir);
        touch(&mark_file);
    } else {
        println!("cargo:warning=Environment variables unchanged, skipping IDL compilation");
        let lib_path = env_dir.join(format!("lib{ROS2_MSGS_LIB_NAME}.a"));
        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", env_dir.display());
            println!("cargo:rustc-link-lib=static={ROS2_MSGS_LIB_NAME}");
        }
    }
}

/// 递归收集 dir 下所有 .c 文件
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

/// 将 idlc 生成的 .c 文件编译并打包为静态库
fn compile_idl_c_files(dds: &pkg_config::Library, idl_out_dir: &Path) {
    let c_files = collect_c_files(idl_out_dir);
    if c_files.is_empty() {
        return;
    }

    let mut build = cc::Build::new();
    for inc in &dds.include_paths {
        build.include(inc);
    }
    build.include(idl_out_dir);
    for f in &c_files {
        build.file(f);
    }
    build.out_dir(idl_out_dir);
    build.compile(ROS2_MSGS_LIB_NAME);

    println!("cargo:rustc-link-search=native={}", idl_out_dir.display());
    println!("cargo:rustc-link-lib=static={ROS2_MSGS_LIB_NAME}");
}

/// 递归收集 dir 下所有 .h 文件
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

/// 为 idlc 生成的 .h 文件生成 Rust binding，写入 msg_bindings.rs
fn gen_msg_bindings(idl_out_dir: &Path, out_dir: &Path) {
    let h_files = collect_h_files(idl_out_dir);
    if h_files.is_empty() {
        return;
    }

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
        .clang_arg(format!("-I{}", idl_out_dir.display()))
        .allowlist_file(format!("{idl_dir_pattern}/.*"))
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

/// 优先从 PATH 查找 idlc，其次从 dds link_paths 推导 bin 目录
fn find_idlc(dds: &pkg_config::Library) -> Option<PathBuf> {
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
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

/// IDL 文件条目，携带路径和 idlc `-b` 基目录
struct IdlEntry {
    path: PathBuf,
    base: PathBuf,
}

/// 收集所有待编译的 IDL 文件（ROS2 系统包 + `DDS_IDL_PATH` 自定义）
fn collect_idl_files() -> Vec<IdlEntry> {
    let split_char = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };
    let mut result = Vec::new();

    let ros_msgs = collect_ros_msgs();
    let mut prefix_dirs: Vec<PathBuf> = Vec::new();
    for var in &["AMENT_PREFIX_PATH", "CMAKE_PREFIX_PATH"] {
        if let Ok(paths) = env::var(var) {
            for prefix in paths.split(split_char) {
                let p = PathBuf::from(prefix);
                if p.exists() {
                    prefix_dirs.push(p);
                }
            }
        }
    }

    for prefix in &prefix_dirs {
        let share_dir = prefix.join("share");
        for msg in &ros_msgs {
            let idl_path = share_dir.join(msg);
            if idl_path.exists() {
                result.push(IdlEntry {
                    path: idl_path,
                    base: share_dir.clone(),
                });
            }
        }
    }

    if let Ok(val) = env::var("DDS_IDL_PATH") {
        for entry in val.split(split_char) {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let path = PathBuf::from(entry);
            if path.is_file() && path.extension().map_or(false, |e| e == "idl") {
                let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                result.push(IdlEntry {
                    path,
                    base,
                });
            } else if path.is_dir() {
                let mut stack = vec![path.clone()];
                while let Some(dir) = stack.pop() {
                    let Ok(rd) = fs::read_dir(&dir) else { continue };
                    let mut entries: Vec<_> = rd.flatten().collect();
                    entries.sort_by_key(|e| e.file_name());
                    for e in entries {
                        let p = e.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().map_or(false, |ext| ext == "idl") {
                            result.push(IdlEntry {
                                path: p,
                                base: path.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("msg_list.txt"),
        result
            .iter()
            .map(|e| e.path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("Failed to write msg_list.txt");
    result
}

/// 编译所有 IDL 文件，产物（`.c`/`.h`）写入 `out_dir`
fn compile_idl_files(dds: &pkg_config::Library, out_dir: &Path) {
    let idl_files = collect_idl_files();
    if idl_files.is_empty() {
        return;
    }
    let Some(idlc) = find_idlc(dds) else {
        println!("cargo:warning=idlc not found, skipping IDL compilation");
        return;
    };

    let mut include_dirs: Vec<PathBuf> = dds.include_paths.clone();
    for base in idl_files.iter().map(|e| &e.base) {
        if !include_dirs.contains(base) {
            include_dirs.push(base.clone());
        }
    }

    for entry in idl_files {
        let mut cmd = Command::new(&idlc);
        cmd.arg("-f").arg("case-sensitive");
        cmd.arg("-b").arg(&entry.base);
        cmd.arg("-o").arg(out_dir);
        for inc in &include_dirs {
            cmd.arg("-I").arg(inc);
        }
        cmd.arg(&entry.path);

        match cmd.status() {
            Ok(s) if s.success() => {}
            Ok(_) => println!("cargo:warning=idlc failed for {}", entry.path.display()),
            Err(e) => println!(
                "cargo:warning=Failed to run idlc for {}: {e}",
                entry.path.display()
            ),
        }
    }
}

fn collect_ros_msgs() -> Vec<String> {
    let mut msgs = Vec::new();
    let mut paths = Vec::new();
    let split_char = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };

    if let Ok(cmake_idl_packages) = env::var("CMAKE_IDL_PACKAGES") {
        for package_dir in cmake_idl_packages.split(split_char) {
            let path = Path::new(package_dir);
            if path.exists() {
                paths.extend(path.to_str().map(String::from));
            }
        }
    }
    if let Ok(ament_paths) = env::var("AMENT_PREFIX_PATH") {
        paths.extend(ament_paths.split(split_char).map(String::from));
    }
    if let Ok(cmake_paths) = env::var("CMAKE_PREFIX_PATH") {
        paths.extend(cmake_paths.split(split_char).map(String::from));
    }

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
                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some((prefix, name)) = line.split_once('/') {
                        if name.ends_with(".idl") {
                            msgs.push(std::format!("{}/{}/{}", package_name_str, prefix, name));
                        }
                    }
                }
            }
        }
    }
    msgs.sort_unstable();
    msgs
}
