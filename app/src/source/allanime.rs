use crate::{
    LiveState,
    show::Show,
    source::{QueryItem, Source},
};

pub struct AllAnime;
impl Source for AllAnime {
    fn query(
        &self,
        live: &mut LiveState,
        config: &crate::Config,
        show: &Show,
        filter_episode: Option<u32>,
    ) -> impl Future<Output = eyre::Result<Vec<QueryItem>>> + Send + 'static {
        let n = show.get_preferred_name(config).to_string().into_boxed_str();
        async move { todo!() }
    }
}
