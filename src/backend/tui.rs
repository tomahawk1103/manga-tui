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

#[cfg(unix)]
fn get_picker() -> Option<Picker> {
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

/// Checks current process environment variables to detect whether the app is running inside a
/// modern Windows terminal emulator.
pub fn is_running_in_modern_windows_terminal() -> bool {
    let wt_session = std::env::var("WT_SESSION").ok();
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let wezterm_exe = std::env::var("WEZTERM_EXECUTABLE").ok();

    is_modern_windows_terminal_env(wt_session.as_deref(), term_program.as_deref(), wezterm_exe.as_deref())
}

#[cfg(target_os = "windows")]
/// Default font width for Windows console on 96 DPI
const DEFAULT_FONT_SIZE_WIDTH: u16 = 10;
#[cfg(target_os = "windows")]
/// Default font height for Windows console on 96 DPI
const DEFAULT_FONT_SIZE_HEIGHT: u16 = 22;

#[cfg(target_os = "windows")]
fn get_picker() -> Option<Picker> {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};

    let window_dpi = unsafe { GetDpiForWindow(GetConsoleWindow()) };
    // In some terminals (ConPTY, Windows Terminal) there is no classic console window and GetDpiForWindow returns 0;
    // fall back to system DPI so HiDPI displays get sharp cell pixel dimensions.
    let dpi = if window_dpi == 0 {
        let sys_dpi = unsafe { GetDpiForSystem() };
        if sys_dpi == 0 { 96 } else { sys_dpi }
    } else {
        window_dpi
    };
    let scale = dpi as f32 / 96.0;
    let size: (u16, u16) = (
        ((DEFAULT_FONT_SIZE_WIDTH as f32 * scale).round() as u16).max(1),
        ((DEFAULT_FONT_SIZE_HEIGHT as f32 * scale).round() as u16).max(1),
    );

    let mut picker = Picker::new((size.0, size.1));

    if let Ok(override_proto) = std::env::var("MANGA_TUI_PROTOCOL") {
        match override_proto.trim().to_ascii_lowercase().as_str() {
            "sixel" => picker.protocol_type = ProtocolType::Sixel,
            "iterm2" | "iterm" => picker.protocol_type = ProtocolType::Iterm2,
            "kitty" => picker.protocol_type = ProtocolType::Kitty,
            "halfblocks" => picker.protocol_type = ProtocolType::Halfblocks,
            "none" | "off" => return None,
            _ => {
                picker.guess_protocol();
            },
        }
        return Some(picker);
    }

    let protocol = picker.guess_protocol();

    let is_vscode_conpty = std::env::var("TERM_PROGRAM").map(|v| v.eq_ignore_ascii_case("vscode")).unwrap_or(false);
    let is_modern_terminal = is_running_in_modern_windows_terminal();

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
        if was_overlay_open != is_overlay_open || app.current_tab != last_tab {
            terminal.clear()?;
        }
        was_overlay_open = is_overlay_open;
        last_tab = app.current_tab;

        terminal.draw(|f| {
            app.render(f.area(), f);
        })?;

        app.listen_to_event().await;

        app.update_based_on_action();
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
}
