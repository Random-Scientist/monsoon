use iced_core::{
    Alignment, Element, Layout, Length, Rectangle, Size, Vector, Widget, layout::Node, widget::Tree,
};
#[derive(Debug, Clone, Copy)]
pub enum Axis {
    Horizontal,
    Vertical,
}
impl Axis {
    fn invert(self) -> Self {
        match self {
            Axis::Horizontal => Self::Vertical,
            Axis::Vertical => Self::Horizontal,
        }
    }
}
pub struct Subdivision<'a, Message, Theme, Renderer> {
    elements: Vec<Element<'a, Message, Theme, Renderer>>,
    limits: Vec<f32>,
    size: Size<Length>,
    axis: Axis,
    alignment: Alignment,
}
impl<'a, Message, Theme, Renderer> Subdivision<'a, Message, Theme, Renderer> {
    pub fn from_vec<V: Into<Element<'a, Message, Theme, Renderer>>>(v: Vec<(V, f32)>) -> Self {
        let len = v.len();
        let it = v.into_iter();
        let (mut elements, mut limits) = (Vec::with_capacity(len), Vec::with_capacity(len));
        for (el, limit) in it {
            elements.push(el.into());
            limits.push(limit);
        }
        Self {
            elements,
            limits,
            size: Size {
                width: Length::Fill,
                height: Length::Shrink,
            },
            axis: Axis::Horizontal,
            alignment: Alignment::Center,
        }
    }
}
fn select<T>(l: Size<T>, axis: Axis) -> T {
    match axis {
        Axis::Horizontal => l.width,
        Axis::Vertical => l.height,
    }
}
fn select_vec<T>(axis_dir: T, non_axis_dir: T, axis: Axis) -> Vector<T> {
    match axis {
        Axis::Horizontal => Vector::new(axis_dir, non_axis_dir),
        Axis::Vertical => Vector::new(non_axis_dir, axis_dir),
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Subdivision<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.elements.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.elements);
    }

    fn size(&self) -> Size<Length> {
        self.size
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        #[derive(Clone, Copy)]
        enum Len {
            Factor(u16),
            Pix(f32),
        }
        let portions: Vec<_> = self
            .elements
            .iter()
            .map(|v| match select(v.as_widget().size(), self.axis) {
                v @ (Length::FillPortion(_) | Length::Fill) => Len::Factor(v.fill_factor()),
                Length::Shrink => panic!("widgets in a Subdivision may not be Shrink"),
                Length::Fixed(p) => Len::Pix(p),
            })
            .collect();
        let portions = portions.iter().copied();

        let full_budget = select(limits.max(), self.axis) - select(limits.min(), self.axis);

        let mut factor_budget = full_budget;
        let mut total_factor = 0u16;

        // compute physical size ratios
        for portion in portions.clone() {
            match portion {
                Len::Factor(f) => total_factor += f,
                Len::Pix(p) => factor_budget -= p,
            }
        }

        let mut factor_to_percent: f32 = total_factor.into();
        factor_to_percent = factor_to_percent.recip();

        let mut cursor = 0.0f32;
        let mut children = Vec::with_capacity(self.elements.len());
        let mut largest_non_axis_size = 0.0f32;

        let align_axis = self.axis.invert();

        for (((element, portion), limit), tree) in self
            .elements
            .iter_mut()
            .zip(portions)
            .zip(self.limits.iter().copied())
            .zip(tree.children.iter_mut())
        {
            let axis_size = match portion {
                Len::Factor(f) => {
                    // percentage of the budget that is the limit for this child
                    let limit_percentage = limit / factor_budget;
                    // tentative requested percentage of the budget for this child according to the factor
                    let tentative_percentage = f as f32 * factor_to_percent;

                    (if tentative_percentage > limit_percentage {
                        // correct for the space we weren't able to fill
                        factor_to_percent += tentative_percentage - limit_percentage;

                        limit_percentage
                    } else {
                        tentative_percentage
                    }) * factor_budget
                }
                Len::Pix(p) => p,
            };

            let this_limits = match self.axis {
                Axis::Horizontal => limits.width(axis_size),
                Axis::Vertical => limits.height(axis_size),
            };

            let translation = select_vec(cursor, 0.0, self.axis);
            cursor += axis_size;

            let child = element
                .as_widget_mut()
                .layout(tree, renderer, &this_limits)
                .translate(translation);

            largest_non_axis_size = largest_non_axis_size.max(select(child.size(), align_axis));
            children.push(child);
        }

        let mut size = limits.max();

        if self.alignment != Alignment::Start {
            for node in children.iter_mut() {
                let mut ofs = largest_non_axis_size - select(node.size(), align_axis);
                if self.alignment == Alignment::Center {
                    ofs *= 0.5;
                }
                node.translate_mut(select_vec(ofs, 0.0, align_axis));
            }
        }

        *(match self.axis {
            Axis::Horizontal => &mut size.height,
            Axis::Vertical => &mut size.width,
        }) = largest_non_axis_size;

        Node::with_children(size, children)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: iced_core::Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced_core::widget::Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.elements
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget_mut()
                        .operate(state, layout, renderer, operation);
                });
        });
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        viewport: &iced_core::Rectangle,
    ) {
        self.elements
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .for_each(|((child, state), layout)| {
                child.as_widget_mut().update(
                    state, event, layout, cursor, renderer, clipboard, shell, viewport,
                )
            })
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        viewport: &iced_core::Rectangle,
        renderer: &Renderer,
    ) -> iced_core::mouse::Interaction {
        self.elements
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced_core::renderer::Style,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(clipped_viewport) = layout.bounds().intersection(viewport) {
            for ((child, state), layout) in self
                .elements
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
            {
                child.as_widget().draw(
                    state,
                    renderer,
                    theme,
                    style,
                    layout,
                    cursor,
                    &clipped_viewport,
                );
            }
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced_core::overlay::Element<'b, Message, Theme, Renderer>> {
        iced_core::overlay::from_children(
            &mut self.elements,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Subdivision<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    fn from(
        val: Subdivision<'a, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::<'a, Message, Theme, Renderer>::new(val)
    }
}
