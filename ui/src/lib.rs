use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    iter::zip,
    path::Path,
    sync::Arc,
    time::Duration,
};

use ::nyaa::NyaaClient;
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

use rqstream::Rqstream;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OnceCell, OwnedMutexGuard};

use crate::{
    db::MainDb,
    media::{AnyMedia, Media, PlayRequest, PlayableMedia, PlayingMedia},
    player::{PlayerSession, PlayerSessionMpv},
    show::{EpochInstant, Show, ShowId, WatchEvent},
    source::{Source, nyaa::Nyaa},
    util::NoDebug,
};

// TODO support MAL, list/tracker abstraction
pub mod anilist;
pub mod db;
pub mod list;
pub mod player;
pub mod show;

pub mod media;
pub mod source;

pub mod util;
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
    default_source_type: SourceType,
    preferred_name_kind: NameKind,
    anilist: anilist::Config,
    nyaa: source::nyaa::Config,
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
pub enum SourceType {
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
    show_source_dedupe: HashMap<ShowId, HashSet<Arc<str>>>,
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
            show_source_dedupe: HashMap::new(),
        }
    }

    pub fn get_rqstream(
        &self,
    ) -> impl Future<Output = eyre::Result<Arc<Rqstream>>> + Send + 'static {
        let a2 = Arc::clone(&self.rqstream);
        async move {
            a2.get_or_try_init(|| Rqstream::create("127.0.0.1:9000"))
                .await
                .map(Arc::clone)
                .map_err(|v| eyre!(Box::new(v)))
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

    pub fn play(&mut self, req: PlayRequest, media: PlayableMedia) -> Task<Message> {
        // preemptively start unpausing the download/media preparation before the player is up
        let h = media.lifecycle.clone().map(|mut v| {
            tokio::spawn(async move { v.update(media::MediaLifecycle::Resume).await })
        });

        let PlayRequest {
            show,
            episode_idx,
            pos,
        } = req;
        self.with_player_session(move |mut session| async move {
            // wait for the media lifecycle change to finish
            if let Some(join_handle) = h
                && let Some(err) = join_handle.await??
            {
                // the jank
                return Ok(Message::Error(err));
            }

            session
                .play(match &media.playable {
                    media::Playable::Url(s) => s.clone(),
                    media::Playable::File(path_buf) => path_buf.to_string_lossy().into(),
                })
                .await?;
            session.seek(pos).await?;

            Ok::<_, eyre::Report>(Message::Session(ModifySession::SetPlaying(PlayingMedia {
                show,
                episode_idx,
                media,
            })))
        })
        .chain(Task::done(Message::Watch(
            show,
            Watch::Event(WatchEvent {
                episode: episode_idx,
                ty: show::WatchEventType::Opened,
            }),
        )))
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

                    tasks.extend(self.quit_player_session());

                    return tasks.batch().chain(iced::exit());
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
                ModifyShow::FlushSourceCache => {
                    let _ = self.db.shows.update_with(show_id, |show| {
                        show.media_cache.clear();
                    });
                }
                ModifyShow::CacheMedia(any_media) => {
                    let _ = self
                        .db
                        .shows
                        .update_with(show_id, move |v| v.media_cache.push(any_media));
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
                        player_pos: 0,
                        player_remaining: 0,
                    })
                }
                ModifySession::SetPlaying(play) => {
                    tasks.extend(self.stop_show());
                    if let Some(session) = &mut self.live.current_player_session {
                        session.playing = Some(play);
                    }
                }
                ModifySession::SetPosRemaining(new_pos, new_remaining) => {
                    if let Some(p) = self.live.current_player_session.as_mut() {
                        p.player_pos = new_pos;
                        p.player_remaining = new_remaining;
                    }
                }
                ModifySession::PollPos => {
                    tasks.push(self.with_player_session(|mut player| async move {
                        if player.dead().await {
                            Ok(Message::Session(ModifySession::Quit))
                        } else {
                            loop {
                                if let (Some(pos), Some(remaining)) =
                                    (player.pos().await, player.remaining().await)
                                {
                                    break Ok::<_, eyre::Report>(Message::Session(
                                        ModifySession::SetPosRemaining(pos, remaining),
                                    ));
                                }
                            }
                        }
                    }))
                }
                ModifySession::Quit => {
                    tasks.extend(self.quit_player_session());
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
            Message::RequestPlay(req) => 'play: {
                let show = unwrap!(
                    self.db
                        .shows
                        .get(req.show)
                        .ok_or_eyre("tried to play a show not in DB")
                );
                let name = show.get_preferred_name(&self.config);
                for media in show.media_cache.iter() {
                    if media.has_ep(req.episode_idx) {
                        let fut = media.play(&req, &mut self.live).unwrap();
                        log::trace!(
                            "using cached source {media:?} for episode {} of show {name}",
                            req.episode_idx,
                        );
                        tasks.push(
                            async move {
                                let media = Box::into_pin(fut).await?;
                                Ok::<_, eyre::Report>(Message::Play(req, media))
                            }
                            .into_task(),
                        );

                        break 'play;
                    }
                }
                log::info!("failed to locate cached source for show {name}");
                let episode_query =
                    Nyaa.query(&mut self.live, &self.config, show, Some(req.episode_idx));
                let batch_query = Nyaa.query(&mut self.live, &self.config, show, None);
                match self.config.default_source_type {
                            SourceType::RqNyaa => tasks.push(
                                async move {
                                    let episode = episode_query.await?;
                                    log::trace!("nyaa responses for direct episode: {episode:?}");
                                    let selected_item = match episode.into_iter().next() {
                                        Some(v) => Some(v),
                                        None => {
                                            let batch = batch_query.await?;
                                            log::trace!("nyaa fell back to batch searching. got responses: {batch:?}");
                                            batch.into_iter().next()
                                        },
                                    }.ok_or_eyre("nyaa failed to select a source")?;

                                    log::trace!("nyaa selected item {selected_item:?}");
                                    let playable_fut = Box::into_pin(NoDebug::into_inner(selected_item.media));

                                    Ok::<_, eyre::Report>(Message::MakePlayable(req, playable_fut.await?))
                                }
                                .into_task(),
                            ),
                        }
            }
            Message::Play(req, play) => {
                tasks.push(self.play(req, play));
            }
            Message::MakePlayable(play_request, any_media) => 'branch: {
                let Some(show) = self.db.shows.get(play_request.show) else {
                    break 'branch;
                };
                let dedupe = self
                    .live
                    .show_source_dedupe
                    .entry(play_request.show)
                    .or_insert_with(|| show.media_cache.iter().map(|v| v.identifier()).collect());
                if !dedupe.insert(any_media.identifier()) {
                    log::error!("fetched a new media source but it was already in the cache!");
                    break 'branch;
                }

                let playable_fut = Box::into_pin(unwrap!(
                    any_media
                        .play(&play_request, &mut self.live)
                        .ok_or_eyre("expected media to have an episode that was not present!")
                ));
                let _ = self
                    .db
                    .shows
                    .update_with(play_request.show, |v| v.media_cache.push(any_media));
                tasks.push(
                    async move {
                        Ok::<_, eyre::Report>(Message::Play(play_request, playable_fut.await?))
                    }
                    .into_task(),
                );
            }
        }
        tasks.batch()
    }
    pub fn stop_show(&mut self) -> Option<Task<Message>> {
        let sess = self.live.current_player_session.as_mut()?;
        let to_stop = sess.playing.take()?;
        Some(
            Task::done(Message::Watch(
                to_stop.show,
                Watch::Event(WatchEvent {
                    episode: to_stop.episode_idx,
                    ty: show::WatchEventType::Closed(Some(sess.player_pos)),
                }),
            ))
            .chain(
                Task::future(async move {
                    // TODO pause until memory budget or something. Keep torrent medias in a paused state up to a certain configurable memory budget for quick reentry
                    if let Some(mut s) = to_stop.media.lifecycle {
                        s.update(media::MediaLifecycle::Destroy).await
                    } else {
                        Ok(None)
                    }
                })
                .discard(),
            ),
        )
    }
    pub fn quit_player_session(&mut self) -> Option<iced::Task<Message>> {
        let mut cleanup = self.stop_show()?;
        if let Some(quit) = self.live.current_player_session.take().map(|v| async move {
            v.instance.lock().await.quit().await;
        }) {
            cleanup = cleanup.chain(Task::future(quit).discard());
        }
        Some(cleanup)
    }

    pub fn title(&self, _id: window::Id) -> String {
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
    fn extend(&mut self, vals: impl IntoIterator<Item = Task<Message>>) {
        self.inner.extend(vals);
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
    RequestPlay(PlayRequest),
    MakePlayable(PlayRequest, AnyMedia),
    Play(PlayRequest, PlayableMedia),
}
#[derive(Debug, Clone)]
pub enum Watch {
    Event(WatchEvent),
}
#[derive(Debug, Clone)]
pub enum ModifySession {
    New(Arc<Mutex<PlayerSessionMpv>>),
    SetPlaying(PlayingMedia),
    SetPosRemaining(u32, u32),
    PollPos,
    Quit,
}

#[derive(Debug, Clone)]
pub enum ModifyShow {
    CacheMedia(AnyMedia),
    FlushSourceCache,
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
