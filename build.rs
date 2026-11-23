use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn emit_git_rerun_directives() {
    let head_path = Path::new(".git/HEAD");
    if !head_path.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head_contents) = std::fs::read_to_string(head_path)
        && let Some(ref_path) = head_contents.trim().strip_prefix("ref: ")
    {
        let target = Path::new(".git").join(ref_path);
        if target.exists() {
            println!("cargo:rerun-if-changed=.git/{ref_path}");
        }
    }
}

fn emit_build_metadata() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=MANGA_TUI_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=MANGA_TUI_BUILD_TIMESTAMP");
    emit_git_rerun_directives();

    let commit = non_empty_env("GITHUB_SHA")
        .or_else(|| run_git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());

    let commit_short: String = if commit == "unknown" { "unknown".to_string() } else { commit.chars().take(7).collect() };

    let timestamp = non_empty_env("MANGA_TUI_BUILD_TIMESTAMP")
        .or_else(|| run_git(&["show", "-s", "--format=%ct", "HEAD"]))
        .filter(|s| s.parse::<i64>().is_ok())
        .unwrap_or_else(|| "0".to_string());

    let channel = non_empty_env("MANGA_TUI_BUILD_CHANNEL").unwrap_or_else(|| "dev".to_string());

    println!("cargo:rustc-env=MANGA_TUI_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=MANGA_TUI_BUILD_COMMIT_SHORT={commit_short}");
    println!("cargo:rustc-env=MANGA_TUI_BUILD_TIMESTAMP={timestamp}");
    println!("cargo:rustc-env=MANGA_TUI_BUILD_CHANNEL={channel}");
}

fn load_dotenv_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(contents) = std::fs::read_to_string(path) else {
        return map;
    };

    for raw_line in contents.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_val)) = trimmed.split_once('=') else {
            continue;
        };
        let key = raw_key.trim().strip_prefix("export ").unwrap_or(raw_key.trim()).trim();
        if key.is_empty() {
            continue;
        }
        let mut val = raw_val.trim();
        if val.len() >= 2 && ((val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\''))) {
            val = &val[1..val.len() - 1];
        }
        let val = val.trim();
        if !val.is_empty() {
            map.insert(key.to_string(), val.to_string());
        }
    }

    map
}

fn emit_cloud_oauth_config() {
    println!("cargo:rerun-if-env-changed=MANGA_TUI_GDRIVE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=MANGA_TUI_GDRIVE_CLIENT_SECRET");
    println!("cargo:rerun-if-env-changed=MANGA_TUI_MAL_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=MANGA_TUI_MAL_CLIENT_SECRET");
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=.env.local");

    let env_base = load_dotenv_file(Path::new(".env"));
    let env_local = load_dotenv_file(Path::new(".env.local"));

    let resolve = |key: &str| -> Option<String> {
        non_empty_env(key)
            .or_else(|| env_local.get(key).cloned())
            .or_else(|| env_base.get(key).cloned())
            .filter(|v| !v.trim().is_empty())
    };

    if let Some(client_id) = resolve("MANGA_TUI_GDRIVE_CLIENT_ID") {
        println!("cargo:rustc-env=MANGA_TUI_EMBEDDED_GDRIVE_CLIENT_ID={client_id}");
    }
    if let Some(client_secret) = resolve("MANGA_TUI_GDRIVE_CLIENT_SECRET") {
        println!("cargo:rustc-env=MANGA_TUI_EMBEDDED_GDRIVE_CLIENT_SECRET={client_secret}");
    }
    if let Some(client_id) = resolve("MANGA_TUI_MAL_CLIENT_ID") {
        println!("cargo:rustc-env=MANGA_TUI_EMBEDDED_MAL_CLIENT_ID={client_id}");
    }
    if let Some(client_secret) = resolve("MANGA_TUI_MAL_CLIENT_SECRET") {
        println!("cargo:rustc-env=MANGA_TUI_EMBEDDED_MAL_CLIENT_SECRET={client_secret}");
    }
}

#[cfg(windows)]
fn embed_windows_resources() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

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

fn main() {
    emit_build_metadata();
    emit_cloud_oauth_config();
    #[cfg(windows)]
    embed_windows_resources();
}
