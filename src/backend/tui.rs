use std::error::Error;
use std::time::Duration;

use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};
use futures::{FutureExt, StreamExt};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui_image::picker::{Picker, ProtocolType};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use super::manga_provider::{ChapterToRead, Manga, MangaProvider};
use super::tracker::MangaTracker;
use crate::view::app::{App, AppState, MangaToRead};
use crate::view::widgets::Component;

pub enum Action {
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewChapterNotificationGroup {
    pub manga_id: String,
    pub manga_title: String,
    pub provider: super::manga_provider::MangaProviders,
    pub chapter_numbers: String,
    pub chapters: Vec<super::manga_provider::LatestChapter>,
}

/// These are the events this app will listen to
#[derive(Clone, Debug, PartialEq)]
pub enum Events {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent),
    GoToMangaPage(Manga),
    GoToOfflineMangaPage(crate::backend::offline::OfflineManga),
    GoBackMangaPage,
    GoToHome,
    GoSearchPage,
    GoFeedPage,
    Error(String),
    CheckForNewDownloadedChapters,
    NewChaptersNotification(Vec<NewChapterNotificationGroup>),
    NewChaptersDownloadFinished(Result<usize, String>),
    AutoDownloadedFavorites {
        count: usize,
        details: Vec<String>,
    },
    ReadChapter(ChapterToRead, MangaToRead),
    EnqueueDownloadBatch(super::download_queue::EnqueueDownloadBatch),
    DownloadQueueProgress(super::download_queue::DownloadQueueProgress),
    DownloadQueueFinished(usize),
    DownloadQueueChapterFinished {
        manga_id: String,
        chapter_id: String,
    },
    SearchAllProviders(String),
    ProviderCheckTrying(super::manga_provider::MangaProviders),
    ProviderCheckFinished {
        active_provider: super::manga_provider::MangaProviders,
        switched_from: Option<super::manga_provider::MangaProviders>,
        all_failed: bool,
    },
    ImageProtocolChanged(crate::config::ImageProtocol),
    AppUpdateAvailable(crate::backend::updater::AvailableUpdate),
    AppUpdateCheckStatus {
        message: String,
        is_error: bool,
    },
    AppUpdateDownloadProgress(crate::backend::updater::UpdateProgress),
    AppUpdateReadyToInstall(crate::backend::updater::PreparedUpdate),
    AppUpdateFailed(String),
    CloudSyncProgress(crate::backend::cloud::CloudSyncProgress),
    CloudSyncFinished(Result<crate::backend::cloud::CloudSyncReport, String>),
    CloudOAuthFinished(Result<crate::backend::cloud::CloudConnectionState, String>),
}

impl From<super::download_queue::DownloadQueueEvent> for Events {
    fn from(event: super::download_queue::DownloadQueueEvent) -> Self {
        match event {
            super::download_queue::DownloadQueueEvent::Progress(progress) => Self::DownloadQueueProgress(progress),
            super::download_queue::DownloadQueueEvent::ChapterFinished {
                manga_id,
                chapter_id,
            } => Self::DownloadQueueChapterFinished {
                manga_id,
                chapter_id,
            },
            super::download_queue::DownloadQueueEvent::Finished(total) => Self::DownloadQueueFinished(total),
        }
    }
}

static MEASURED_FONT_SIZE: std::sync::OnceLock<(u16, u16)> = std::sync::OnceLock::new();

pub fn resolve_protocol_override(
    env_proto: Option<&str>,
    config_proto: crate::config::ImageProtocol,
) -> Option<Option<ProtocolType>> {
    if let Some(raw) = env_proto.map(str::trim).filter(|s| !s.is_empty()) {
        match raw.to_ascii_lowercase().as_str() {
            "sixel" => return Some(Some(ProtocolType::Sixel)),
            "iterm2" | "iterm" => return Some(Some(ProtocolType::Iterm2)),
            "kitty" => return Some(Some(ProtocolType::Kitty)),
            "halfblocks" => return Some(Some(ProtocolType::Halfblocks)),
            "none" | "off" => return Some(None),
            "auto" => return None,
            _ => {},
        }
    }

    match config_proto {
        crate::config::ImageProtocol::Auto => None,
        crate::config::ImageProtocol::Sixel => Some(Some(ProtocolType::Sixel)),
        crate::config::ImageProtocol::Halfblocks => Some(Some(ProtocolType::Halfblocks)),
        crate::config::ImageProtocol::Kitty => Some(Some(ProtocolType::Kitty)),
        crate::config::ImageProtocol::Iterm2 => Some(Some(ProtocolType::Iterm2)),
        crate::config::ImageProtocol::None => Some(None),
    }
}

