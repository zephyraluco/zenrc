use std::env;
use std::path::{Path, PathBuf};

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
        Err(e) => {
            panic!("Failed to find CycloneDDS via pkg-config: {}", e);
        }
    };

    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    gen_bindings(&dds, &out_dir);

    for path in &dds.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for lib in &dds.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
}
