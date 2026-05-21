fn main() {
    // Link macOS frameworks needed by the helper module
    // ServiceManagement is loaded dynamically at runtime (available macOS 13+)
    println!("cargo:rustc-link-lib=framework=Security");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    tauri_build::build()
}