pub fn build_picker_with_font_size(
    font_size: (u16, u16),
    is_modern_terminal: bool,
    proto: crate::config::ImageProtocol,
) -> Option<Picker> {
    let mut picker = Picker::new(font_size);
    match proto {
        crate::config::ImageProtocol::None => None,
        crate::config::ImageProtocol::Sixel => {
            picker.protocol_type = ProtocolType::Sixel;
            Some(picker)
        },
        crate::config::ImageProtocol::Halfblocks => {
            picker.protocol_type = ProtocolType::Halfblocks;
            Some(picker)
        },
        crate::config::ImageProtocol::Kitty => {
            picker.protocol_type = ProtocolType::Kitty;
            Some(picker)
        },
        crate::config::ImageProtocol::Iterm2 => {
            picker.protocol_type = ProtocolType::Iterm2;
            Some(picker)
        },
        crate::config::ImageProtocol::Auto => {
            let guessed = picker.guess_protocol();
            #[cfg(target_os = "windows")]
            {
                let is_vscode_conpty = std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false);
                if is_vscode_conpty || (guessed == ProtocolType::Halfblocks && is_modern_terminal) {
                    picker.protocol_type = ProtocolType::Sixel;
                    Some(picker)
                } else if guessed == ProtocolType::Halfblocks {
                    // Avoid unreadable Halfblocks on classic conhost.exe: fall back to Visor Web HD (`None`).
                    None
                } else {
                    Some(picker)
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = is_modern_terminal;
                if guessed == ProtocolType::Halfblocks { None } else { Some(picker) }
            }
        },
    }
}

/// Returns a safe fallback cell `(width, height)` in pixels on Windows when `CSI 16t` is
/// unavailable. Keeps a 1:2 aspect ratio and never exceeds Windows Terminal's 10x20 virtual Sixel grid.
pub fn safe_windows_fallback_font_size(is_modern_terminal: bool, is_vscode: bool) -> (u16, u16) {
    if is_modern_terminal && !is_vscode { (9, 18) } else { (8, 16) }
}

/// Builds a `Picker` for `proto` at runtime using the cached startup font size without querying stdin.
pub fn picker_for_protocol(proto: crate::config::ImageProtocol) -> Option<Picker> {
    let is_modern = is_running_in_modern_windows_terminal();
    let fallback_size = if cfg!(target_os = "windows") {
        let is_vscode = std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false);
        safe_windows_fallback_font_size(is_modern, is_vscode)
    } else {
        (8, 16)
    };
    let font_size = *MEASURED_FONT_SIZE.get().unwrap_or(&fallback_size);
    build_picker_with_font_size(font_size, is_modern, proto)
}

#[cfg(unix)]
pub fn get_picker() -> Option<Picker> {
    let picker_from_termios = Picker::from_termios().ok();
    let size = picker_from_termios.as_ref().map(|p| p.font_size).unwrap_or((8, 16));
    let _ = MEASURED_FONT_SIZE.set(size);

    let env_proto = std::env::var("MANGA_TUI_PROTOCOL").ok();
    let config_proto = crate::view::widgets::settings::active_image_protocol();
    match resolve_protocol_override(env_proto.as_deref(), config_proto) {
        Some(Some(proto)) => {
            let mut picker = picker_from_termios.unwrap_or_else(|| Picker::new(size));
            picker.protocol_type = proto;
            return Some(picker);
        },
        Some(None) => return None,
        None => {},
    }

    picker_from_termios
        .map(|mut picker| {
            picker.guess_protocol();
            picker
        })
        .filter(|picker| picker.protocol_type != ProtocolType::Halfblocks)
}

