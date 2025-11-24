use iced_core::{Alignment, Font, font::Family, widget};

pub mod sizes;
pub mod subdivision;

pub fn itext<'a, T: iced_widget::text::Catalog + 'a, R: iced_core::text::Renderer + 'a>(
    str: &'static str,
    sz: f32,
) -> widget::Text<'a, T, R>
where
    <R as iced_core::text::Renderer>::Font: std::convert::From<iced_core::Font>,
{
    iced_widget::text(str)
        .font(Font {
            family: Family::Serif,
            ..Default::default()
        })
        .size(sz)
        .align_x(Alignment::Center)
}
