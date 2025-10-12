use crate::{LiveState, media::AnyMedia, show::Show, util::NoDebug};

pub mod nyaa;

#[derive(Debug)]
pub struct QueryItem {
    pub source: SourceKind,
    /// user-facing name for this option
    pub name: Box<str>,
    pub file_size: Option<u64>,
    pub media: NoDebug<Box<dyn Future<Output = eyre::Result<AnyMedia>> + Send + 'static>>,
}

pub trait Source {
    /// Perform a search for the given show and optionally episode. Return
    fn query(
        &self,
        live: &mut LiveState,
        config: &crate::Config,
        show: &Show,
        filter_episode: Option<u32>,
    ) -> impl Future<Output = eyre::Result<Vec<QueryItem>>> + Send + 'static;
}
#[derive(Debug)]
pub enum SourceKind {
    Nyaa,
    File,
}
