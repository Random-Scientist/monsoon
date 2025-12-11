use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    iter::zip,
    path::{Path, PathBuf},
    sync::Arc,
};

use ::nyaa::NyaaClient;
use anilist_moe::models::Anime;
use bincode::{Decode, Encode};
use directories::ProjectDirs;
use eyre::{Context, OptionExt, eyre};

use iced_core::{image, keyboard::Key};

use iced_runtime::{
    Task,
    futures::{
        Subscription,
        futures::{future::join_all, stream::try_unfold},
    },
    keyboard,
};
use log::error;

#[cfg(not(test))]
use iced_runtime::futures::backend::default::time::every;
#[cfg(not(test))]
use std::time::Duration;

use rqstream::Rqstream;
use serde::{Deserialize, Serialize};

use tokio::sync::{Mutex, OnceCell, OwnedMutexGuard};

#[cfg(feature = "discord")]
use crate::discord::UpdatePresence;
#[cfg(feature = "discord")]
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    db::MainDb,
    discord::DiscordPresence,
    media::{AnyMedia, Media, PlayRequest, PlayableMedia, PlayingMedia},
    player::{PlayerSession, PlayerSessionMpv},
    show::{EpochInstant, Show, ShowId, WatchEvent},
    source::{Source, nyaa::Nyaa},
    util::NoDebug,
};

// hi :3

// TODO support MAL, list/tracker abstraction
pub mod anilist;
pub mod db;
pub mod player;
pub mod show;

pub mod media;
pub mod source;

#[cfg(feature = "discord")]
pub mod discord;

pub mod util;

#[derive(
    Debug, Default, Clone, Encode, Decode, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum NameKind {
    #[default]
    English,
    Romaji,
    Synonym,
    Native,
}

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_source_type: SourceType,
    pub preferred_name_kind: NameKind,
    pub anilist: anilist::Config,
    pub nyaa: source::nyaa::Config,
    pub player: PlayerConfig,
    pub db_path: Option<PathBuf>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerConfig {
    //TODO player path etc
    pub max_remaining_to_complete: u32,
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
    pub main_window_id: iced_core::window::Id,
    pub more_info_windows: HashMap<iced_core::window::Id, ShowId>,
    pub dirs: ProjectDirs,
    pub thumbnails: HashMap<ShowId, image::Handle>,
    pub db: MainDb,
    pub config: Config,
    pub live: LiveState,
}

pub struct LiveState {
    pub nyaa: NyaaClient,
    pub rqstream: Arc<OnceCell<Arc<Rqstream>>>,
    pub current_player_session: Option<PlayerSession>,
    pub ani_client: Arc<anilist_moe::AniListClient>,
    pub current_add_query: Option<AddQuery>,
    pub couldnt_load_image: image::Handle,
    pub show_source_dedupe: HashMap<ShowId, HashSet<Arc<str>>>,
    #[cfg(feature = "discord")]
    discord_rpc: UnboundedSender<UpdatePresence>,
}

#[derive(Default)]
pub struct AddQuery {
    pub query: String,
    pub candidates_dirty: bool,
    pub candidates: Vec<(Option<image::Handle>, Anime)>,
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
            #[cfg(feature = "discord")]
            discord_rpc: DiscordPresence::spawn(),
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
            .with_module_level("app", log::LevelFilter::Trace)
            .init()
            .expect("no logger to be set");
        let dirs =
            directories::ProjectDirs::from("rs", "rsci", "monsoon").expect("directories to load");
        let config = Config::load(dirs.config_dir().join("config.toml"));
        let d;
        let db_path = match config.db_path.as_ref() {
            Some(v) => v,
            None => {
                d = dirs.config_dir().join("db");
                &d
            }
        };
        let db = MainDb::open(db_path);
        let (main_window_id, task) = iced_runtime::window::open(Default::default());
        let live = LiveState::new(&config);

