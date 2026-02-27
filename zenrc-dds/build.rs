use std::path::PathBuf;

fn main() {
    // 使用 pkg-config 查找 cyclonedds
    let include_paths = match pkg_config::Config::new().probe("CycloneDDS") {
        Ok(lib) => lib.include_paths,
        Err(e) => {
            panic!("Failed to find CycloneDDS via pkg-config: {}", e);
        }
    };

    // 生成 Rust 绑定
    let bindings = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_args(
            include_paths
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
}