/// Returns `true` when the environment variables indicate a modern Windows terminal emulator
/// (such as Windows Terminal, VS Code integrated terminal, WezTerm, Alacritty, etc.) rather than
/// classic `conhost.exe`.
pub fn is_modern_windows_terminal_env(wt_session: Option<&str>, term_program: Option<&str>, wezterm_exe: Option<&str>) -> bool {
    if wt_session.is_some_and(|v| !v.trim().is_empty()) {
        return true;
    }

    if wezterm_exe.is_some_and(|v| !v.trim().is_empty()) {
        return true;
    }

    const MODERN_TERM_PROGRAMS: [&str; 8] = ["vscode", "wezterm", "mintty", "rio", "alacritty", "tabby", "hyper", "kitty"];

    term_program
        .map(str::trim)
        .is_some_and(|prog| MODERN_TERM_PROGRAMS.iter().any(|known| prog.eq_ignore_ascii_case(known)))
}

/// Detects ConPTY / Windows 11 DefTerm sessions where `WT_SESSION` may not be present (e.g.
/// double-clicking an executable from Explorer when Windows Terminal is the default terminal).
/// Classic `conhost.exe` attaches a visible console window (`GetConsoleWindow` non-null and visible),
/// whereas ConPTY uses no console window or a hidden pseudo-console window.
#[cfg(target_os = "windows")]
pub fn is_conpty_or_pseudo_console() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;

    let hwnd = unsafe { GetConsoleWindow() };
    hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0
}

/// Checks current process environment variables (and ConPTY window state on Windows) to detect
/// whether the app is running inside a modern Windows terminal emulator.
pub fn is_running_in_modern_windows_terminal() -> bool {
    let wt_session = std::env::var("WT_SESSION").ok();
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let wezterm_exe = std::env::var("WEZTERM_EXECUTABLE").ok();

    let is_modern_env = is_modern_windows_terminal_env(wt_session.as_deref(), term_program.as_deref(), wezterm_exe.as_deref());

    #[cfg(target_os = "windows")]
    {
        is_modern_env || is_conpty_or_pseudo_console()
    }
    #[cfg(not(target_os = "windows"))]
    {
        is_modern_env
    }
}

/// Parses a terminal CSI 16t cell-pixel-size report (`CSI 6 ; <height> ; <width> t`) and
/// returns `(cell_width, cell_height)` when both dimensions are within sane bounds.
pub fn parse_csi_cell_size_response(buf: &str) -> Option<(u16, u16)> {
    let start = buf.find("\x1b[6;")?;
    let rest = &buf[start + 4..];
    let end = rest.find('t')?;
    let body = &rest[..end];
    let (h_str, w_str) = body.split_once(';')?;
    let h: u16 = h_str.trim().parse().ok()?;
    let w: u16 = w_str.trim().parse().ok()?;
    if (4..=64).contains(&w) && (8..=128).contains(&h) { Some((w, h)) } else { None }
}

