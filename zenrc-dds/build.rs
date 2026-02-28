use std::path::PathBuf;

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
}
