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
}

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

#[cfg(unix)]
pub fn get_picker() -> Option<Picker> {
    let env_proto = std::env::var("MANGA_TUI_PROTOCOL").ok();
    let config_proto = crate::view::widgets::settings::active_image_protocol();
    match resolve_protocol_override(env_proto.as_deref(), config_proto) {
        Some(Some(proto)) => {
            let mut picker = Picker::from_termios().unwrap_or_else(|_| Picker::new((8, 16)));
            picker.protocol_type = proto;
            return Some(picker);
        },
        Some(None) => return None,
        None => {},
    }

    Picker::from_termios()
        .ok()
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

    if is_modern_windows_terminal_env(wt_session.as_deref(), term_program.as_deref(), wezterm_exe.as_deref()) {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        is_conpty_or_pseudo_console()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
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
/// Safe default cell width in pixels when CSI 16t query is unavailable (never over-estimates height).
const DEFAULT_FONT_SIZE_WIDTH: u16 = 8;
#[cfg(target_os = "windows")]
/// Safe default cell height in pixels when CSI 16t query is unavailable (never over-estimates height).
const DEFAULT_FONT_SIZE_HEIGHT: u16 = 16;

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

    let mut stdout = std::io::stdout();
    stdout.write_all(b"\x1b[16t").ok()?;
    stdout.flush().ok()?;

    let mut found_escape = false;
    // Drain unrelated startup events (FOCUS_EVENT, WINDOW_BUFFER_SIZE_EVENT, key-up from Enter)
    // until the CSI 16t ESC prefix arrives or the short polling budget expires.
    for _ in 0..25 {
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
        if peek_record.EventType as u32 != KEY_EVENT as u32 {
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
    for _ in 0..32 {
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
        if record.EventType as u32 == KEY_EVENT as u32 && unsafe { record.Event.KeyEvent.bKeyDown } != 0 {
            let code_unit = unsafe { record.Event.KeyEvent.uChar.UnicodeChar };
            if let Some(ch) = char::from_u32(code_unit as u32)
                && ch != '\0'
            {
                response.push(ch);
                if ch == 't' {
                    break;
                }
            }
        }
    }

    parse_csi_cell_size_response(&response)
}

#[cfg(target_os = "windows")]
pub fn get_picker() -> Option<Picker> {
    const FALLBACK_CELL_WIDTH: u16 = 10;
    const FALLBACK_CELL_HEIGHT: u16 = 22;

    let is_modern_terminal = is_running_in_modern_windows_terminal();
    let size: (u16, u16) = if is_modern_terminal {
        query_windows_terminal_cell_size().unwrap_or_else(|| {
            use windows_sys::Win32::System::Console::GetConsoleWindow;
            use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};

            let window_dpi = unsafe { GetDpiForWindow(GetConsoleWindow()) };
            let dpi = if window_dpi == 0 {
                let sys_dpi = unsafe { GetDpiForSystem() };
                if sys_dpi == 0 { 96 } else { sys_dpi }
            } else {
                window_dpi
            };
            let scale = dpi as f32 / 96.0;
            (
                ((FALLBACK_CELL_WIDTH as f32 * scale).round() as u16).max(1),
                ((FALLBACK_CELL_HEIGHT as f32 * scale).round() as u16).max(1),
            )
        })
    } else {
        (DEFAULT_FONT_SIZE_WIDTH, DEFAULT_FONT_SIZE_HEIGHT)
    };

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

    let protocol = picker.guess_protocol();

    let is_vscode_conpty = std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false);

    if is_vscode_conpty || (protocol == ProtocolType::Halfblocks && is_modern_terminal) {
        picker.protocol_type = ProtocolType::Sixel;
    }

    Some(picker)
}

///Start app's main loop
pub async fn run_app<T: MangaProvider>(
    mut terminal: Terminal<impl Backend>,
    api_client: T,
    manga_tracker: Option<impl MangaTracker>,
    filter_state: T::FiltersHandler,
    filter_widget: T::Widget,
) -> Result<(), Box<dyn Error>> {
    let mut app = App::new(api_client, manga_tracker, get_picker(), filter_state, filter_widget);
    app.spawn_startup_provider_check();
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

    Ok(())
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
}
