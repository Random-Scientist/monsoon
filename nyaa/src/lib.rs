use std::{
    num::{NonZero, NonZeroUsize},
    str::FromStr,
    sync::OnceLock,
    time::Duration,
};

use futures::{FutureExt, future::join_all};
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use size::Size;
use strum::{AsRefStr, EnumDiscriminants, FromRepr, IntoDiscriminant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NyaaError {
    #[error("failed to request nyaa page")]
    GetError(#[from] reqwest::Error),
    #[error("nyaa HTML was not valid UTF-8")]
    InvalidBodyError(#[from] std::str::Utf8Error),
}
#[derive(Debug, Error)]
pub enum ParseMediaCategoryError {
    #[error("invalid category code {0}")]
    InvalidCategory(u8),
    #[error("invalid subcategory for {0:?}: {1}")]
    InvalidSubCategory(MediaCategory, u8),
    #[error("invalid media category string: {0}")]
    InvalidString(Box<str>),
}

pub struct NyaaClient {
    pub client: reqwest::Client,
    pub config: NyaaClientConfig,
}
pub struct NyaaClientConfig {
    /// URL of the nyaa instance to connect to, *including the protocol*
    pub instance_url: Box<str>,
    /// Request timeout
    pub timeout: Option<Duration>,
}
impl Default for NyaaClientConfig {
    fn default() -> Self {
        Self {
            instance_url: "https://nyaa.si".into(),
            timeout: None,
        }
    }
}

impl NyaaClient {
    pub fn new(config: NyaaClientConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
    pub async fn search(&self, q: &SearchQuery) -> Result<SearchResponse, NyaaError> {
        pub struct ScrapeSelectors([Selector; 11]);
        fn make_sels() -> ScrapeSelectors {
            fn selector(s: &str) -> Selector {
                Selector::parse(s).expect("statically specified selector to parse")
            }
            ScrapeSelectors([
                selector("table.torrent-list > tbody > tr"),
                selector("td:first-of-type > a"),
                selector("td:nth-of-type(2) > a:last-of-type"),
                selector("td:nth-of-type(3) > a:nth-of-type(2)"),
                selector("td:nth-of-type(3) > a:nth-of-type(1)"),
                selector("td:nth-of-type(4)"),
                selector("td:nth-of-type(5)"),
                selector("td:nth-of-type(6)"),
                selector("td:nth-of-type(7)"),
                selector("td:nth-of-type(8)"),
                selector(".pagination-page-info"),
            ])
        }
        // cache selectors
        static SELECTORS: OnceLock<ScrapeSelectors> = OnceLock::new();
        const MAX_PAGE_IDX: usize = 100;
        const MAX_ITEMS_PER_PAGE: usize = 75;
        let SearchQuery {
            query,
            category,
            filter,
            max_page_idx: max_pages,
            sort,
            user,
        } = q;
        let instance_url = Url::parse(&self.config.instance_url).expect("instance url to be valid");
        let (cat, subcat) = category.encode();
        let (by, order) = (sort.by.as_ref(), sort.order.as_ref());
        let user = user.as_deref().unwrap_or_default();
        let filter = *filter as u8;

        let body_for_page = |page: usize| {
            let mut u = instance_url.clone();
            u.set_query(Some(&format!(
                "q={query}&c={cat}_{subcat}&f={filter}&p={page}&s={by}&o={order}&u={user}",
            )));
            let mut builder = self.client.get(u);
            if let Some(dur) = self.config.timeout {
                builder = builder.timeout(dur);
            }
            builder.send().then(|v| async {
                match v {
                    Ok(resp) => Ok(resp.bytes().await?),
                    Err(e) => Err(e),
                }
            })
        };

        let ScrapeSelectors(
            [
                selector_item,
                selector_icon,
                selector_title,
                selector_magnet,
                selector_torrent,
                selector_size,
                selector_date,
                selector_seeders,
                selector_leechers,
                selector_downloads,
                selector_pagination,
            ],
        ) = SELECTORS.get_or_init(make_sels);

        let process_response = |bytes: &[u8], items: &mut Vec<Item>| -> Result<usize, NyaaError> {
            let html = Html::parse_document(str::from_utf8(bytes)?);
            let num_results: usize = html
                .select(selector_pagination)
                .next()
                .and_then(|v| {
                    v.inner_html()
                        .split(' ')
                        .nth(5)
                        .and_then(|v| str::parse(v).ok())
                })
                .unwrap_or(MAX_PAGE_IDX * MAX_ITEMS_PER_PAGE);
            items.extend(html.select(selector_item).filter_map(|el| {
                let select_attr =
                    |sel, attr| el.select(sel).next().and_then(|v| v.value().attr(attr));

                let select_inner = |sel| el.select(sel).next().as_ref().map(ElementRef::inner_html);

                let size = select_inner(selector_size)?;
                let mut torrent_file_link = String::from(&*self.config.instance_url);
                torrent_file_link.push_str(select_attr(selector_torrent, "href")?);

                Some(Item {
                    category: select_attr(selector_icon, "href")?
                        .split('=')
                        .next_back()?
                        .parse()
                        .ok()?,
                    torrent_file_link,
                    size: size
                        .parse()
                        .map(|v: Size| v.bytes().abs_diff(0))
                        .map_err(move |_| size),
                    title: select_attr(selector_title, "title")?.into(),
                    magnet_link: select_attr(selector_magnet, "href")?.into(),
                    date: select_inner(selector_date)?,
                    seeders: select_inner(selector_seeders)?.parse().unwrap_or_default(),
                    leechers: select_inner(selector_leechers)?.parse().unwrap_or_default(),
                    downloads: select_inner(selector_downloads)?
                        .parse()
                        .unwrap_or_default(),
                })
            }));
            Ok(num_results)
        };

        let mut items = Vec::new();
        let first_response = body_for_page(0).await?;
        let num_results = process_response(&first_response, &mut items)?;
        // max page count
        let server_last_page = num_results.div_ceil(MAX_ITEMS_PER_PAGE);
        // limit pages requested in unbounded case to actual max
        let last_read_page =
            server_last_page.min(max_pages.map(NonZero::get).unwrap_or(usize::MAX));

        join_all((1..last_read_page).map(body_for_page))
            .await
            .into_iter()
            .try_for_each(|v| process_response(&v?, &mut items).map(|_| ()))?;
        Ok(SearchResponse {
            results: items,
            last_page: server_last_page,
            last_read_page,
        })
    }
}
#[derive(Debug, Clone)]
pub struct Item {
    pub seeders: u32,
    pub leechers: u32,
    pub downloads: u32,
    pub category: MediaCategory,
    pub magnet_link: Box<str>,
    pub torrent_file_link: String,
    pub title: Box<str>,
    pub size: Result<u64, String>,
    pub date: String,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Search query
    pub query: String,
    /// Category to restrict search to
    pub category: MediaCategory,
    /// filter response
    pub filter: Filter,
    /// Loop and fetch up to this many pages. Will attempt to load all pages if `None`
    pub max_page_idx: Option<NonZeroUsize>,
    /// How to sort returned results
    pub sort: Sort,
    pub user: Option<String>,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Sort {
    pub by: SortBy,
    pub order: SortOrder,
}

pub struct SearchResponse {
    pub results: Vec<Item>,
    // index of the last page of results this query has
    pub last_page: usize,
    // index of the last page retrieved in this query
    pub last_read_page: usize,
}
#[derive(Debug, Default, Clone, Copy, AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum SortBy {
    #[default]
    #[doc(alias = "Date")]
    Id,
    Downloads,
    Seeders,
    Leechers,
    Size,
}
#[derive(Debug, Default, Clone, Copy, AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Desc,
    Asc,
}
#[derive(Debug, Default, Clone, Copy)]
pub enum Filter {
    #[default]
    NoFilter,
    NoRemakes,
    TrustedOnly,
    Batches,
}
#[derive(Debug, Default, Clone, Copy, FromRepr, EnumDiscriminants)]
#[repr(u8)]
/// Represents a specific kind of media in Nyaa's model. In variants containing an optional, None implies that all items of that category are being referred to
pub enum MediaCategory {
    #[default]
    All,
    Anime(Option<AnimeKind>),
    Audio(Option<AudioKind>),
    Literature(Option<LiteratureKind>),
    LiveAction(Option<LiveActionKind>),
    Picture(Option<PictureKind>),
    Software(Option<SoftwareKind>),
}
impl MediaCategory {
    pub fn encode(self) -> (u8, u8) {
        (
            self.discriminant() as u8,
            match self {
                // horrid code duplication. std, please impl From<Enum> for Repr or at least Into<Repr> for Enum
                MediaCategory::All => None,
                MediaCategory::Anime(anime_kind) => anime_kind.map(|v| v as u8),
                MediaCategory::Audio(audio_kind) => audio_kind.map(|v| v as u8),
                MediaCategory::Literature(literature_kind) => literature_kind.map(|v| v as u8),
                MediaCategory::LiveAction(live_action_kind) => live_action_kind.map(|v| v as u8),
                MediaCategory::Picture(picture_kind) => picture_kind.map(|v| v as u8),
                MediaCategory::Software(software_kind) => software_kind.map(|v| v as u8),
            }
            .unwrap_or(0u8),
        )
    }
    pub fn decode(pair: [u8; 2]) -> Result<Self, ParseMediaCategoryError> {
        use ParseMediaCategoryError as P;
        let mut cat = MediaCategory::from_repr(pair[0]).ok_or(P::InvalidCategory(pair[0]))?;
        let cat_only = cat;
        // more absymal code dupe
        match &mut cat {
            MediaCategory::All => {}
            MediaCategory::Anime(anime_kind) => {
                *anime_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        AnimeKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
            MediaCategory::Audio(audio_kind) => {
                *audio_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        AudioKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
            MediaCategory::Literature(literature_kind) => {
                *literature_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        LiteratureKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
            MediaCategory::LiveAction(live_action_kind) => {
                *live_action_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        LiveActionKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
            MediaCategory::Picture(picture_kind) => {
                *picture_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        PictureKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
            MediaCategory::Software(software_kind) => {
                *software_kind = if pair[1] == 0 {
                    None
                } else {
                    Some(
                        SoftwareKind::from_repr(pair[1])
                            .ok_or(P::InvalidSubCategory(cat_only, pair[1]))?,
                    )
                };
            }
        }
        Ok(cat)
    }
}
impl FromStr for MediaCategory {
    type Err = ParseMediaCategoryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut it = s.split('_');
        let mut next = || {
            it.next()
                .ok_or_else(|| ParseMediaCategoryError::InvalidString(s.into()))
                .and_then(|v| {
                    v.parse()
                        .map_err(|_| ParseMediaCategoryError::InvalidString(s.into()))
                })
        };
        MediaCategory::decode([next()?, next()?])
    }
}

#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum AnimeKind {
    MusicVideo = 1,
    SubEnglish,
    SubNonEnglish,
    Raw,
}
#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum AudioKind {
    Lossless = 1,
    Lossy,
}

#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum LiteratureKind {
    TranslatedEnglish = 1,
    TranslatedNonEnglish,
    Raw,
}
#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum LiveActionKind {
    TranslatedEnglish = 1,
    TranslatedNonEnglish,
    IdolPromoVideo,
}
#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum PictureKind {
    Graphic = 1,
    Photo,
}
#[derive(Debug, Clone, Copy, FromRepr)]
#[repr(u8)]
pub enum SoftwareKind {
    Application = 1,
    Game,
}

#[cfg(test)]
mod test {
    use std::num::NonZeroUsize;

    use crate::NyaaClient;

    #[tokio::test]
    async fn test() {
        let client = NyaaClient::new(Default::default());
        let resp = client
            .search(&crate::SearchQuery {
                query: "".into(),
                max_page_idx: NonZeroUsize::new(1),
                ..Default::default()
            })
            .await
            .unwrap();
        dbg!(&resp.results);
    }
}
