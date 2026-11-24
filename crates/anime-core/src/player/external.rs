use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::{env, io};

use crate::AnimePlaybackServer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalPlayer {
    Vlc,
    Mpv,
}

impl ExternalPlayer {
    fn executable(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Self::Vlc, true) => "vlc.exe",
            (Self::Vlc, false) => "vlc",
            (Self::Mpv, true) => "mpv.exe",
            (Self::Mpv, false) => "mpv",
        }
    }
}

pub struct ExternalPlayerLauncher;

impl ExternalPlayerLauncher {
    pub fn detect(player: ExternalPlayer, custom_path: Option<&Path>) -> Option<PathBuf> {
        let valid = |path: &Path| {
            let Ok(metadata) = path.metadata() else { return false };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            metadata.is_file()
        };

        if let Some(path) = custom_path.filter(|path| !path.as_os_str().is_empty() && valid(path)) {
            return Some(path.to_owned());
        }

        let locations = env::var_os("PATH").unwrap_or_default();
        let typical = if cfg!(windows) {
            let folder = match player {
                ExternalPlayer::Vlc => "VideoLAN/VLC",
                ExternalPlayer::Mpv => "mpv",
            };
            ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
                .into_iter()
                .filter_map(env::var_os)
                .map(|root| PathBuf::from(root).join(folder).join(player.executable()))
                .collect::<Vec<_>>()
        } else if cfg!(target_os = "macos") {
            let path = match player {
                ExternalPlayer::Vlc => "/Applications/VLC.app/Contents/MacOS/VLC",
                ExternalPlayer::Mpv => "/Applications/mpv.app/Contents/MacOS/mpv",
            };
            vec![PathBuf::from(path), Path::new("/opt/homebrew/bin").join(player.executable())]
        } else {
            Vec::new()
        };

        env::split_paths(&locations)
            .filter(|path| path.is_absolute())
            .map(|path| path.join(player.executable()))
            .chain(typical)
            .find(|path| valid(path))
    }

    pub async fn launch(player: ExternalPlayer, custom_path: Option<&Path>, server: &AnimePlaybackServer) -> io::Result<Child> {
        let path = Self::detect(player, custom_path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} was not found. Set its executable path or use the web player.", player.executable()),
            )
        })?;
        let (url, title) = server.external_media().await;
        let title_option = match player {
            ExternalPlayer::Vlc => "--meta-title",
            ExternalPlayer::Mpv => "--force-media-title",
        };

        Command::new(path.canonicalize()?).arg(format!("{title_option}={title}")).arg(url).spawn()
    }
}