#[cfg(target_os = "windows")]
fn query_windows_terminal_cell_size() -> Option<(u16, u16)> {
    use std::io::Write;
    use std::thread;

    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetNumberOfConsoleInputEvents, GetStdHandle, INPUT_RECORD, KEY_EVENT, PeekConsoleInputW, ReadConsoleInputW,
        STD_INPUT_HANDLE,
    };

    let stdin_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if stdin_handle.is_null() || stdin_handle == INVALID_HANDLE_VALUE {
        return None;
    }

    // Drain any queued non-key-down events (focus/resize/key-up from launch) before probing.
    loop {
        let mut count = 0u32;
        if unsafe { GetNumberOfConsoleInputEvents(stdin_handle, &mut count) } == 0 || count == 0 {
            break;
        }
        let mut peek_record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut read_count = 0u32;
        if unsafe { PeekConsoleInputW(stdin_handle, &mut peek_record, 1, &mut read_count) } == 0 || read_count == 0 {
            break;
        }
        let is_key_down = peek_record.EventType as u32 == KEY_EVENT && unsafe { peek_record.Event.KeyEvent.bKeyDown } != 0;
        if is_key_down {
            break;
        }
        let mut discard: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut got = 0u32;
        if unsafe { ReadConsoleInputW(stdin_handle, &mut discard, 1, &mut got) } == 0 || got == 0 {
            break;
        }
    }

    let mut stdout = std::io::stdout();
    stdout.write_all(b"\x1b[16t").ok()?;
    stdout.flush().ok()?;

    let mut found_escape = false;
    // Drain unrelated startup events (FOCUS_EVENT, WINDOW_BUFFER_SIZE_EVENT, key-up from Enter)
    // until the CSI 16t ESC prefix arrives or the 150ms polling budget expires.
    for _ in 0..75 {
        let mut count = 0u32;
        if unsafe { GetNumberOfConsoleInputEvents(stdin_handle, &mut count) } == 0 {
            return None;
        }
        if count == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let mut peek_record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut read_count = 0u32;
        if unsafe { PeekConsoleInputW(stdin_handle, &mut peek_record, 1, &mut read_count) } == 0 || read_count == 0 {
            return None;
        }
        if peek_record.EventType as u32 != KEY_EVENT {
            let mut discard: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut got = 0u32;
            let _ = unsafe { ReadConsoleInputW(stdin_handle, &mut discard, 1, &mut got) };
            continue;
        }
        let is_key_down = unsafe { peek_record.Event.KeyEvent.bKeyDown } != 0;
        let ch_u16 = unsafe { peek_record.Event.KeyEvent.uChar.UnicodeChar };
        if !is_key_down {
            let mut discard: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut got = 0u32;
            let _ = unsafe { ReadConsoleInputW(stdin_handle, &mut discard, 1, &mut got) };
            continue;
        }
        if ch_u16 == 0x1b {
            found_escape = true;
            break;
        }
        return None;
    }
    if !found_escape {
        return None;
    }

    let mut response = String::new();
    let mut chars_collected = 0usize;
    for _ in 0..64 {
        let mut avail = 0u32;
        if unsafe { GetNumberOfConsoleInputEvents(stdin_handle, &mut avail) } == 0 {
            break;
        }
        if avail == 0 {
            thread::sleep(Duration::from_millis(2));
            if unsafe { GetNumberOfConsoleInputEvents(stdin_handle, &mut avail) } == 0 || avail == 0 {
                break;
            }
        }
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut got = 0u32;
        if unsafe { ReadConsoleInputW(stdin_handle, &mut record, 1, &mut got) } == 0 || got == 0 {
            break;
        }
        if record.EventType as u32 == KEY_EVENT && unsafe { record.Event.KeyEvent.bKeyDown } != 0 {
            let code_unit = unsafe { record.Event.KeyEvent.uChar.UnicodeChar };
            if let Some(ch) = char::from_u32(code_unit as u32)
                && ch != '\0'
            {
                response.push(ch);
                chars_collected += 1;
                if ch == 't' || chars_collected >= 32 {
                    break;
                }
            }
        }
    }

    parse_csi_cell_size_response(&response)
}

#[cfg(target_os = "windows")]
fn measure_windows_cell_size() -> (u16, u16) {
    let is_modern = is_running_in_modern_windows_terminal();
    let is_vscode = std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false);
    if is_modern {
        query_windows_terminal_cell_size().unwrap_or_else(|| safe_windows_fallback_font_size(true, is_vscode))
    } else {
        safe_windows_fallback_font_size(false, is_vscode)
    }
}

/// Probes and caches the terminal character-cell size before mouse capture is enabled on Windows.
#[cfg(target_os = "windows")]
pub fn probe_terminal_cell_size() {
    if MEASURED_FONT_SIZE.get().is_none() {
        let size = measure_windows_cell_size();
        let _ = MEASURED_FONT_SIZE.set(size);
    }
}

/// Probes and caches the terminal character-cell size before mouse capture is enabled.
#[cfg(not(target_os = "windows"))]
pub fn probe_terminal_cell_size() {}

#[cfg(target_os = "windows")]
pub fn get_picker() -> Option<Picker> {
    let is_modern_terminal = is_running_in_modern_windows_terminal();
    let size = *MEASURED_FONT_SIZE.get_or_init(measure_windows_cell_size);

    let mut picker = Picker::new(size);

    let env_proto = std::env::var("MANGA_TUI_PROTOCOL").ok();
    let config_proto = crate::view::widgets::settings::active_image_protocol();
    match resolve_protocol_override(env_proto.as_deref(), config_proto) {
        Some(Some(proto)) => {
            picker.protocol_type = proto;
            return Some(picker);
        },
        Some(None) => return None,
        None => {},
    }

    build_picker_with_font_size(size, is_modern_terminal, crate::config::ImageProtocol::Auto)
}

