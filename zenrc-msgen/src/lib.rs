mod msg_gen;

use std::{env, fs};
use std::path::{Path, PathBuf};
use std::process::Command;

pub use crate::msg_gen::generate_rust_wrappers;

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
pub fn compile_idl_libs(lib_name :&str, dds_include_paths: &Vec<PathBuf>, idl_out_dir: &Path) {
    compile_idl_files(&dds_include_paths, idl_out_dir);
    let c_files = collect_c_files(idl_out_dir);
    if c_files.is_empty() {
        return;
    }

    let mut build = cc::Build::new();
    for inc in dds_include_paths {
        build.include(inc);
    }
    build.include(idl_out_dir);
    for f in &c_files {
        build.file(f);
    }
    build.out_dir(idl_out_dir);
    build.compile(lib_name);
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
pub fn generate_msg_bindings(idl_out_dir: &Path, out_dir: &Path) {
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
fn find_idlc() -> Option<PathBuf> {
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
fn compile_idl_files(dds_include_paths: &Vec<PathBuf>, out_dir: &Path) {
    let idl_files = collect_idl_files();
    if idl_files.is_empty() {
        return;
    }
    let Some(idlc) = find_idlc() else {
        println!("cargo:warning=idlc not found, skipping IDL compilation");
        return;
    };

    let mut include_dirs: Vec<PathBuf> = dds_include_paths.clone();
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
            Ok(_) => {}
            Err(e) => println!(
                "cargo:warning=Failed to run idlc for {}: {e}",
                entry.path.display()
            )
        }
    }
}