use super::filter_state::InmangaFiltersProvider;
use crate::backend::manga_provider::FiltersWidget;
use crate::view::widgets::StatefulWidgetFrame;

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
