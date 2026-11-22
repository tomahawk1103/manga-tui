use crate::backend::manga_provider::SearchPageProvider;
use crate::backend::manga_provider::comizy::ComizyProvider;
use crate::backend::manga_provider::inmanga::InmangaProvider;
use crate::backend::manga_provider::leercapitulo::LeercapituloProvider;
use crate::backend::manga_provider::mangadex::MangadexClient;
use crate::backend::manga_provider::mangaoni::MangaoniProvider;
use crate::backend::manga_provider::mangapill::MangaPillProvider;
use crate::backend::manga_provider::weebcentral::WeebcentralProvider;
use crate::backend::tui::Events;
use crate::view::widgets::StatefulWidgetFrame;

pub mod comizy;
pub mod inmanga;
pub mod leercapitulo;
pub mod mangadex;
pub mod mangaoni;
pub mod mangapill;
pub mod weebcentral;

pub trait EventHandler {
    fn handle_events(&mut self, events: Events);
}

pub trait FiltersHandler: EventHandler {
    type InnerState: Send + Clone;

    fn toggle(&mut self);
    fn is_open(&self) -> bool;
    fn is_typing(&self) -> bool;
    fn get_state(&self) -> &Self::InnerState;
}

/// This trait represent the widget used to show the user the filters available for the
/// `manga_provider`, why not just use the `StatefulWidgetFrame`?, maybe in the future the widget
/// may require more methods/functionality
pub trait FiltersWidget: StatefulWidgetFrame<State = Self::FilterState> {
    type FilterState;
}

pub trait TuiSearchProvider: SearchPageProvider {
    type FiltersHandler: FiltersHandler<InnerState = <Self as SearchPageProvider>::InnerState>;
    type Widget: FiltersWidget<FilterState = Self::FiltersHandler>;
}

impl TuiSearchProvider for MangadexClient {
    type FiltersHandler = mangadex::filter_provider::MangadexFilterProvider;
    type Widget = mangadex::filter_widget::MangadexFilterWidget;
}

impl TuiSearchProvider for WeebcentralProvider {
    type FiltersHandler = weebcentral::WeebcentralFiltersProvider;
    type Widget = weebcentral::WeebcentralFilterWidget;
}

impl TuiSearchProvider for MangaPillProvider {
    type FiltersHandler = mangapill::MangaPillFiltersProvider;
    type Widget = mangapill::MangaPillFilterWidget;
}

impl TuiSearchProvider for InmangaProvider {
    type FiltersHandler = inmanga::InmangaFiltersProvider;
    type Widget = inmanga::InmangaFilterWidget;
}

impl TuiSearchProvider for MangaoniProvider {
    type FiltersHandler = mangaoni::MangaoniFiltersProvider;
    type Widget = mangaoni::MangaoniFilterWidget;
}

impl TuiSearchProvider for LeercapituloProvider {
    type FiltersHandler = leercapitulo::LeercapituloFiltersProvider;
    type Widget = leercapitulo::LeercapituloFilterWidget;
}

impl TuiSearchProvider for ComizyProvider {
    type FiltersHandler = comizy::ComizyFiltersProvider;
    type Widget = comizy::ComizyFilterWidget;
}

#[cfg(test)]
pub mod mock {
    use super::*;
    pub use crate::backend::manga_provider::mock::{MockFilterState, MockMangaPageProvider};

    #[derive(Debug, Clone)]
    pub struct MockFiltersHandler {
        is_open: bool,
        pub state: MockFilterState,
    }

    impl MockFiltersHandler {
        pub fn new(state: MockFilterState) -> Self {
            Self {
                is_open: false,
                state,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct MockWidgetFilter {}

    impl EventHandler for MockFiltersHandler {
        fn handle_events(&mut self, _events: Events) {}
    }

    impl FiltersHandler for MockFiltersHandler {
        type InnerState = MockFilterState;

        fn toggle(&mut self) {
            self.is_open = !self.is_open;
        }

        fn is_open(&self) -> bool {
            self.is_open
        }

        fn is_typing(&self) -> bool {
            self.is_open
        }

        fn get_state(&self) -> &Self::InnerState {
            &self.state
        }
    }

    impl StatefulWidgetFrame for MockWidgetFilter {
        type State = MockFiltersHandler;

        fn render(&mut self, _area: ratatui::prelude::Rect, _frame: &mut ratatui::Frame<'_>, _state: &mut Self::State) {}
    }

    impl FiltersWidget for MockWidgetFilter {
        type FilterState = MockFiltersHandler;
    }

    impl TuiSearchProvider for MockMangaPageProvider {
        type FiltersHandler = MockFiltersHandler;
        type Widget = MockWidgetFilter;
    }
}
