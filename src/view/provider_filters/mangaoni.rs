use crossterm::event::KeyCode;

use crate::backend::manga_provider::mangaoni::filter_state::MangaoniFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct MangaoniFiltersProvider {
    is_open: bool,
    filter: MangaoniFilterState,
}

impl MangaoniFiltersProvider {
    pub fn new(filter: MangaoniFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for MangaoniFiltersProvider {
    fn handle_events(&mut self, events: Events) {
        #![allow(clippy::single_match)]
        match events {
            Events::Key(key) => match key.code {
                KeyCode::Char('f') => self.toggle(),
                KeyCode::Esc if self.is_open => self.toggle(),
                _ => {},
            },
            _ => {},
        }
    }
}

impl FiltersHandler for MangaoniFiltersProvider {
    type InnerState = MangaoniFilterState;

    fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn is_typing(&self) -> bool {
        false
    }

    fn get_state(&self) -> &Self::InnerState {
        &self.filter
    }
}

#[derive(Debug, Clone, Default)]
pub struct MangaoniFilterWidget {}

impl MangaoniFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl FiltersWidget for MangaoniFilterWidget {
    type FilterState = MangaoniFiltersProvider;
}

impl StatefulWidgetFrame for MangaoniFilterWidget {
    type State = MangaoniFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("MangaOni", area, frame.buffer_mut());
    }
}
