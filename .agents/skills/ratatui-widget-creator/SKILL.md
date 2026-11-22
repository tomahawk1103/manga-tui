---
name: ratatui-widget-creator
description: >-
  Use this skill when creating, modifying, or refactoring TUI components, pages,
  modals, or widgets in manga-tui. Guides through Ratatui layouts, Component and
  StatefulWidgetFrame traits, active ThemePalette styling (src/theme.rs), Spanish/English
  i18n (src/i18n.rs), scroll-aware mouse hitboxes, and Sixel/Zen reader rendering.
---

# Ratatui Widget & Page Creator Skill

This skill guides the construction, styling, localization, and event handling of user interface components in `manga-tui` using `ratatui`.

---

## Architecture Overview

In `manga-tui`, UI elements are organized under `src/view/`:
- `src/view/pages/`: Full-screen tabs/views (`home.rs`, `manga.rs`, `reader.rs`, `search.rs`, `feed.rs`). Registered in `src/view/pages.rs` (`SelectedPage`).
- `src/view/widgets/`: Reusable building blocks and modals (`home.rs`, `manga.rs`, `reader.rs`, `search.rs`, `feed.rs`, `cloud.rs`, `settings.rs`, `updater.rs`).
- `src/view/widgets.rs`: Defines core traits `Component`, `StatefulWidgetFrame`, and `ModalBuilder`.
- `src/theme.rs`: Dynamic theme system (`TokyoNight`, `Catppuccin`, `Classic`) and `ThemePalette`.
- `src/i18n.rs`: Bilateral Spanish/English localization helper `tr(es, en)` and `is_es()`.

---

## 1. Choosing the Right Abstraction

### A. Full Interactive Components (`pub trait Component`)
Use when the element manages its own event loop, async tasks, and state transitions:
```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use crate::view::widgets::Component;
use crate::backend::tui::Events;

pub struct MyComponent {
    // Local state, list states, flags
}

pub enum MyComponentAction {
    Select(usize),
    Close,
}

impl Component for MyComponent {
    type Actions = MyComponentAction;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        // Draw layouts, borders, and sub-widgets
    }

    fn handle_events(&mut self, events: Events) {
        // Handle KeyCode, MouseEvent, or Tick
    }

    fn update(&mut self, action: Self::Actions) {
        // Mutate state in response to actions
    }

    fn clean_up(&mut self) {
        // Abort background tokio tasks, clear caches
    }
}
```

### B. Stateful Renderers (`pub trait StatefulWidgetFrame`)
Use for modular UI elements where state is owned by a parent page (e.g., modals, filter panels, scroll lists):
```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use crate::view::widgets::StatefulWidgetFrame;

pub struct MyWidget;
pub struct MyWidgetState {
    pub selected_index: usize,
}

impl StatefulWidgetFrame for MyWidget {
    type State = MyWidgetState;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>, state: &mut Self::State) {
        // Render using passed state
    }
}
```

---

## 2. Theming & Localization (Mandatory)

### A. Always Use `src/theme.rs` Palette
Never hardcode raw colors when a semantic theme token exists. Fetch the active palette via `crate::theme::palette()` (or helper block/chip builders in `src/theme.rs`):
```rust
let p = crate::theme::palette();
let border_style = Style::default().fg(p.border_accent);
let selected_style = Style::default().bg(p.row_selected_bg).fg(p.text_primary);
let status_ok = Style::default().fg(p.status_success);
```

### B. Always Localize UI Copy with `src/i18n.rs`
Every user-visible string (titles, buttons, status messages, keybinding hints) must support Spanish and English via `crate::i18n::tr`:
```rust
use crate::i18n::tr;

let title = tr(" Descargas y Biblioteca ", " Downloads & Library ");
let hint = tr("Pulsa [Enter] para abrir", "Press [Enter] to open");
```

---

## 3. Mouse Hitboxes & Scroll Alignment

When handling `MouseEventKind::Down(MouseButton::Left)` on scrollable lists or cards (such as in `feed.rs`, `search.rs`, or `manga.rs`):
1. **Store the rendered content `Rect`** (inner area of the block, excluding borders) during `render()`.
2. **Account for scroll offset**: When mapping `mouse.row` to a card or list item index, add the current scroll offset (`list_state.offset()` or `scroll_offset`) and divide by the exact rendered card height (`CARD_HEIGHT`).
3. **Verify bounds**: Always check `content_area.contains(Position::new(mouse.column, mouse.row))` and ignore clicks when a modal popup is active above the page.

---

## 4. Reader & Sixel Image Rendering Guidelines (`src/view/pages/reader.rs`)

When modifying the manga reader or image rendering (`ratatui-image` / Sixel / Halfblocks):
- **Prevent Sixel Vertical Overflow Scroll**: Sixel escape sequences must never touch or exceed the last terminal row (`area.bottom()`), otherwise terminals emit a newline that scrolls the entire TUI buffer upward. Clamp image render `Rect` height strictly inside the available viewport (`height.saturating_sub(1)` when near the bottom boundary).
- **Zen Mode (HUD-less Reading)**: When `zen_mode` is toggled (`z` / `F11`), hide the top/bottom status bars and give the full clamped viewport to the page canvas while still respecting the Sixel bottom-row safety margin.

See [references/component_patterns.md](./references/component_patterns.md) for full examples.
