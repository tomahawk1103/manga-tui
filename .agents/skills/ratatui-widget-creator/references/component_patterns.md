# Ratatui Component Patterns in manga-tui

### 1. Theme-Aware & Localized Block with Scrollable List

Always use `crate::theme::palette()` and `crate::i18n::tr(es, en)`:

```rust
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::style::{Modifier, Style};
use ratatui::Frame;
use ratatui::layout::Rect;
use crate::i18n::tr;
use crate::theme::palette;

pub struct ScrollableListWidget<'a> {
    items: Vec<String>,
    title_es: &'a str,
    title_en: &'a str,
}

pub struct ScrollableListState {
    pub list_state: ListState,
    pub inner_area: Rect,
}

impl ScrollableListState {
    pub fn new(count: usize) -> Self {
        let mut state = ListState::default();
        if count > 0 {
            state.select(Some(0));
        }
        Self {
            list_state: state,
            inner_area: Rect::default(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, items: &[String], title: &str) {
        let p = palette();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(p.border_accent))
            .title(title);

        self.inner_area = block.inner(area);

        let list_items: Vec<ListItem> = items
            .iter()
            .map(|text| ListItem::new(text.as_str()).style(Style::default().fg(p.text_primary)))
            .collect();

        let list = List::new(list_items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(p.row_selected_bg)
                    .fg(p.accent_primary)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}
```

---

### 2. Scroll-Aware Mouse Hitbox Calculation

When handling mouse clicks on a vertical list of cards where each card occupies `CARD_HEIGHT` rows:

```rust
use ratatui::layout::{Position, Rect};

pub fn card_index_at_mouse(
    inner_area: Rect,
    mouse_col: u16,
    mouse_row: u16,
    scroll_offset_items: usize,
    card_height: u16,
    total_items: usize,
) -> Option<usize> {
    if !inner_area.contains(Position::new(mouse_col, mouse_row)) || card_height == 0 {
        return None;
    }
    let relative_row = mouse_row.saturating_sub(inner_area.y);
    let visible_slot = (relative_row / card_height) as usize;
    let index = scroll_offset_items + visible_slot;
    (index < total_items).then_some(index)
}
```

---

### 3. Centered Confirmation Popup

```rust
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::style::Style;
use ratatui::Frame;
use ratatui::layout::Rect;
use crate::utils::centered_rect;
use crate::theme::palette;
use crate::i18n::tr;

pub fn render_confirmation_popup(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let p = palette();
    let popup_area = centered_rect(50, 25, area);
    // Clear area behind modal to avoid bleed-through
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(p.border_accent))
        .title(tr(" Confirmar acción ", " Confirm Action "));
    let text = Paragraph::new(message)
        .style(Style::default().fg(p.text_primary))
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(text, popup_area);
}
```

---

### 4. Asynchronous Task Lifecycle with Clean-up

Always store task handles so they can be safely aborted when switching tabs:

```rust
pub struct PageComponent {
    search_task: Option<tokio::task::JoinHandle<()>>,
}

impl Component for PageComponent {
    // ...
    fn clean_up(&mut self) {
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
    }
}
```
