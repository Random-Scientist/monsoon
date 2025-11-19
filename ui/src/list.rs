use std::{fmt::Display, num::NonZero};

use iced::{
    Alignment as A, Element, Font, Length,
    widget::{self, Column, column, row},
};
use itertools::Itertools;

use crate::{
    ElementExt, Message, ModifyShow, Monsoon,
    ext::{Subdivision, WithSizeExt},
    media::PlayRequest,
    style::{UI_SIZES, rounded_box},
};

pub(crate) fn itext<'a, T: iced_widget::text::Catalog + 'a, R: iced_core::text::Renderer + 'a>(
    str: &'static str,
) -> widget::Text<'a, T, R>
where
    <R as iced_core::text::Renderer>::Font: std::convert::From<iced::Font>,
{
    widget::text(str)
        .font(Font {
            family: iced::font::Family::Serif,
            ..Default::default()
        })
        .size(UI_SIZES.info_font_size.get())
        .align_x(A::Center)
}

impl Monsoon {
    #[allow(unstable_name_collisions)]
    pub(crate) fn view_list(&'_ self) -> Element<'_, Message> {
        let bold_serif = Font {
            family: iced::font::Family::Serif,
            weight: iced::font::Weight::Bold,
            ..Default::default()
        };
        let serif = Font {
            family: iced::font::Family::Serif,
            ..Default::default()
        };
        Column::new()
            .extend(
                self.db
                    .shows
                    .enumerate()
                    .map(|(id, s)| {
                        let name: &str = s.get_preferred_name(&self.config);
                        let image = self.thumbnails.get(&id).map(widget::image);

                        let towatch = s.next_episode();

                        let cont = widget::container(
                            row![
                                image
                                    .unwrap_or(widget::image(&self.live.couldnt_load_image))
                                    .content_fit(iced::ContentFit::Contain)
                                    .filter_method(widget::image::FilterMethod::Linear)
                                    .exact_size(UI_SIZES.thumb_image_size.get()),
                                widget::column![
                                    widget::text(name)
                                        .font(bold_serif)
                                        .size(UI_SIZES.title_font_size.get()),
                                    widget::text(format!(
                                        "watched episodes: {}",
                                        towatch
                                            .map(|v| v.0)
                                            .or(s.num_episodes.map(NonZero::get))
                                            .unwrap_or(0)
                                    ))
                                    .font(serif)
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
                                    .font(serif)
                                    .size(UI_SIZES.info_font_size.get()),
                                ]
                            ]
                            .spacing(UI_SIZES.size10.get()),
                        )
                        .style(rounded_box(UI_SIZES.size10.get()))
                        .padding(UI_SIZES.pad20.get());

                        let watch_unwatch = column![
                            widget::button(itext("+"))
                                .on_press_maybe(s.next_episode().map(|(idx, _)| {
                                    Message::ModifyShow(id, ModifyShow::SetWatched(idx, true))
                                }))
                                .exact_size(UI_SIZES.add_sub_button_size.get()),
                            widget::button(itext("-"))
                                .on_press_maybe(
                                    s.watched_episodes.iter().enumerate().rev().find_map(
                                        |(idx, val)| val.then_some(Message::ModifyShow(
                                            id,
                                            ModifyShow::SetWatched(idx as u32, false)
                                        ))
                                    )
                                )
                                .exact_size(UI_SIZES.add_sub_button_size.get())
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
                        let next_ep = widget::button(itext("next episode"))
                            .on_press_maybe(playnext)
                            .style(widget::button::success);
                        let manage = column![
                            widget::button(itext("remove"))
                                .on_press(Message::ModifyShow(id, ModifyShow::RequestRemove)),
                            widget::button(itext("flush cache"))
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
                    .intersperse_with(|| widget::Rule::horizontal(5).into()),
            )
            .into()
    }
}
