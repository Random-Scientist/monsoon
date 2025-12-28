use app::{
    AddAnime, Config, Message, ModifyShow, Monsoon, NameKind,
    media::PlayRequest,
    show::{Show, ShowId},
};
use helpers::{info_text, large_bold, sizes::WithSizeExt, subdivision::Subdivision};
use iced::{
    Alignment as A, Element, Font,
    Length::{self, Fill},
    widget::{self, Container, button, row},
    window,
};

use crate::style::{UI_SIZES, rounded_box};
use std::{fmt::Display, num::NonZero};

use itertools::Itertools;

pub mod style;

pub trait MonsoonExt {
    fn view(&'_ self, window: window::Id) -> Element<'_, Message>;
}
impl MonsoonExt for Monsoon {
    fn view(&'_ self, window: window::Id) -> Element<'_, Message> {
        if window == self.main_window_id {
            let content = if let Some(current) = &self.live.current_add_query {
                widget::column(current.candidates.iter().map(|v| {
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
                view_list(self)
            };
            widget::column![
                view_top_bar(self),
                widget::scrollable(content).width(Length::Fill)
            ]
            .spacing(UI_SIZES.size10.get())
            .erase_element()
        } else if let Some(&id) = self.more_info_windows.get(&window)
            && let Some(s) = self.db.shows.get(id)
        {
            let content = view_show_inlay(&self, s, id);
            widget::scrollable(content).into()
        } else {
            unimplemented!()
        }
    }
}
fn view_show_inlay<'a>(m: &Monsoon, s: &'a Show, id: ShowId) -> Container<'a, Message> {
    let name: &str = s.get_preferred_name(&m.config);

    let thumb = widget::image(m.get_show_thumb(id))
        .content_fit(iced::ContentFit::Fill)
        .filter_method(widget::image::FilterMethod::Linear)
        .exact_size(UI_SIZES.thumb_image_size.get());
    let towatch = s.next_episode();
    widget::container(
        row![
            thumb,
            widget::column![
                // UI_SIZES.title_font_size.get()
                large_bold(name, 25.0),
                widget::text(format!(
                    "watched episodes: {}",
                    towatch
                        .map(|v| v.0)
                        .or(s.num_episodes.map(NonZero::get))
                        .unwrap_or(0)
                ))
                .font(Font {
                    family: iced::font::Family::Serif,
                    ..Default::default()
                })
                .size(UI_SIZES.info_font_size.get()),
                widget::text({
                    // frankly this is dumb but it seemed funny at the time
                    let (a, eps): (&dyn Display, _);
                    if let Some(nzep) = s.num_episodes {
                        eps = nzep.get();
                        a = &eps;
                    } else {
                        a = &"??";
                    }
                    format!("total episodes: {}", a)
                })
                .font(Font {
                    family: iced::font::Family::Serif,
                    ..Default::default()
                })
                .size(UI_SIZES.info_font_size.get()),
            ]
        ]
        .spacing(UI_SIZES.size10.get()),
    )
    .style(rounded_box(UI_SIZES.size10.get()))
    .padding(UI_SIZES.pad20.get())
}

#[allow(unstable_name_collisions)]
pub(crate) fn view_list(monsoon: &'_ Monsoon) -> Element<'_, Message> {
    widget::column(
        monsoon
            .db
            .shows
            .enumerate()
            .map(|(id, s)| {
                let towatch = s.next_episode();

                let cont = view_show_inlay(monsoon, s, id);
                let sz = UI_SIZES.info_font_size.get();
                let bts = UI_SIZES.add_sub_button_size.get();
                let watch_unwatch = widget::column![
                    widget::button(info_text("+", sz))
                        .on_press_maybe(s.next_episode().map(|(idx, _)| {
                            Message::ModifyShow(id, ModifyShow::SetWatched(idx, true))
                        }))
                        .exact_size(bts),
                    widget::button(info_text("-", sz))
                        .on_press_maybe(s.watched_episodes.iter().enumerate().rev().find_map(
                            |(idx, val)| {
                                val.then_some(Message::ModifyShow(
                                    id,
                                    ModifyShow::SetWatched(idx as u32, false),
                                ))
                            },
                        ))
                        .exact_size(bts),
                    widget::button(info_text("…", sz))
                        .on_press(Message::ModifyShow(id, ModifyShow::ShowMoreInfo))
                        .exact_size(bts)
                ]
                .spacing(UI_SIZES.size10.get())
                .padding(UI_SIZES.pad10.get())
                .align_x(A::Center);

                let playnext = towatch.map(|(ep, ts)| {
                    Message::RequestPlay(PlayRequest {
                        show: id,
                        episode_idx: ep,
                        pos: ts.unwrap_or(0),
                    })
                });
                let next_ep = widget::button(info_text("next episode", sz))
                    .on_press_maybe(playnext)
                    .style(widget::button::success);
                let manage = widget::column![
                    widget::button(info_text("remove", sz))
                        .on_press(Message::ModifyShow(id, ModifyShow::RequestRemove)),
                    widget::button(info_text("flush cache", sz))
                        .on_press(Message::ModifyShow(id, ModifyShow::FlushSourceCache)),
                    next_ep
                ]
                .spacing(UI_SIZES.size10.get())
                .padding(UI_SIZES.pad10.get())
                .align_x(A::Center);

                let control = row![watch_unwatch, manage].align_y(A::Center);

                widget::container(Subdivision::from_vec(vec![
                    (
                        cont.width(Length::FillPortion(1)).erase_element(),
                        UI_SIZES.cont_max_width.get(),
                    ),
                    (
                        control.width(Length::FillPortion(1)).erase_element(),
                        f32::MAX,
                    ),
                ]))
                .padding(UI_SIZES.pad10.get())
                .align_y(A::Center)
                .erase_element()
            })
            // FIXME replace with std implementation and remove itertools when it is stabilized
            .intersperse_with(|| widget::rule::horizontal(5).into()),
    )
    .into()
}

fn view_top_bar(monsoon: &'_ Monsoon) -> Element<'_, Message> {
    let serif = Font {
        family: iced::font::Family::Serif,
        ..Default::default()
    };
    let sz = UI_SIZES.info_font_size.get();
    row![
        button(info_text("+", sz))
            .on_press_maybe(
                monsoon
                    .live
                    .current_add_query
                    .is_some()
                    .then_some(Message::AddAnime(AddAnime::Submit))
            )
            .exact_size(UI_SIZES.add_sub_button_size.get()),
        widget::text_input(
            "anime name or anilist ID",
            monsoon
                .live
                .current_add_query
                .as_ref()
                .map(|v| &*v.query)
                .unwrap_or(""),
        )
        .line_height(widget::text::LineHeight::Absolute(iced::Pixels(
            UI_SIZES.add_sub_button_size.get().height - 10.0
        )))
        .size(UI_SIZES.info_font_size.get())
        .font(serif)
        .on_input(|s| Message::AddAnime(AddAnime::ModifyQuery(s)))
        .on_submit(Message::AddAnime(AddAnime::Submit))
    ]
    .align_y(A::Center)
    .spacing(UI_SIZES.size10.get())
    .into()
}

trait ElementExt<'a, T> {
    fn erase_element(self) -> Element<'a, T>;
}

impl<'a, T: Into<Element<'a, U>>, U> ElementExt<'a, U> for T {
    fn erase_element(self) -> Element<'a, U> {
        self.into()
    }
}
