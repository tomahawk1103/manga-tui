use crossterm::event::KeyCode;

use crate::backend::manga_provider::leercapitulo::filter_state::LeercapituloFilterState;
use crate::backend::tui::Events;
use crate::view::provider_filters::{EventHandler, FiltersHandler, FiltersWidget};
use crate::view::widgets::StatefulWidgetFrame;

#[derive(Debug, Clone)]
pub struct LeercapituloFiltersProvider {
    is_open: bool,
    filter: LeercapituloFilterState,
}

impl LeercapituloFiltersProvider {
    pub fn new(filter: LeercapituloFilterState) -> Self {
        Self {
            is_open: false,
            filter,
        }
    }
}

impl EventHandler for LeercapituloFiltersProvider {
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

impl FiltersHandler for LeercapituloFiltersProvider {
    type InnerState = LeercapituloFilterState;

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
pub struct LeercapituloFilterWidget {}

impl LeercapituloFilterWidget {
    pub fn new() -> Self {
        Self {}
    }
}

impl FiltersWidget for LeercapituloFilterWidget {
    type FilterState = LeercapituloFiltersProvider;
}

impl StatefulWidgetFrame for LeercapituloFilterWidget {
    type State = LeercapituloFiltersProvider;

    fn render(&mut self, area: ratatui::prelude::Rect, frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {
        crate::utils::render_no_advanced_filters_card("LeerCapitulo", area, frame.buffer_mut());
    }
}
