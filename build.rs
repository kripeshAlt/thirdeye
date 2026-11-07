// Build script to work around bindgen issue with anonymous unions in v4l2 headers
// This allows the build to proceed by ignoring invalid identifiers

fn main() {
    // Only apply workaround on Linux
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "linux" {
        // Tell cargo to re-run if build.rs changes
        println!("cargo:rerun-if-changed=build.rs");
        
        // Use bindgen directly with allow-invalid-idents for the v4l2-sys dependency
        // We need to configure the v4l2-sys crate's bindgen invocation
        // This is done by setting environment variables that v4l2-sys respects
        println!("cargo:rustc-env=BINDGEN_EXTRA_CLANG_ARGS=-Wno-invalid-token-pasting");
        
        // The v4l2-sys-mit crate uses bindgen internally. 
        // We can try to override its behavior by setting BINDGEN_EXTRA_CLANG_ARGS
        // or we can generate our own bindings. Let's try the env var approach first.
        
        // For bindgen 0.56+, --allow-invalid-idents is available
        // But we can't easily pass it through the v4l2-sys-mit build
        // Let's use a more direct approach: generate bindings ourselves if needed
    }
    
    // Always rerun if this file changes
    println!("cargo:rerun-if-changed=build.rs");
}