///Start app's main loop
pub async fn run_app<T: MangaProvider + crate::view::provider_filters::TuiSearchProvider>(
    mut terminal: Terminal<impl Backend>,
    api_client: T,
    manga_tracker: Option<impl MangaTracker>,
    filter_state: T::FiltersHandler,
    filter_widget: T::Widget,
) -> Result<Option<crate::backend::updater::PreparedUpdate>, Box<dyn Error>> {
    let mut app = App::new(api_client, manga_tracker, get_picker(), filter_state, filter_widget);
    app.spawn_startup_provider_check();
    app.spawn_startup_update_check();
    app.check_for_new_downloaded_chapters();

    let tick_rate = std::time::Duration::from_millis(250);

    let main_event_handle = handle_events(tick_rate, app.global_event_tx.clone());

    let mut was_overlay_open = false;
    let mut last_tab = app.current_tab;

    while app.state == AppState::Runnning {
        let is_overlay_open = app.is_overlay_open();
        // Force a full terminal clear when opening/closing overlays or switching tabs so
        // graphics-protocol cells (which ratatui-image marks as `skip`) don't retain stale modal text.
        if was_overlay_open != is_overlay_open || app.current_tab != last_tab || app.take_needs_terminal_clear() {
            terminal.clear()?;
        }
        was_overlay_open = is_overlay_open;
        last_tab = app.current_tab;

        terminal.draw(|f| {
            app.render(f.area(), f);
        })?;

        app.listen_to_event().await;

        app.update_based_on_action();
        app.drain_pending_events();
    }

    main_event_handle.abort();

    Ok(app.pending_update_install.take())
}

