#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let version = env!("CARGO_PKG_VERSION");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Manga TUI");
        res.set("CompanyName", "tomahawk1103 & drodri22");
        res.set("FileDescription", "Terminal-based manga reader and downloader with image rendering support");
        res.set("FileVersion", version);
        res.set("ProductVersion", version);
        res.set("OriginalFilename", "manga-tui.exe");
        res.set("InternalName", "manga-tui");
        res.set("LegalCopyright", "Copyright (c) 2026 tomahawk1103 & drodri22 (MIT License)");
        res.compile().expect("Failed to embed Windows icon and version metadata");
    }
}

#[cfg(not(windows))]
fn main() {}
