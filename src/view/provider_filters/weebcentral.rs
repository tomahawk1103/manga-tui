use crossterm::event::KeyCode;

use crate::backend::manga_provider::weebcentral::filter_state::WeebcentralFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct WeebcentralFiltersProvider {
    is_open: bool,
    filter: WeebcentralFilterState,
}

impl WeebcentralFiltersProvider {
    pub fn new(filter: WeebcentralFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for WeebcentralFiltersProvider {
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

impl FiltersHandler for WeebcentralFiltersProvider {
    type InnerState = WeebcentralFilterState;

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

/// TODO: implement Weebcentral filters in future release
#[derive(Debug, Clone)]
pub struct WeebcentralFilterWidget {}

impl WeebcentralFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl FiltersWidget for WeebcentralFilterWidget {
    type FilterState = WeebcentralFiltersProvider;
}

impl StatefulWidgetFrame for WeebcentralFilterWidget {
    type State = WeebcentralFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("WeebCentral", area, frame.buffer_mut());
    }
}
