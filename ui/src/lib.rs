use std::{
    collections::HashMap,
    fs::{self, File},
    iter::zip,
    ops::Range,
    path::Path,
    sync::Arc,
    time::Duration,
};

use ::nyaa::{AnimeKind, Item, NyaaClient};
use anilist_moe::models::Anime;
use bincode::{Decode, Encode};
use directories::ProjectDirs;
use eyre::{Context, OptionExt, eyre};
use iced::{
    Element, Length, Subscription, Task,
    futures::{future::join_all, stream},
    keyboard::{self, Key},
    time,
    widget::{self, button, image, row},
    window,
};
use log::error;

use rqstream::{ResultExt, Rqstream};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OnceCell, OwnedMutexGuard};

use crate::{
    db::MainDb,
    player::{Play, PlayerSession, PlayerSessionMpv},
    show::{EpochInstant, MediaSource, Show, ShowId, WatchEvent},
};
// TODO support MAL, list/tracker abstraction
pub mod anilist;
pub mod db;
pub mod list;
pub mod nyaa;
pub mod player;
pub mod show;
// TODO integrate rqstream, nyaa

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
    default_source_type: MediaSourceType,
    preferred_name_kind: NameKind,
    anilist: anilist::Config,
    nyaa: nyaa::Config,
    player: PlayerConfig,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerConfig {
    //todo player path etc
    max_remaining_to_complete: u32,
}
impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            max_remaining_to_complete: 120,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub enum MediaSourceType {
    #[default]
    RqNyaa,
    // TODO etc
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
    nyaa: NyaaClient,
    rqstream: Arc<OnceCell<Arc<Rqstream>>>,
    current_player_session: Option<PlayerSession>,
    ani_client: Arc<anilist_moe::AniListClient>,
    current_add_query: Option<AddQuery>,
    couldnt_load_image: image::Handle,
}

#[derive(Default)]
pub struct AddQuery {
    query: String,
    candidates_dirty: bool,
    candidates: Vec<(Option<image::Handle>, Anime)>,
}
pub(crate) const FAILED_LOAD_IMAGE: &[u8] = include_bytes!("../itbroke.jpg");
impl LiveState {
    fn new(conf: &Config) -> Self {
        Self {
            rqstream: Arc::new(OnceCell::new()),
            // todo auth
            ani_client: Arc::new(anilist_moe::AniListClient::new()),
            current_add_query: None,
            couldnt_load_image: image::Handle::from_bytes(FAILED_LOAD_IMAGE),
            current_player_session: None,
            nyaa: NyaaClient::new(conf.nyaa.nyaa.clone()),
        }
    }
}

impl Monsoon {
    pub fn init() -> (Self, Task<Message>) {
        simple_logger::SimpleLogger::new()
            .env()
            .with_level(log::LevelFilter::Off)
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
        row![
            button("+").on_press(Message::AddAnime(AddAnime::Submit)),
            widget::text_input(
                "anime name or anilist ID",
                self.live
                    .current_add_query
                    .as_ref()
                    .map(|v| &*v.query)
                    .unwrap_or(""),
            )
            .on_input(|s| Message::AddAnime(AddAnime::ModifyQuery(s)))
            .on_submit(Message::AddAnime(AddAnime::Submit))
        ]
        .into()
    }

