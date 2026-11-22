use crossterm::event::KeyCode;

use crate::backend::manga_provider::inmanga::filter_state::InmangaFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct InmangaFiltersProvider {
    is_open: bool,
    filter: InmangaFilterState,
}

impl InmangaFiltersProvider {
    pub fn new(filter: InmangaFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for InmangaFiltersProvider {
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

impl FiltersHandler for InmangaFiltersProvider {
    type InnerState = InmangaFilterState;

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

#[derive(Debug, Clone)]
pub struct InmangaFilterWidget {}

impl InmangaFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for InmangaFilterWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl FiltersWidget for InmangaFilterWidget {
    type FilterState = InmangaFiltersProvider;
}

impl StatefulWidgetFrame for InmangaFilterWidget {
    type State = InmangaFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("InManga", area, frame.buffer_mut());
    }
}
