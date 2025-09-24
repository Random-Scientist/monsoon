use std::{
    collections::HashMap,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anilist_moe::models::Anime;
use bincode::{Decode, Encode};
use directories::ProjectDirs;
use eyre::{Context, OptionExt};
use iced::{
    Element, Subscription, Task, time,
    widget::{self, button, image, row},
    window,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::{
    db::MainDb,
    show::{Show, ShowId},
};

pub mod anilist;
pub mod db;
pub mod list;
pub mod show;

#[derive(
    Debug, Default, Clone, Encode, Decode, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
enum NameKind {
    #[default]
    English,
    Romaji,
    Synonym,
    Native,
}
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    preferred_name_kind: NameKind,
    anilist: anilist::Config,
    media_directory: MediaDir,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaDir(PathBuf);
impl Default for MediaDir {
    fn default() -> Self {
        Self(
            directories::BaseDirs::new()
                .expect("failed to get base directories")
                .home_dir()
                .into(),
        )
    }
}
impl Config {
    fn load(file: impl AsRef<Path>) -> Self {
        let p = file.as_ref();
        if !p.exists() {
            File::create(p).expect("config file to be created");
        }
        let text = fs::read_to_string(p).expect("config file to be readable");
        toml::from_str::<Config>(&text).expect("config to be deserialized correctly")
    }
}

pub struct Monsoon {
    main_window_id: window::Id,
    dirs: ProjectDirs,
    thumbnails: HashMap<ShowId, image::Handle>,
    db: MainDb,
    config: Config,
    live: LiveState,
}

pub struct LiveState {
    ani_client: Arc<anilist_moe::AniListClient>,
    current_add_query: Option<AddQuery>,
}
#[derive(Default)]
pub struct AddQuery {
    query: String,
    candidates_dirty: bool,
    candidates: Vec<Anime>,
}

impl LiveState {
    fn new(conf: &Config) -> Self {
        Self {
            // todo auth
            ani_client: Arc::new(anilist_moe::AniListClient::new()),
            current_add_query: None,
        }
    }
}
#[allow(private_interfaces)]
impl Monsoon {
    pub fn new() -> (Self, Task<Message>) {
        simple_logger::SimpleLogger::new()
            .env()
            .with_level(log::LevelFilter::Error)
            .with_module_level("ui", log::LevelFilter::Trace)
            .init()
            .expect("no logger to be set");
        let dirs =
            directories::ProjectDirs::from("rs", "rsci", "monsoon").expect("directories to load");
        let db = MainDb::open(dirs.config_dir().join("db"));
        let config = Config::load(dirs.config_dir().join("config.toml"));
        let (main_window_id, task) = window::open(window::Settings::default());
        let live = LiveState::new(&config);

        (
            Self {
                dirs,
                db,
                config,
                main_window_id,
                live,
                thumbnails: HashMap::new(),
            },
            task.map(|_| Message::MainWindowOpened),
        )
    }
    fn draw_top_bar(&'_ self) -> Element<'_, Message> {
        let text = widget::text_input(
            "anime name or anilist ID",
            self.live
                .current_add_query
                .as_ref()
                .map(|v| &*v.query)
                .unwrap_or(""),
        )
        .on_input(|s| Message::AddAnime(AddAnime::ModifyQuery(s)))
        .on_submit(Message::AddAnime(AddAnime::Submit));
        row![
            button("+").on_press(Message::AddAnime(AddAnime::Submit)),
            text
        ]
        .into()
    }
    pub fn view(&'_ self, window: window::Id) -> Element<'_, Message> {
        if window == self.main_window_id {
            let content = if let Some(current) = &self.live.current_add_query {
                widget::column![]
                    .extend(current.candidates.iter().map(|v| {
                        widget::button(widget::text({
                            if let Some(titles) = v.title.as_ref() {
                                let candidates = [
                                    titles.english.as_ref(),
                                    titles.romaji.as_ref(),
                                    titles.native.as_ref(),
                                    titles.user_preferred.as_ref(),
                                ];
                                let preferred = match self.config.preferred_name_kind {
                                    NameKind::English => candidates[0],
                                    NameKind::Romaji => candidates[1],
                                    NameKind::Synonym => None,
                                    NameKind::Native => candidates[2],
                                };
                                let name: &str = preferred.map_or(
                                    candidates.iter().find_map(|v| *v).map_or("", |v| v),
                                    |v| v,
                                );
                                name
                            } else {
                                "[no name found]"
                            }
                        }))
                        .on_press(Message::AddAnime(AddAnime::RequestCreateAnilist(v.id)))
                        .erase_element()
                    }))
                    .into()
            } else {
                self.view_list()
            };
            widget::column![self.draw_top_bar(), widget::Rule::horizontal(5), content].into()
        } else {
            unimplemented!()
        }
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let mut tasks = TaskList::new();
        macro_rules! unwrap {
            ($val:expr) => {
                match $val {
                    Ok(v) => v,
                    Err(e) => {
                        tasks.push(Message::Error(Arc::new(e.into())));
                        return tasks.batch();
                    }
                }
            };
        }
        match message {
            Message::WindowClosed(id) => {
                if id == self.main_window_id {
                    self.db.shows.flush_all();
                    let e: Task<Message> = iced::exit();
                    tasks.push(e);
                }
            }
            Message::LoadThumbnail(id) => {
                let show = unwrap!(
                    self.db
                        .shows
                        .get(id)
                        .ok_or_eyre("tried to load the thumnail for a show not in DB")
                );
                let mut should_clear = false;
                if let Some(path) = show.thumbnail.as_ref() {
                    match path {
                        show::ThumbnailPath::File(path_buf) => {
                            if path_buf.exists() {
                                let p = path_buf.clone();
                                tasks.push(
                                    async move {
                                        Message::ThumbnailLoaded(id, image::Handle::from_path(p))
                                    }
                                    .into_task(),
                                );
                            } else {
                                should_clear = true;
                            }
                        }
                        show::ThumbnailPath::Url(path) => {
                            let p = path.clone();
                            tasks.push(Task::future(async move {
                                match reqwest::get(p)
                                    .await
                                    .map_err(reqwest::Error::without_url)
                                    .wrap_err("failed to request thumbnail from url")
                                {
                                    Ok(resp) => resp
                                        .bytes()
                                        .await
                                        .wrap_err("thumbnail response bytes not present")
                                        .map(|v| {
                                            Message::ThumbnailLoaded(
                                                id,
                                                image::Handle::from_bytes(v),
                                            )
                                        })
                                        .into(),
                                    Err(err) => Message::Error(err.into()),
                                }
                            }));
                        }
                    }
                }
                if should_clear {
                    // remove invalid path thumbnail
                    self.db.shows.update_cached(id, |v| v.thumbnail = None);
                    self.db.shows.flush(id);
                }
            }
            Message::MainWindowOpened => {
                tasks.extend_from(
                    self.db
                        .shows
                        .enumerate()
                        .map(|(id, _)| Message::LoadThumbnail(id)),
                );
            }
            Message::AddAnime(a) => {
                match a {
                    AddAnime::ModifyQuery(v) => {
                        if !v.is_empty() {
                            let q = self.live.current_add_query.get_or_insert_default();
                            if q.query != v {
                                q.query = v;
                                q.candidates_dirty = true;
                            }
                        } else {
                            // exit add mode if the search bar is empty
                            self.live.current_add_query = None;
                        }
                    }
                    AddAnime::Submit => {
                        // try to parse an anilist id first
                        if let Some(id) = self.live.current_add_query.take().and_then(|q| {
                            q.query.parse().ok().or(q.candidates.first().map(|v| v.id))
                        }) {
                            tasks.push(Message::AddAnime(AddAnime::RequestCreateAnilist(id)));
                        }
                    }
                    AddAnime::CreateFromAnilist(anime) => {
                        let mut s = Show::default();
                        s.update_with(&anime);
                        let id = self.db.shows.insert(s);
                        tasks.push(Message::LoadThumbnail(id))
                    }
                    AddAnime::RequestCreateAnilist(v) => 'add: {
                        //todo don't exit add mode if shift held or something
                        self.live.current_add_query = None;
                        // make sure nothing else has this id
                        if self
                            .db
                            .shows
                            .enumerate()
                            .any(|s| s.1.anilist_id.is_some_and(|id| id == v))
                        {
                            break 'add;
                        }
                        let client = self.make_ani_client();
                        tasks.push(
                            async move {
                                client
                                    .anime()
                                    .get_by_id(v)
                                    .await
                                    .wrap_err("getting anime details by ID")
                                    .map(|v| {
                                        Message::AddAnime(AddAnime::CreateFromAnilist(v.into()))
                                    })
                            }
                            .into_task(),
                        );
                    }

                    AddAnime::RefreshCandidates => {
                        if let Some(query) = self.live.current_add_query.as_ref() {
                            let client = self.make_ani_client();
                            let q = query.query.clone();
                            let q2 = q.clone();
                            tasks.push(
                                async move {
                                    client.anime().search(&q, 1, 20).await.map(|v| {
                                        Message::AddAnime(AddAnime::UpdateCandidates(q2.clone(), v))
                                    })
                                }
                                .into_task(),
                            )
                        }
                    }
                    AddAnime::UpdateCandidates(string, animes) => {
                        if let Some(query) = self.live.current_add_query.as_mut() {
                            // only satisfy search if the query still matches
                            if string == query.query {
                                query.candidates = animes;
                                query.candidates_dirty = false;
                            } else {
                                // request retry with latest query
                                tasks.push(Message::AddAnime(AddAnime::RefreshCandidates));
                            }
                        }
                    }
                }
            }
            Message::RequestRemove(show_id) => {
                let _ = self.db.shows.drop(show_id);
            }
            Message::ThumbnailLoaded(show_id, handle) => {
                self.thumbnails.insert(show_id, handle);
            }
            Message::Error(r) => {
                error!("{r:?}");
            }
        }
        tasks.batch()
    }
    pub fn title(&self, id: window::Id) -> String {
        "monsoon".to_string()
    }
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![window::close_events().map(Message::WindowClosed)];
        if self
            .live
            .current_add_query
            .as_ref()
            .is_some_and(|v| v.candidates_dirty && !v.query.is_empty())
        {
            subs.push(
                // conservatively rate limit search queries to 75% of the anilist query quota
                time::every(Duration::from_secs(1))
                    .map(|_| Message::AddAnime(AddAnime::RefreshCandidates)),
            );
        }
        Subscription::batch(subs)
    }

    fn make_ani_client(&self) -> Arc<anilist_moe::AniListClient> {
        Arc::clone(&self.live.ani_client)
    }
}

pub struct TaskList {
    inner: Vec<Task<Message>>,
}
impl TaskList {
    fn new() -> Self {
        Self { inner: Vec::new() }
    }
    fn push(&mut self, v: impl Into<Task<Message>>) {
        self.inner.push(v.into());
    }
    fn extend_from<T: Into<Task<Message>>>(&mut self, it: impl IntoIterator<Item = T>) {
        self.inner.extend(it.into_iter().map(|v| v.into()));
    }
    fn batch(self) -> Task<Message> {
        Task::batch(self.inner)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Error(Arc<eyre::Report>),
    MainWindowOpened,

    AddAnime(AddAnime),
    RequestRemove(ShowId),
    LoadThumbnail(ShowId),
    ThumbnailLoaded(ShowId, image::Handle),
    WindowClosed(window::Id),
}

#[derive(Debug, Clone)]
pub enum AddAnime {
    ModifyQuery(String),
    RefreshCandidates,
    UpdateCandidates(String, Vec<Anime>),
    Submit,
    RequestCreateAnilist(i32),
    CreateFromAnilist(Box<anilist_moe::models::Anime>),
}

impl<E: Into<eyre::Report>> From<Result<Message, E>> for Message {
    fn from(value: Result<Message, E>) -> Self {
        match value {
            Ok(v) => v,
            Err(err) => Message::Error(Arc::new(err.into())),
        }
    }
}

trait ElementExt<'a, T> {
    fn erase_element(self) -> Element<'a, T>;
}
impl<'a, T: Into<Element<'a, U>>, U> ElementExt<'a, U> for T {
    fn erase_element(self) -> Element<'a, U> {
        self.into()
    }
}

impl From<Message> for Task<Message> {
    fn from(value: Message) -> Self {
        Task::done(value)
    }
}
trait FutureExt {
    fn into_task(self) -> Task<Message>;
}

impl<T: Into<Message> + Send + 'static, F: Future<Output = T> + Send + 'static> FutureExt for F {
    fn into_task(self) -> Task<Message> {
        Task::perform(self, Into::into)
    }
}
