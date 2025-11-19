use std::{
    marker::PhantomData,
    ops::{Mul, Range},
};

use crossbeam_utils::atomic::AtomicCell;
use iced::{
    Padding, Size,
    widget::{self, container},
};

const fn pad_vertical(amount: u16) -> Padding {
    let c = amount as f32 * 0.5;

    Padding {
        top: c,
        bottom: c,
        ..Padding::ZERO
    }
}

const fn pad(horiz: u16, vert: u16) -> Padding {
    let [horiz, vert] = [horiz as f32, vert as f32];
    Padding {
        top: vert,
        bottom: vert,
        right: horiz,
        left: horiz,
    }
}

const fn size(horiz: u16, vert: u16) -> Size {
    Size {
        width: horiz as f32,
        height: vert as f32,
    }
}

macro_rules! sizes {
    (
        wrap($wrapper:ident)
        $( #[ $sattr:meta ] )*
        $vis:vis const $constname:ident = struct $sname:ident {
            $(
                $( #[ $fieldattr:meta ] )*
                $name:ident: $ty:ty $( [ $val:expr ] )?
            ),+
            $(,)?
        }
    ) => {
        $( #[$sattr] )*
        $vis struct $sname { $( $( #[$fieldattr] )* $vis $name: $wrapper<$ty>, )+ }

        $( #[$sattr] )*
        $vis const $constname: $sname = const {
            $(
                let $name = $($val)?;
            )+
            $sname { $( $name: $wrapper::new($name) ),+ }
        };
    };
}

sizes! {
    wrap(UIScaledWrapper)
    #[allow(unused)]
    /// Any fixed psuedo-physical sizes for the UI layout. Sizes in pixels at 1x UI scale. Should be multipled by OS DPI scale factor and application ui scale factor
    pub(crate) const UI_SIZES = struct UiSizes {
        add_sub_button_size: Size[size(40, 40)],
        pad5: Padding[pad(5, 5)],
        pad10: Padding[pad(10, 10)],
        pad20: Padding[pad(20, 20)],
        size10: f32[10.0],
        cont_max_width: f32[500.0],
        title_font_size: f32[25.0],
        info_font_size: f32[20.0],
        thumb_image_size: Size[size(150, 214)]
    }
}

sizes! {
    wrap(ChatScaledWrapper)
    #[allow(unused)]
    /// Any fixed psuedo-physical sizes for chat message layout. Sizes in pixels at 1x chat scale. Should be multipled by OS DPI scale factor and application chat message scale factor
    pub(crate) const CHAT_SIZES = struct ChatSizes {
        font_size: f32[16.0],
        member_in_message_pfp_size: Size[size(40, 40)],
    }
}

pub(crate) struct Appearance {
    pub(crate) ui_scale: AtomicCell<f32>,
    pub(crate) chat_scale: AtomicCell<f32>,
    pub(crate) theme: AtomicCell<&'static Theme>,
}

pub(crate) static APPEARANCE: Appearance = Appearance {
    ui_scale: AtomicCell::new(1.0),
    theme: AtomicCell::new(&DEFAULT_THEME),
    chat_scale: AtomicCell::new(1.0),
};

pub static DEFAULT_THEME: Theme = Theme {};
pub struct Theme {}

pub(crate) trait Scale: Sized {
    fn scale(self, factor: f32) -> Self;
}

impl<T: Mul<f32, Output = T> + sealed::ScaleSealed> Scale for T {
    fn scale(self, factor: f32) -> Self {
        self * factor
    }
}

pub trait ScaleCtx {
    fn get() -> f32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ui {}
impl ScaleCtx for Ui {
    fn get() -> f32 {
        APPEARANCE.ui_scale.load()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Chat {}
impl ScaleCtx for Chat {
    fn get() -> f32 {
        APPEARANCE.chat_scale.load()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ScaledWrapper<T, S>(T, PhantomData<fn(S)>);

pub(crate) type UIScaledWrapper<T> = ScaledWrapper<T, Ui>;
pub(crate) type ChatScaledWrapper<T> = ScaledWrapper<T, Chat>;

impl<T: Scale + Clone, U: ScaleCtx> ScaledWrapper<T, U> {
    pub(crate) const fn new(val: T) -> Self {
        Self(val, PhantomData)
    }
    pub(crate) fn get(&self) -> T {
        self.0.clone().scale(U::get())
    }
}

mod sealed {
    use iced::Size;

    pub trait ScaleSealed {}
    macro_rules! scale_sealed {
        (
            $($ty:ty),+
        ) => {
            $( impl ScaleSealed for $ty {} )+
        };
    }
    scale_sealed! { f32, Size }
}
impl Scale for Padding {
    fn scale(self, factor: f32) -> Self {
        Self {
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
            left: self.left * factor,
        }
    }
}
impl Scale for Range<f32> {
    fn scale(self, factor: f32) -> Self {
        self.start * factor..self.end * factor
    }
}

pub fn rounded_box(radius: f32) -> impl Fn(&widget::Theme) -> container::Style {
    move |t| container::Style {
        background: Some(t.extended_palette().background.weak.color.into()),
        border: iced::border::rounded(radius),
        ..Default::default()
    }
}
