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
                        let towatch = s.episode_to_watch_idx();
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
                                            .map(|v| v.0 - 1)
                                            .or(s.num_episodes.map(NonZero::get))
                                            .unwrap_or(0)
                                    )),
                                ]
                                .erase_element()
                            ]),
                            widget::button("Remove")
                                .on_press(Message::ModifyShow(id, ModifyShow::RequestRemove)),
                        ]
                        .push_maybe(towatch.map(|(ep, ts)| {
                            widget::button("watch next episode").on_press_with(move || {
                                Message::Play(Play {
                                    show: id,
                                    episode_idx: ep,
                                    pos: ts.unwrap_or(0),
                                    media: None,
                                    stream: None,
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