pub fn handle_events(tick_rate: Duration, event_tx: UnboundedSender<Events>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = crossterm::event::EventStream::new();
        let mut tick_interval = tokio::time::interval(tick_rate);

        loop {
            let delay = tick_interval.tick();
            let event = reader.next().fuse();
            tokio::select! {
                maybe_event = event => {
                    match maybe_event {
                        Some(Ok(evt)) => {
                            let sent = match evt {
                                crossterm::event::Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                                    event_tx.send(Events::Key(key)).is_ok()
                                },
                                // Mouse movement / drag events are discarded, only clicks and scroll are relevant
                                crossterm::event::Event::Mouse(mouse_event)
                                    if !matches!(mouse_event.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) =>
                                {
                                    event_tx.send(Events::Mouse(mouse_event)).is_ok()
                                },
                                // Redraw immediately so images and layouts adapt to the new size
                                crossterm::event::Event::Resize(_, _) => event_tx.send(Events::Tick).is_ok(),
                                _ => true,
                            };
                            if !sent {
                                break;
                            }
                        },
                        Some(Err(_)) => {},
                        // The event stream is finished, polling it again would return `None` immediately
                        // causing a busy loop
                        None => break,
                    }
                },
                _ = delay => {
                    if event_tx.send(Events::Tick).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_windows_terminal_detection_recognizes_known_terminals() {
        assert!(is_modern_windows_terminal_env(Some("guid-123"), None, None));
        assert!(is_modern_windows_terminal_env(None, None, Some("C:\\Program Files\\WezTerm\\wezterm-gui.exe")));
        assert!(is_modern_windows_terminal_env(None, Some("vscode"), None));
        assert!(is_modern_windows_terminal_env(None, Some("WezTerm"), None));
        assert!(is_modern_windows_terminal_env(None, Some("Alacritty"), None));
        assert!(is_modern_windows_terminal_env(None, Some("mintty"), None));
        assert!(is_modern_windows_terminal_env(None, Some("rio"), None));
        assert!(is_modern_windows_terminal_env(None, Some("tabby"), None));
        assert!(is_modern_windows_terminal_env(None, Some("hyper"), None));
        assert!(is_modern_windows_terminal_env(None, Some("kitty"), None));
    }

    #[test]
    fn modern_windows_terminal_detection_rejects_classic_console_env() {
        assert!(!is_modern_windows_terminal_env(None, None, None));
        assert!(!is_modern_windows_terminal_env(Some("   "), Some(""), Some("")));
        assert!(!is_modern_windows_terminal_env(None, Some("cmd"), None));
    }

    #[test]
    fn resolve_protocol_override_respects_env_precedence_and_config_fallback() {
        use crate::config::ImageProtocol;

        // MANGA_TUI_PROTOCOL overrides config.toml
        assert_eq!(resolve_protocol_override(Some("halfblocks"), ImageProtocol::Sixel), Some(Some(ProtocolType::Halfblocks)));
        assert_eq!(resolve_protocol_override(Some("SIXEL"), ImageProtocol::Auto), Some(Some(ProtocolType::Sixel)));
        assert_eq!(resolve_protocol_override(Some("kitty"), ImageProtocol::Auto), Some(Some(ProtocolType::Kitty)));
        assert_eq!(resolve_protocol_override(Some("iterm2"), ImageProtocol::Auto), Some(Some(ProtocolType::Iterm2)));
        assert_eq!(resolve_protocol_override(Some("none"), ImageProtocol::Sixel), Some(None));
        assert_eq!(resolve_protocol_override(Some("off"), ImageProtocol::Sixel), Some(None));
        assert_eq!(resolve_protocol_override(Some("auto"), ImageProtocol::Sixel), None);

        // Blank or unknown env var falls back to config.toml
        assert_eq!(resolve_protocol_override(None, ImageProtocol::Auto), None);
        assert_eq!(resolve_protocol_override(Some("   "), ImageProtocol::Sixel), Some(Some(ProtocolType::Sixel)));
        assert_eq!(resolve_protocol_override(Some("invalid"), ImageProtocol::Halfblocks), Some(Some(ProtocolType::Halfblocks)));
        assert_eq!(resolve_protocol_override(None, ImageProtocol::None), Some(None));
    }

    #[test]
    fn parse_csi_cell_size_response_handles_valid_and_invalid_reports() {
        assert_eq!(parse_csi_cell_size_response("\x1b[6;16;8t"), Some((8, 16)));
        assert_eq!(parse_csi_cell_size_response("noise\x1b[6;19;9tmore"), Some((9, 19)));
        assert_eq!(parse_csi_cell_size_response("\x1b[6;0;8t"), None);
        assert_eq!(parse_csi_cell_size_response("\x1b[6;16;200t"), None);
        assert_eq!(parse_csi_cell_size_response("not a csi response"), None);
    }

    #[test]
    fn build_picker_with_font_size_applies_all_protocol_modes() {
        use crate::config::ImageProtocol;

        assert!(build_picker_with_font_size((10, 22), true, ImageProtocol::None).is_none());

        let sixel = build_picker_with_font_size((10, 22), false, ImageProtocol::Sixel).unwrap();
        assert_eq!(sixel.protocol_type, ProtocolType::Sixel);
        assert_eq!(sixel.font_size, (10, 22));

        let hb = build_picker_with_font_size((8, 16), true, ImageProtocol::Halfblocks).unwrap();
        assert_eq!(hb.protocol_type, ProtocolType::Halfblocks);
        assert_eq!(hb.font_size, (8, 16));

        #[cfg(target_os = "windows")]
        {
            let auto_modern = build_picker_with_font_size((10, 22), true, ImageProtocol::Auto).unwrap();
            assert_eq!(auto_modern.protocol_type, ProtocolType::Sixel);

            // Classic conhost.exe (`is_modern_terminal = false`) falls back to Web HD (`None`) instead of Halfblocks
            if std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false) {
                let auto_vscode = build_picker_with_font_size((8, 16), false, ImageProtocol::Auto).unwrap();
                assert_eq!(auto_vscode.protocol_type, ProtocolType::Sixel);
            } else {
                let auto_classic = build_picker_with_font_size((8, 16), false, ImageProtocol::Auto);
                if let Some(p) = auto_classic {
                    assert_ne!(p.protocol_type, ProtocolType::Halfblocks);
                }
            }
        }
    }

    #[test]
    fn windows_fallback_font_size_never_exceeds_sixel_cell_height() {
        let modern = safe_windows_fallback_font_size(true, false);
        assert_eq!(modern, (9, 18));
        assert!(modern.1 <= 20, "modern Windows fallback cell height must not exceed Windows Terminal's 20px Sixel height");

        assert_eq!(safe_windows_fallback_font_size(true, true), (8, 16));
        assert_eq!(safe_windows_fallback_font_size(false, false), (8, 16));
        assert_eq!(safe_windows_fallback_font_size(false, true), (8, 16));
    }
}
