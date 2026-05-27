use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zenrc_msgen::{compile_idl_libs, generate_msg_bindings, generate_rust_wrappers};

const WATCHED_ENV_VARS: &[&str] = &[
    "DDS_INCLUDE_PATH",
    "DDS_LIBRARY_PATH",
    "DDS_IDL_PATH",
    "ROS_DISTRO",
    "AMENT_PREFIX_PATH",
    "CMAKE_PREFIX_PATH",
];

const MSGS_LIB_NAME: &str = "msgs";

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

pub fn generate_dds_bindings(dds_include_paths: &Vec<PathBuf>, out_dir: &Path) {
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(
            dds_include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        )
        // 仅保留 dds_* 函数/类型和 DDS_* 变量
        .allowlist_function("dds_.*")
        .allowlist_type("dds_.*")
        .allowlist_var("DDS_.*")
        // XTypes TypeObject 结构体（DDS_XTypes_TypeObject / CompleteStructType 等）
        .allowlist_type("DDS_XTypes_.*")
        // ddsi_typelib.h 中的 typeinfo/typeid 工具函数
        .allowlist_function("ddsi_typeinfo_.*")
        .allowlist_function("ddsi_typeid_.*")
        .size_t_is_usize(true)
        .merge_extern_blocks(true)
        .generate_comments(true)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

/// 收集 ROS2 系统包中所有 IDL 文件的完整路径。
/// 仅在 `ROS_DISTRO` 为 `jazzy` 或 `humble` 时生效，否则返回空列表。
fn collect_ros_msgs() -> Vec<PathBuf> {
    let distro = env::var("ROS_DISTRO").unwrap_or_default();
    if distro != "jazzy" && distro != "humble" {
        return Vec::new();
    }

    let mut seen_paths = std::collections::HashSet::new();
    let mut result = Vec::new();
    let mut prefix_paths = Vec::new();
    let split_char = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };

    if let Ok(ament_paths) = env::var("AMENT_PREFIX_PATH") {
        prefix_paths.extend(ament_paths.split(split_char).map(String::from));
    }
    if let Ok(cmake_paths) = env::var("CMAKE_PREFIX_PATH") {
        prefix_paths.extend(cmake_paths.split(split_char).map(String::from));
    }
    // AMENT_PREFIX_PATH 与 CMAKE_PREFIX_PATH 常含相同条目，先去重
    prefix_paths.sort_unstable();
    prefix_paths.dedup();

    for prefix_path in &prefix_paths {
        let resource_index_path = Path::new(prefix_path)
            .join("share")
            .join("ament_index")
            .join("resource_index")
            .join("rosidl_interfaces");
        if !resource_index_path.exists() {
            continue;
        }
        let full_path = Path::new(prefix_path).join("share");
        if full_path.exists() && seen_paths.insert(full_path.clone()) {
            result.push(full_path);
        }
    }
    result.sort_unstable();
    result
}

/// 将 ROS2 IDL 文件路径追加到 `DDS_IDL_PATH` 环境变量。
///
/// 若 `ROS_DISTRO` 不是受支持的发行版，或未找到任何 IDL 文件，则不做任何修改。
fn setup_ros_dds_idl_path() {
    let idl_paths = collect_ros_msgs();
    if idl_paths.is_empty() {
        return;
    }
    let sep = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };
    let new_entries = idl_paths
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join(&sep.to_string());
    let new_val = match env::var("DDS_IDL_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{existing}{sep}{new_entries}"),
        _ => new_entries,
    };
    // SAFETY: build scripts are single-threaded
    unsafe { env::set_var("DDS_IDL_PATH", &new_val) };
}

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }
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

    setup_ros_dds_idl_path();
    print_cargo_watches();

    let env_hash = get_env_hash();
    let env_dir = out_dir.join(&env_hash);
    let mark_file = env_dir.join("done");

    if !mark_file.exists() {
        // 生成 DDS API 绑定（bindings.rs）
        generate_dds_bindings(&dds.include_paths, &out_dir);
        compile_idl_libs(MSGS_LIB_NAME, &dds.include_paths, &env_dir);
        generate_msg_bindings(&env_dir, &out_dir);
        generate_rust_wrappers(&out_dir);
        touch(&mark_file);
    } else {
        println!("cargo:warning=Environment variables unchanged, skipping IDL compilation");
    }
    let lib_path = env_dir.join(format!("lib{MSGS_LIB_NAME}.a"));
    if lib_path.exists() {
        println!("cargo:rustc-link-search=native={}", env_dir.display());
        println!("cargo:rustc-link-lib=static={MSGS_LIB_NAME}");
    }
}
