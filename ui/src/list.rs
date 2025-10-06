use std::num::NonZero;

use iced::{
    Element,
    widget::{self, Column, row},
};
use itertools::Itertools;
use log::warn;

use crate::{ElementExt, Message, ModifyShow, Monsoon, player::Play};

impl Monsoon {
    #[allow(unstable_name_collisions)]
    pub(crate) fn view_list(&'_ self) -> Element<'_, Message> {
        Column::new()
            .extend(
                self.db
                    .shows
                    .enumerate()
                    .map(|(id, s)| {
                        let name: &str = s.get_preferred_name(&self.config);
                        let image = self.thumbnails.get(&id).map(widget::image);
                        let towatch = s.episode_to_watch();
                        if !towatch.is_some() {
                            warn!(
                                "failed to compute next episode to watch for show {name} ({id:?})"
                            );
                        }
                        let el: Element<Message> = row![
                            widget::button(row![
                                image.unwrap_or(widget::image(&self.live.couldnt_load_image)),
                                widget::column![
                                    widget::text(name).erase_element(),
                                    widget::text(format!(
                                        "watched episodes: {}",
                                        towatch
                                            .map(|v| v.0)
                                            .or(s.num_episodes.map(NonZero::get))
                                            .unwrap_or(0)
                                    )),
                                ]
                                .erase_element()
                            ]),
                            widget::button("Remove")
                                .on_press(Message::ModifyShow(id, ModifyShow::RequestRemove)),
                            widget::button("mark next watched").on_press_maybe(
                                s.episode_to_watch().map(|(idx, _)| Message::ModifyShow(
                                    id,
                                    ModifyShow::SetWatched(idx, true)
                                ))
                            ),
                            widget::button("mark last unwatched").on_press_maybe(
                                s.watched_episodes.iter().enumerate().rev().find_map(
                                    |(idx, val)| val.then_some(Message::ModifyShow(
                                        id,
                                        ModifyShow::SetWatched(idx as u32, false)
                                    ))
                                )
                            ),
                            widget::button("flush media cache").on_press(Message::ModifyShow(
                                id,
                                ModifyShow::FlushSourceCache(
                                    0..s.num_episodes.map(NonZero::get).unwrap_or(0)
                                )
                            ))
                        ]
                        .push_maybe(towatch.map(|(ep, ts)| {
                            widget::button("watch next episode").on_press_with(move || {
                                Message::Play(Play {
                                    show: id,
                                    episode_idx: ep,
                                    pos: ts.unwrap_or(0),
                                    media: None,
                                    stream: None,
                                    remaining: None,
                                })
                            })
                        }))
                        .into();
                        el
                    })
                    // FIXME replace with std implementation and remove itertools when it is stabilized
                    .intersperse_with(|| widget::Rule::horizontal(5).into()),
            )
            .into()
    }
}
