mod msg_gen;
use msg_gen::*;
use os_str_bytes::RawOsString;
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

const SUPPORTED_ROS_DISTROS: &[&str] = &["foxy", "galactic", "humble", "iron", "jazzy", "rolling"];

const WATCHED_ENV_VARS: &[&str] = &[
    "AMENT_PREFIX_PATH",
    "CMAKE_PREFIX_PATH",
    "CMAKE_IDL_PACKAGES",
    "IDL_PACKAGE_FILTER",
    "ROS_DISTRO",
];

const MSG_INCLUDES_NAME: &str = "msg_includes.h";
const INTROSPECTION_MAP_NAME: &str = "introspection_maps.rs";
const RCL_BINDINGS_NAME: &str = "rcl_bindings.rs";

fn main() {
    println!("正在生成绑定文件...");
    print_cargo_watches();
    print_cargo_ros_distro();
    let ros_msgs = collect_ros_msgs();
    generate_includes(MSG_INCLUDES_NAME, &ros_msgs);
    generate_introspection_map(INTROSPECTION_MAP_NAME, &ros_msgs);
    run_bindgen();
    run_dynlink(&ros_msgs);
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
    format!("{:x}", hash)
}

fn print_cargo_watches() {
    for var in WATCHED_ENV_VARS {
        println!("cargo:rerun-if-env-changed={}", var);
    }
}

fn setup_bindgen_builder() -> bindgen::Builder {
    let mut builder = bindgen::Builder::default()
        .layout_tests(false)
        .derive_copy(false)
        .size_t_is_usize(true)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        });

    if let Ok(cmake_includes) = env::var("CMAKE_INCLUDE_DIRS") {
        let mut includes = cmake_includes.split(':').collect::<Vec<_>>();
        includes.sort_unstable();
        includes.dedup();

        for x in &includes {
            let clang_arg = format!("-I{}", x);
            println!("adding clang arg: {}", clang_arg);
            builder = builder.clang_arg(clang_arg);
        }
    }

    let ament_prefix_var_name = "AMENT_PREFIX_PATH";
    let split_char = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };
    let ament_prefix_var = {
        let mut ament_str = env::var_os(ament_prefix_var_name).expect("Source your ROS!");
        if let Some(cmake_prefix_var) = env::var_os("CMAKE_PREFIX_PATH") {
            ament_str.push(&split_char.to_string());
            ament_str.push(cmake_prefix_var);
        }
        RawOsString::new(ament_str)
    };
    for p in ament_prefix_var.split(split_char) {
        let path = Path::new(&p.as_os_str()).join("include");

        let entries = std::fs::read_dir(path.clone());
        if let Ok(e) = entries {
            let dirs = e
                .filter_map(|a| {
                    let path = a.unwrap().path();
                    if path.is_dir() { Some(path) } else { None }
                })
                .collect::<Vec<_>>();

            // 设置 include 路径，支持双层 include（如 rcl/include/rcl 和 rcl/include/rcl/rcl）
            builder = dirs.iter().fold(builder, |builder, d| {
                if let Some(leaf) = d.file_name() {
                    let double_include_path = Path::new(d).join(leaf);
                    if double_include_path.is_dir() {
                        let temp = d.to_str().unwrap();
                        builder.clang_arg(format!("-I{}", temp))
                    } else {
                        let temp = d.parent().unwrap().to_str().unwrap();
                        builder.clang_arg(format!("-I{}", temp))
                    }
                } else {
                    builder
                }
            });
        }
    }

    builder
}

fn print_cargo_ros_distro() {
    let ros_distro =
        env::var("ROS_DISTRO").unwrap_or_else(|_| panic!("ROS_DISTRO not set: Source your ROS!"));

    if SUPPORTED_ROS_DISTROS.contains(&ros_distro.as_str()) {
        println!("cargo:rustc-cfg=r2r__ros__distro__{ros_distro}");
    } else {
        panic!("ROS_DISTRO not supported: {ros_distro}");
    }
}

