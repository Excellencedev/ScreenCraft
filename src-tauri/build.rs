fn main() {
    // The ScreenCaptureKit bridge links the Swift runtime through @rpath.
    // The system copy lives in /usr/lib/swift on macOS 12+.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
    tauri_build::build()
}
