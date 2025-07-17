use std::env;
use std::path::PathBuf;

fn main() {
    // Get the Binary Ninja installation directory
    let binja_dir = env::var("BINJA_DIR").unwrap_or_else(|_| {
        // Default paths for different platforms
        if cfg!(target_os = "macos") {
            "/Applications/Binary Ninja.app/Contents/MacOS".to_string()
        } else if cfg!(target_os = "linux") {
            "/opt/binaryninja".to_string()
        } else if cfg!(target_os = "windows") {
            "C:\\Program Files\\Vector35\\BinaryNinja".to_string()
        } else {
            panic!("Unsupported platform")
        }
    });

    // Link to binaryninjacore
    println!("cargo:rustc-link-search=native={}", binja_dir);
    println!("cargo:rustc-link-lib=dylib=binaryninjacore");

    // Set up include path for headers
    let include_path = PathBuf::from(&binja_dir).join("include");
    println!("cargo:include={}", include_path.display());

    // Tell cargo to rerun this script if the environment variable changes
    println!("cargo:rerun-if-env-changed=BINJA_DIR");
    
    // Add rpath on macOS and Linux
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", binja_dir);
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", binja_dir);
    }
}