fn print_cargo_link_search() {
    let ament_prefix_var_name = "AMENT_PREFIX_PATH";
    if let Some(paths) = env::var_os(ament_prefix_var_name) {
        let split_char = if cfg!(target_os = "windows") {
            ';'
        } else {
            ':'
        };
        let paths = if let Some(cmake_prefix_var) = env::var_os("CMAKE_PREFIX_PATH") {
            let mut cmake_paths = paths;
            cmake_paths.push(split_char.to_string());
            cmake_paths.push(cmake_prefix_var);
            RawOsString::new(cmake_paths)
        } else {
            RawOsString::new(paths)
        };
        for path in paths.split(split_char) {
            if cfg!(target_os = "windows") {
                let lib_path = Path::new(&path.as_os_str()).join("Lib");
                if !lib_path.exists() {
                    continue;
                }
                if let Some(s) = lib_path.to_str() {
                    println!("cargo:rustc-link-search={}", s);
                }
            } else {
                let lib_path = Path::new(&path.as_os_str()).join("lib");
                if let Some(s) = lib_path.to_str() {
                    println!("cargo:rustc-link-search=native={}", s)
                }
            }
        }
    }
}

fn run_bindgen() {
    let env_hash = get_env_hash();
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let bindgen_dir = out_dir.join(env_hash);
    let mark_file = bindgen_dir.join("done");
    let target_file = out_dir.join(RCL_BINDINGS_NAME);

    if !mark_file.exists() {
        eprintln!("Generate bindings file '{}'", target_file.display());
        gen_bindings(&target_file);
        touch(&mark_file);
    } else {
        eprintln!("using last generated: {}", target_file.display());
    }
}

fn run_dynlink(ros_msgs: &[RosMsg]) {
    print_cargo_link_search();
    // rcl 链接路径
    println!("cargo:rustc-link-lib=dylib=rcl");
    println!("cargo:rustc-link-lib=dylib=rcl_logging_spdlog");
    println!("cargo:rustc-link-lib=dylib=rcl_yaml_param_parser");
    println!("cargo:rustc-link-lib=dylib=rcutils");
    println!("cargo:rustc-link-lib=dylib=rmw");
    println!("cargo:rustc-link-lib=dylib=rmw_implementation");
    println!("cargo:rustc-link-lib=dylib=rosidl_typesupport_c");
    println!("cargo:rustc-link-lib=dylib=rosidl_runtime_c");

    // msg/srv/action 类型链接路径
    print_msg_link_libs(ros_msgs);
}

fn gen_bindings(out_file: &Path) {
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let includes_file = out_dir.join(MSG_INCLUDES_NAME);
    let bindings = setup_bindgen_builder()
        .header("wrapper.hpp")
        .header(includes_file.to_str().unwrap())
        // msg/srv/action 相关的函数和类型
        .allowlist_function(r"[\w_]*__(msg|srv|action)__[\w_]*__(create|destroy)")
        .allowlist_function(r"[\w_]*__(msg|srv|action)__[\w_]*__Sequence__(init|fini)")
        .allowlist_var(r"[\w_]*__(msg|srv|action)__[\w_]*__[\w_]*")
        // rcl、rcutils、rmw、rosidl相关的函数和类型
        .allowlist_type("rcl_.*")
        .allowlist_type("rcutils_.*")
        .allowlist_type("rmw_.*")
        .allowlist_type("rosidl_.*")
        .allowlist_type("RCUTILS_.*")
        .allowlist_var("RCL_.*")
        .allowlist_var("RCUTILS_.*")
        .allowlist_var("RMW_.*")
        .allowlist_var("rosidl_.*")
        .allowlist_var("g_rcutils_.*")
        .allowlist_function("rcl_.*")
        .allowlist_function("rcutils_.*")
        .allowlist_function("rmw_.*")
        .allowlist_function("rosidl_.*")
        .allowlist_function(".*_typesupport_.*")
        .allowlist_function(".*_sequence_bound_.*")
        .no_debug("_OSUnaligned.*")
        .size_t_is_usize(true)
        .merge_extern_blocks(true)
        .derive_partialeq(true)
        .derive_copy(true)
        .generate_comments(false)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_file)
        .expect("Couldn't write bindings!");
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
