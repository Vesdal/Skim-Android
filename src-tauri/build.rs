fn main() {
    // Recent Android (15+, e.g. Galaxy S25 series) runs kernels with 16 KiB
    // pages: a native lib linked with 4 KiB alignment fails dlopen and the app
    // dies at launch with no message. rustc/LLVM default to 4 KiB, so force
    // 16 KiB segments for Android targets — harmless on 4 KiB kernels too.
    if std::env::var("TARGET").map(|t| t.contains("android")).unwrap_or(false) {
        println!("cargo::rustc-link-arg=-Wl,-z,max-page-size=16384");
    }
    tauri_build::build()
}
