use iced_core::Size;

pub trait WithSizeExt: Sized {
    fn exact_size(self, s: Size) -> Self;
    fn max_size(self, s: Size) -> Self {
        self.exact_size(s)
    }
}
mod imp {
    use iced_core::Size;
    use iced_widget::{Button, Container, Image};

    use super::WithSizeExt;

    impl<'a, Message, Theme, Renderer> WithSizeExt for Container<'a, Message, Theme, Renderer>
    where
        Theme: iced_widget::container::Catalog,
        Renderer: iced_core::Renderer,
    {
        fn exact_size(self, s: Size) -> Self {
            self.width(s.width).height(s.height)
        }
        fn max_size(self, s: Size) -> Self {
            self.max_width(s.width).max_height(s.height)
        }
    }
    impl<'a, Message, Theme, Renderer> WithSizeExt for Button<'a, Message, Theme, Renderer>
    where
        Theme: iced_widget::button::Catalog,
        Renderer: iced_core::Renderer,
    {
        fn exact_size(self, s: Size) -> Self {
            self.width(s.width).height(s.height)
        }
    }
    impl<Handle> WithSizeExt for Image<Handle> {
        fn exact_size(self, s: Size) -> Self {
            self.width(s.width).height(s.height)
        }
    }
}
