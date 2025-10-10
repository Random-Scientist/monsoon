use crate::source::Source;

pub struct Nyaa;
impl Source for Nyaa {
    fn query(
        &self,
        live: &mut crate::LiveState,
        show: &crate::show::Show,
        filter_episode: Option<u32>,
    ) -> iced::Task<eyre::Result<Vec<crate::media::AnyMedia>>> {
        live.nyaa.search(q)
    }
}