    pub fn view(&'_ self, window: window::Id) -> Element<'_, Message> {
        if window == self.main_window_id {
            let content = if let Some(current) = &self.live.current_add_query {
                widget::column![]
                    .extend(current.candidates.iter().map(|v| {
                        row![
                            widget::image(v.0.as_ref().unwrap_or(&self.live.couldnt_load_image)),
                            widget::button(widget::text({
                                if let Some(titles) = v.1.title.as_ref() {
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
                            .on_press(Message::AddAnime(AddAnime::RequestCreateAnilist(v.1.id),))
                        ]
                        .erase_element()
                    }))
                    .into()
            } else {
                self.view_list()
            };
            widget::column![
                self.draw_top_bar(),
                widget::Rule::horizontal(5),
                widget::scrollable(content).width(Length::Fill)
            ]
            .into()
        } else {
            unimplemented!()
        }
    }

    fn load_thumbnail(&mut self, id: ShowId, tasks: &mut TaskList) {
        let show = match self
            .db
            .shows
            .get(id)
            .ok_or_eyre("tried to load the thumnail for a show not in DB")
        {
            Ok(v) => v,
            Err(e) => {
                tasks.push(Message::Error(Arc::new(e)));
                return;
            }
        };
        let mut should_clear = false;
        if let Some(path) = show.thumbnail.as_ref() {
            match path {
                show::ThumbnailPath::File(path_buf) => {
                    if path_buf.exists() {
                        let p = path_buf.clone();
                        tasks.push(Message::ModifyShow(
                            id,
                            ModifyShow::LoadedThumbnail(image::Handle::from_path(p)),
                        ));
                    } else {
                        should_clear = true;
                    }
                }
                show::ThumbnailPath::Url(path) => {
                    let p = path.clone();

                    tasks.push(Task::future(async move {
                        load_image_url(p)
                            .await
                            .map(|v| Message::ModifyShow(id, ModifyShow::LoadedThumbnail(v)))
                            .into()
                    }));
                }
            }
        }
        if should_clear {
            // remove invalid path thumbnail
            self.db.shows.update_with(id, |v| v.thumbnail = None);
        }
    }
    pub fn with_player_session<
        Out: Into<Message>,
        Fut: Future<Output = Out> + Send + 'static,
        Func: FnOnce(OwnedMutexGuard<PlayerSessionMpv>) -> Fut + Send + 'static,
    >(
        &self,
        f: Func,
    ) -> Task<Message> {
        enum WithSessionState<F> {
            NewSession(F),
            RunCallback(Arc<Mutex<PlayerSessionMpv>>, F),
            Done,
        }
        match &self.live.current_player_session {
            Some(val) => {
                let c = val.instance.clone();
                async move { f(c.lock_owned().await).await.into() }.into_task()
            }
            None => Task::stream(stream::try_unfold(
                WithSessionState::NewSession(f),
                |v| async move {
                    match v {
                        WithSessionState::NewSession(f) => {
                            let r = PlayerSessionMpv::new().await?;
                            let a: Arc<Mutex<PlayerSessionMpv>> = Arc::new(Mutex::new(r));

                            Ok::<_, eyre::Report>(Some((
                                // send NewSession before the user-specified function runs as it may send a message that interacts with the current session state
                                Message::Session(ModifySession::New(a.clone())),
                                WithSessionState::RunCallback(a, f),
                            )))
                        }
                        WithSessionState::RunCallback(sess, f) => Ok(Some((
                            f(sess.lock_owned().await).await.into(),
                            WithSessionState::Done,
                        ))),
                        WithSessionState::Done => Ok(None),
                    }
                },
            ))
            .map(Into::into),
        }
    }
    pub fn get_rqstream(
        &self,
    ) -> impl Future<Output = eyre::Result<Arc<Rqstream>>> + Send + 'static {
        let a2 = Arc::clone(&self.live.rqstream);
        async move {
            a2.get_or_try_init(|| Rqstream::create("127.0.0.1:9000"))
                .await
                .map(Arc::clone)
                .map_err(|v| eyre!(Box::new(v)))
        }
    }
    fn play(&mut self, mut play: Play, tasks: &mut TaskList) {
        let rq = self.get_rqstream();
        tasks.push(self.with_player_session(move |mut session| async move {
            let (url, stream_id) = match &**play.media.as_ref().expect("bug") {
                MediaSource::Magnet(mag) => {
                    let mag = mag.to_owned();
                    let show = play.show;
                    let episode_idx = play.episode_idx;

                    let rq = rq.await?;
                    let info = rq.get_info(mag.to_string()).await.anyhow_to_eyre()?;
                    let mut get_file = None;
                    for (id, file) in info.info.iter_file_details().anyhow_to_eyre()?.enumerate() {
                        // TODO make less dumb
                        let v = file
                            .filename
                            .iter_components()
                            .last()
                            .ok_or_eyre("empty_filename")?
                            .anyhow_to_eyre()?;
                        if matches!(v.split('.').last(), Some("mp4" | "mkv" | "mp3")) {
                            get_file = Some(id);
                            break;
                        }
                    }
                    let file_id = get_file.ok_or_eyre("failed to find video file in torrent")?;
                    let torrent = rq.add_magnet_managed(mag).await.anyhow_to_eyre()?;
                    let show_id: u64 = show.into();
                    let episode_number = episode_idx + 1;
                    let id = format!("{show_id}_e{episode_number}");
                    let handle = rq
                        .stream_file(&torrent, file_id, id)
                        .await
                        .anyhow_to_eyre()?;

                    (
                        format!("http://127.0.0.1:9000/stream/{show_id}_e{episode_number}"),
                        Some(handle),
                    )
                }
                MediaSource::DirectUrl(url) => {
                    let url = url.to_owned();
                    (url, None)
                }
                MediaSource::File(path) => {
                    let p = path.to_string_lossy().to_string();
                    (p, None)
                }
            };
            session.play(url).await?;
            session.seek(play.pos).await?;
            play.stream = stream_id;

            Ok::<_, eyre::Report>(Message::Session(ModifySession::SetPlaying(play)))
        }));
    }
    pub fn handle_stop_playing(&mut self, stopped: Play, tasks: &mut TaskList) {
        tasks.push(Message::Watch(
            stopped.show,
            Watch::Event(WatchEvent {
                episode: stopped.episode_idx,
                ty: show::WatchEventType::Closed(Some(stopped.pos)),
            }),
        ));

        self.db.shows.update_with(stopped.show, |v| {
            if let Some(r) = stopped.remaining
                && r <= self.config.player.max_remaining_to_complete
                && let Some(v) = v.watched_episodes.get_mut(stopped.episode_idx as usize)
            {
                *v = true;
            }
            // cache media source if one is not present already
            if let Some(media) = stopped.media
                && let std::collections::hash_map::Entry::Vacant(vacant_entry) =
                    v.cached_sources.entry(stopped.episode_idx)
            {
                vacant_entry.insert((&*media).clone());
            }
        });
        let rq = self.get_rqstream();
        if let Some(s) = stopped.stream {
            tokio::spawn(async move {
                if let Ok(rq) = rq.await {
                    let _ = rq.stop_streaming(s).await;
                }
            });
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
                    tasks.push(
                        Task::done(Message::Session(ModifySession::Quit)).chain(iced::exit()),
                    );
                }
            }
            Message::MainWindowOpened => {
                self.db
                    .shows
                    .enumerate()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .for_each(|id| self.load_thumbnail(id, &mut tasks));
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
                            q.query
                                .parse()
                                .ok()
                                .or(q.candidates.first().map(|v| v.1.id))
                        }) {
                            tasks.push(Message::AddAnime(AddAnime::RequestCreateAnilist(id)));
                        }
                    }
                    AddAnime::CreateFromAnilist(anime) => {
                        let mut s = Show::default();
                        s.update_with(&anime);
                        let id = self.db.shows.insert(s);
                        self.load_thumbnail(id, &mut tasks);
                    }
                    AddAnime::RequestCreateAnilist(v) => 'add: {
                        // todo don't exit add mode if shift held or something
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
                                    let v = client.anime().search(&q, 1, 20).await?;
                                    let images = join_all(v.iter().filter_map(|v| {
                                        v.cover_image.as_ref().and_then(|v| {
                                            v.medium.as_ref().map(|v| load_image_url(v.clone()))
                                        })
                                    }))
                                    .await;

                                    Result::<_, eyre::Report>::Ok(Message::AddAnime(
                                        AddAnime::UpdateCandidates(
                                            q2.clone(),
                                            zip(images.into_iter().map(|v| v.ok()), v).collect(),
                                        ),
                                    ))
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
                    AddAnime::Exit => self.live.current_add_query = None,
                }
            }
            Message::ModifyShow(show_id, modify) => match modify {
                ModifyShow::LoadedThumbnail(handle) => {
                    let _ = self.thumbnails.insert(show_id, handle);
                }
                ModifyShow::RequestRemove => {
                    let _ = self.db.shows.drop(show_id);
                }
                ModifyShow::SetWatched(ep, watched) => {
                    let _ = self.db.shows.update_with(show_id, |show| {
                        match show.watched_episodes.get_mut(ep as usize) {
                            Some(r) => *r = watched,
                            None => log::warn!(
                                "tried to set a out-of-bounds episode as watched or unwatched"
                            ),
                        }
                    });
                }
                ModifyShow::FlushSourceCache(range) => {
                    let _ = self.db.shows.update_with(show_id, |show| {
                        for i in range {
                            show.cached_sources.remove(&i);
                        }
                    });
                }
            },
            Message::Error(r) => {
                error!("{r:?}");
            }
            Message::Session(s) => match s {
                ModifySession::New(mutex) => {
                    self.live.current_player_session = Some(PlayerSession {
                        instance: mutex,
                        playing: None,
                    })
                }
                ModifySession::SetPlaying(play) => {
                    if let Some(previous) = self
                        .live
                        .current_player_session
                        .as_mut()
                        .and_then(|v| v.playing.replace(play))
                    {
                        tasks.push(Message::Watch(
                            previous.show,
                            Watch::Event(WatchEvent {
                                episode: previous.episode_idx,
                                ty: show::WatchEventType::Closed(Some(previous.pos)),
                            }),
                        ));
                    }
                }
                ModifySession::SetPosRemaining(new_pos, new_remaining) => {
                    if let Some(p) = self
                        .live
                        .current_player_session
                        .as_mut()
                        .and_then(|v| v.playing.as_mut())
                    {
                        p.pos = new_pos;
                        p.remaining = Some(new_remaining);
                    }
                }
                ModifySession::PollPos => {
                    tasks.push(self.with_player_session(|mut player| async move {
                        if player.dead().await {
                            Ok(Message::Session(ModifySession::Quit))
                        } else {
                            Ok::<_, eyre::Report>(Message::Session(ModifySession::SetPosRemaining(
                                player.pos().await?,
                                player.remaining().await?,
                            )))
                        }
                    }))
                }
                ModifySession::Quit => {
                    if let Some(v) = self.live.current_player_session.take() {
                        if let Some(p) = v.playing {
                            self.handle_stop_playing(p, &mut tasks);
                        }
                        let _ = tokio::spawn(async move { v.instance.lock().await.quit().await });
                    }
                }
            },
            Message::Watch(show_id, watch) => match watch {
                Watch::Event(watch_event) => {
                    unwrap!(
                        self.db
                            .shows
                            .update_with(show_id, |v| v
                                .watch_history
                                .insert(EpochInstant::now(), watch_event))
                            .ok_or_eyre("watch event key should have been unique")
                    );
                }
            },
            Message::Play(mut play) => {
                let show = unwrap!(
                    self.db
                        .shows
                        .get(play.show)
                        .ok_or_eyre("tried to play a show not in DB")
                );
                if play.media.is_some() {
                    log::info!("playing with given source");
                    self.play(play, &mut tasks);
                } else if let Some(s) = show.cached_sources.get(&play.episode_idx) {
                    log::info!("playing with cached source");
                    play.media = Some(Arc::new(s.clone()));
                    self.play(play, &mut tasks);
                } else {
                    log::info!("no media source cache hit. Attempting to locate");
                    match self.config.default_source_type {
                        MediaSourceType::RqNyaa => {
                            let q = show.nyaa_query_for(
                                &self.config,
                                // episode numbers are 1-indexed
                                play.episode_idx + 1,
                                AnimeKind::SubEnglish,
                            );
                            let conf = self.config.nyaa.clone();
                            let nyaa = self.live.nyaa.clone();
                            tasks.push(
                                async move {
                                    let mut chosen = None;
                                    for query in q {
                                        let resp = nyaa.search(&query).await?;

                                        let score = |it: &Item| -> f64 {
                                            let s = *it.size.as_ref().unwrap() as f64;
                                            if s == 0.0 {
                                                return f64::MAX;
                                            }
                                            // negative if below the preferred size, positive if past it
                                            // normalize to percentage above/below preferred size
                                            ((s - conf.preferred_size as f64) / s) * 10.0
                                                - it.seeders as f64 / 10.0
                                                + it.leechers as f64 / 100.0
                                        };
                                        let nresp = resp.results.len();
                                        let mut candidates = resp
                                            .results
                                            .into_iter()
                                            .filter(|v| {
                                                v.seeders >= conf.min_seeders
                                                    && v.size
                                                        .as_ref()
                                                        .is_ok_and(|&v| v <= conf.max_size)
                                            })
                                            .collect::<Vec<_>>();
                                        log::info!("tried query: {query:#?}, total responses: {nresp}, qualified responses: {}", candidates.len());

                                        candidates.sort_by(|a, b| {
                                            score(a)
                                                .partial_cmp(&score(b))
                                                .expect("total ordering of scores")
                                        });
                                        chosen = candidates.into_iter().next();
                                        if chosen.is_some() {
                                            break;
                                        }
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                    }

                                    let chosen = chosen.ok_or_eyre(
                                        "failed to locate qualified torrent for anime",
                                    )?;
                                    log::info!("got magnet link {}", &chosen.magnet_link);
                                    play.media = Some(Arc::new(MediaSource::Magnet(
                                        chosen.magnet_link.into(),
                                    )));

                                    // now we have a media source :3
                                    Ok::<_, eyre::Report>(Message::Play(play))
                                }
                                .into_task(),
                            );
                        }
                    }
                }
            }
        }
        tasks.batch()
    }

    pub fn title(&self, id: window::Id) -> String {
        "monsoon".to_string()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![window::close_events().map(Message::WindowClosed)];
        if let Some(q) = &self.live.current_add_query {
            subs.push(iced::keyboard::on_key_press(|k, _| {
                if k == Key::Named(keyboard::key::Named::Escape) {
                    Some(Message::AddAnime(AddAnime::Exit))
                } else {
                    None
                }
            }));
            if q.candidates_dirty && !q.query.is_empty() {
                subs.push(
                    // conservatively limit search queries to 75% of the anilist ratelimit
                    time::every(Duration::from_secs(1))
                        .map(|_| Message::AddAnime(AddAnime::RefreshCandidates)),
                );
            }
        }
        if self
            .live
            .current_player_session
            .as_ref()
            .is_some_and(|v| v.playing.is_some())
        {
            // poll player position
            subs.push(
                time::every(Duration::from_secs(1))
                    .map(|_| Message::Session(ModifySession::PollPos)),
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
    fn batch(self) -> Task<Message> {
        Task::batch(self.inner)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Error(Arc<eyre::Report>),
    MainWindowOpened,
    WindowClosed(window::Id),
    AddAnime(AddAnime),
    ModifyShow(ShowId, ModifyShow),
    Watch(ShowId, Watch),
    Session(ModifySession),
    Play(Play),
}
#[derive(Debug, Clone)]
pub enum Watch {
    Event(WatchEvent),
}
#[derive(Debug, Clone)]
pub enum ModifySession {
    New(Arc<Mutex<PlayerSessionMpv>>),
    SetPlaying(Play),
    SetPosRemaining(u32, u32),
    PollPos,
    Quit,
}

#[derive(Debug, Clone)]
pub enum ModifyShow {
    FlushSourceCache(Range<u32>),
    SetWatched(u32, bool),
    LoadedThumbnail(image::Handle),
    RequestRemove,
}

#[derive(Debug, Clone)]
pub enum AddAnime {
    ModifyQuery(String),
    RefreshCandidates,
    UpdateCandidates(String, Vec<(Option<image::Handle>, Anime)>),
    Submit,
    Exit,
    RequestCreateAnilist(i32),
    CreateFromAnilist(Box<anilist_moe::models::Anime>),
}

async fn load_image_url(url: String) -> eyre::Result<image::Handle> {
    let resp = reqwest::get(url)
        .await
        .map_err(reqwest::Error::without_url)
        .wrap_err("failed to request image from url")?;
    resp.bytes()
        .await
        .wrap_err("image response bytes not present")
        .map(image::Handle::from_bytes)
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
trait IntoTask {
    fn into_task(self) -> Task<Message>;
}

impl<T: Into<Message> + Send + 'static, F: Future<Output = T> + Send + 'static> IntoTask for F {
    fn into_task(self) -> Task<Message> {
        Task::perform(self, Into::into)
    }
}
