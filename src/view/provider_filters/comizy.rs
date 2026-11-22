use crossterm::event::KeyCode;

use crate::backend::manga_provider::comizy::filter_state::ComizyFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct ComizyFiltersProvider {
    is_open: bool,
    filter: ComizyFilterState,
}

impl ComizyFiltersProvider {
    pub fn new(filter: ComizyFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for ComizyFiltersProvider {
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

impl FiltersHandler for ComizyFiltersProvider {
    type InnerState = ComizyFilterState;

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
pub struct ComizyFilterWidget {}

impl ComizyFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl FiltersWidget for ComizyFilterWidget {
    type FilterState = ComizyFiltersProvider;
}

impl StatefulWidgetFrame for ComizyFilterWidget {
    type State = ComizyFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("Comizy", area, frame.buffer_mut());
    }
}
