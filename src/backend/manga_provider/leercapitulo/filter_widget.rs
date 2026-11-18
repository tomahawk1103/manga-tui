use super::filter_state::LeercapituloFiltersProvider;
use crate::backend::manga_provider::FiltersWidget;
use crate::view::widgets::StatefulWidgetFrame;

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
