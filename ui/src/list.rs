use iced::{
    Element,
    widget::{self, Column, row},
};
use itertools::Itertools;

use crate::{Message, ModifyShow, Monsoon};

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
                        let el: Element<Message> =
                            row![
                                widget::button(
                                    row![]
                                        .push(image.unwrap_or(widget::image(
                                            &self.live.couldnt_load_image
                                        )))
                                        .push(widget::text(name))
                                ),
                                widget::button("Remove")
                                    .on_press(Message::ModifyShow(id, ModifyShow::RequestRemove))
                            ]
                            .into();
                        el
                    })
                    // FIXME replace with std implementation and remove itertools when it is stabilized
                    .intersperse_with(|| widget::Rule::horizontal(5).into()),
            )
            .into()
    }
}
