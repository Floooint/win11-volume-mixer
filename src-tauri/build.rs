fn main() {
    // Tauri 依赖 Common-Controls v6，但默认只把清单嵌入主程序。
    // 改为通过链接参数嵌入所有目标（含 `cargo test` 生成的测试程序），
    // 否则测试程序启动时会报 STATUS_ENTRYPOINT_NOT_FOUND。
    let manifest = std::env::current_dir()
        .expect("无法获取当前目录")
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.to_str().expect("清单路径不是有效的 UTF-8")
    );

    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build 失败");
}
