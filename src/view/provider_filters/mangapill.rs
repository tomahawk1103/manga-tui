use crossterm::event::KeyCode;

use crate::backend::manga_provider::mangapill::filter_state::MangaPillFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct MangaPillFiltersProvider {
    is_open: bool,
    filter: MangaPillFilterState,
}

impl MangaPillFiltersProvider {
    pub fn new(filter: MangaPillFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for MangaPillFiltersProvider {
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

impl FiltersHandler for MangaPillFiltersProvider {
    type InnerState = MangaPillFilterState;

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

/// TODO: implement MangaPill filters in future release
#[derive(Debug, Clone)]
pub struct MangaPillFilterWidget {}

impl MangaPillFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl FiltersWidget for MangaPillFilterWidget {
    type FilterState = MangaPillFiltersProvider;
}

impl StatefulWidgetFrame for MangaPillFilterWidget {
    type State = MangaPillFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("MangaPill", area, frame.buffer_mut());
    }
}
