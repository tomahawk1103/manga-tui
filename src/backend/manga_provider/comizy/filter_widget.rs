use super::filter_state::ComizyFiltersProvider;
use crate::backend::manga_provider::FiltersWidget;
use crate::view::widgets::StatefulWidgetFrame;

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