        (
            Self {
                dirs,
                db,
                config,
                main_window_id,
                live,
                thumbnails: HashMap::new(),
                more_info_windows: HashMap::new(),
            },
            task.map(|_| Message::MainWindowOpened),
        )
    }
    pub fn get_show_thumb(&self, id: ShowId) -> image::Handle {
        self.thumbnails
            .get(&id)
            .cloned()
            .unwrap_or(self.live.couldnt_load_image.clone())
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
            None => Task::stream(try_unfold(
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
                self.more_info_windows.remove(&id);
                if id == self.main_window_id {
                    self.db.shows.flush_all();

                    tasks.extend(self.quit_player_session());

                    return tasks.batch().chain(iced_runtime::exit());
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
                ModifyShow::ShowMoreInfo => {
                    let (id, task) = iced_runtime::window::open(Default::default());
                    self.more_info_windows.insert(id, show_id);
                    tasks.push(task.discard());
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
                    tasks.extend(self.cleanup_show());

                    if let Some(session) = &mut self.live.current_player_session {
                        #[cfg(feature = "discord")]
                        {
                            if let Some(show) = self.db.shows.get(play.show) {
                                let _ = self.live.discord_rpc.send(UpdatePresence::Show {
                                    title: show.get_preferred_name(&self.config).into(),
                                    thumb_url: show.thumbnail.as_ref().and_then(|v| match v {
                                        show::ThumbnailPath::Url(u) => Some(u.clone()),
                                        _ => None,
                                    }),
                                    episode_idx: (show.watched_episodes.len() > 1)
                                        .then_some(play.episode_idx),
                                });
                                let _ = self.live.discord_rpc.send(UpdatePresence::Timestamp {
                                    timestamp_secs: session.player_pos,
                                    remaining_secs: session.player_remaining,
                                });
                            }
                        }

                        session.playing = Some(play);
                    }
                }
                ModifySession::SetPosRemaining(new_pos, new_remaining) => {
                    if let Some(p) = self.live.current_player_session.as_mut() {
                        p.player_pos = new_pos;
                        p.player_remaining = new_remaining;
                        let _ = self.live.discord_rpc.send(UpdatePresence::Timestamp {
                            timestamp_secs: p.player_pos,
                            remaining_secs: p.player_remaining,
                        });
                        if let Some(media) = &p.playing
                            && p.player_remaining <= self.config.player.max_remaining_to_complete
                            && self.db.shows.get(media.show).is_some_and(|v| {
                                v.watched_episodes
                                    .get(media.episode_idx as usize)
                                    .is_some_and(|v| !v)
                            })
                        {
                            let _ = self.db.shows.update_with(media.show, |v| {
                                v.watched_episodes[media.episode_idx as usize] = true;
                            });
                        }
                    }
                }
                ModifySession::PollPos => {
                    tasks.push(self.with_player_session(|mut player| async move {
                        if player.dead().await {
                            return Ok(Message::Session(ModifySession::Quit));
                        }
                        return Ok::<_, eyre::Report>(Message::Session(
                            ModifySession::SetPosRemaining(
                                player.pos().await,
                                player.remaining().await,
                            ),
                        ));
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
                            log::trace!("nyaa searching for direct episode");
                            let selected_item = match episode.into_iter().next() {
                                Some(v) => Some(v),
                                None => {
                                    let batch = batch_query.await?;
                                    log::info!("nyaa fell back to batch searching");
                                    batch.into_iter().next()
                                }
                            }
                            .ok_or_eyre("nyaa failed to select a source")?;

                            log::trace!("nyaa selected item {selected_item:?}");
                            let playable_fut =
                                Box::into_pin(NoDebug::into_inner(selected_item.media));

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
    pub fn cleanup_show(&mut self) -> Option<Task<Message>> {
        #[cfg(feature = "discord")]
        {
            let _ = self.live.discord_rpc.send(UpdatePresence::Clear);
        }
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
                    // TODO pause until memory budget or something. Keep medias in a paused state up to a certain configurable memory budget for quick reentry
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
    pub fn quit_player_session(&mut self) -> Option<iced_runtime::Task<Message>> {
        let mut cleanup = self.cleanup_show()?;
        if let Some(quit) = self.live.current_player_session.take().map(|v| async move {
            v.instance.lock().await.quit().await;
            // give it a little time
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }) {
            cleanup = cleanup.chain(Task::future(quit).discard());
        }
        Some(cleanup)
    }

    pub fn title(&self, _id: iced_core::window::Id) -> String {
        "monsoon".to_string()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![iced_runtime::window::close_events().map(Message::WindowClosed)];

        if let Some(q) = &self.live.current_add_query {
            subs.push(iced_runtime::futures::keyboard::listen().filter_map(|k| {
                matches!(
                    k,
                    keyboard::Event::KeyPressed {
                        key: Key::Named(keyboard::key::Named::Escape),
                        ..
                    }
                )
                .then_some(Message::AddAnime(AddAnime::Exit))
            }));
            if q.candidates_dirty && !q.query.is_empty() {
                #[cfg(not(test))]
                subs.push(
                    // conservatively limit search queries to 75% of the anilist ratelimit
                    every(Duration::from_secs(1))
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
            #[cfg(not(test))]
            subs.push(
                every(Duration::from_secs(1)).map(|_| Message::Session(ModifySession::PollPos)),
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
    WindowClosed(iced_core::window::Id),
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
    ShowMoreInfo,
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
