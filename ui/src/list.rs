use iced::{
    Element,
    widget::{self, Column, row},
};
use itertools::Itertools;

use crate::{Message, Monsoon};

impl Monsoon {
    #[allow(unstable_name_collisions)]
    pub(crate) fn view_list(&'_ self) -> Element<'_, Message> {
        Column::new()
            .extend(
                self.db
                    .shows
                    .enumerate()
                    .map(|(id, s)| {
                        let name: &str = s
                            .names
                            .names
                            .iter()
                            .find(|v| v.0 == self.config.preferred_name_kind)
                            .map(|v| &*v.1)
                            .unwrap_or("");
                        let image = self.thumbnails.get(&id).map(widget::image);
                        let el: Element<Message> = row![
                            widget::button(row![].push_maybe(image).push(widget::text(name))),
                            widget::button("Remove").on_press(Message::RequestRemove(id))
                        ]
                        .into();
                        el
                    })
                    // TODO replace with std implementation
                    .intersperse_with(|| widget::Rule::horizontal(5).into()),
            )
            .into()
    }
}
