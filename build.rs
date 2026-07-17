fn main() {
    // Embed icon + version info into the Windows .exe
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/icon.ico");
        res.set("ProductName", "蜜桃成熟");
        res.set("FileDescription", "蜜桃成熟 — 随机片库多开");
        res.set("LegalCopyright", "蜜桃成熟");
        res.set("OriginalFilename", "蜜桃成熟.exe");
        if let Err(e) = res.compile() {
            // Don't fail cross-compiles; local Windows builds should succeed
            println!("cargo:warning=winresource failed: {e}");
        }
    }
    println!("cargo:rerun-if-changed=src/icon.ico");
}
