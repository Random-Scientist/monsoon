use crate::{LiveState, media::AnyMedia, show::Show};

pub mod nyaa;

pub trait Source {
    fn query(
        &self,
        live: &mut LiveState,
        show: &Show,
        filter_episode: Option<u32>,
    ) -> iced::Task<eyre::Result<Vec<AnyMedia>>>;
}
