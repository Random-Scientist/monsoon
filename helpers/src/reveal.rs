use iced_core::{
    Alignment, Element, Layout, Length, Rectangle, Size, Vector, Widget, layout::Node, widget::Tree,
};

pub struct Reveal<'a, Message, Theme, Renderer> {
    preview: Element<'a, Message, Theme, Renderer>,
    content: Element<'a, Message, Theme, Renderer>,
    revealed: bool,
}
impl<'a, Message, Theme, Renderer> Reveal<'a, Message, Theme, Renderer> {
    pub fn new(
        preview: impl Into<Element<'a, Message, Theme, Renderer>>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            preview: preview.into(),
            content: content.into(),
            revealed: false,
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Reveal<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        if self.revealed {
            self.content.as_widget()
        } else {
            self.preview.as_widget()
        }
        .size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        todo!()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced_core::renderer::Style,
        layout: Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        todo!()
    }
}
