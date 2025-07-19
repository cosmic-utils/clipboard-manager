use cosmic::{
    iced::{self, keyboard, touch},
    iced_core::{Size, Vector, keyboard::key::Named, widget::tree},
    iced_widget,
};
use iced_widget::core::{
    self, Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Widget, event,
    layout::{Limits, Node},
    mouse::{self, Cursor},
    overlay, renderer,
    widget::{Operation, Tree},
};

use cosmic::iced::{event::Status, window};

#[allow(missing_debug_implementations)]
pub struct ContextMenu<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    /// The underlying element.
    underlay: Element<'a, Message, Theme, Renderer>,
    /// The content of [`ContextMenuOverlay`].
    overlay: Element<'a, Message, Theme, Renderer>,
}

// pub fn context_menu() -> ContextMenu<> {

// }

impl<'a, Message, Theme, Renderer> ContextMenu<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    /// Creates a new [`ContextMenu`]
    ///
    /// `underlay`: The underlying element.
    ///
    /// `overlay`: The content of [`ContextMenuOverlay`] which will be displayed when `underlay` is clicked.
    pub fn new(
        underlay: impl Into<Element<'a, Message, Theme, Renderer>>,
        overlay: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        ContextMenu {
            underlay: underlay.into(),
            overlay: overlay.into(),
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ContextMenu<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn size(&self) -> iced::Size<Length> {
        self.underlay.as_widget().size()
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.underlay
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        state: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        self.underlay.as_widget().draw(
            &state.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.underlay), Tree::new(&self.overlay)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.underlay, &mut self.overlay]);
    }

    fn operate<'b>(
        &'b self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<()>,
    ) {
        let state: &mut State = tree.state.downcast_mut();

        if state.show {
            self.overlay
                .as_widget()
                .operate(&mut tree.children[1], layout, renderer, operation);
        } else {
            self.underlay
                .as_widget()
                .operate(&mut tree.children[0], layout, renderer, operation);
        }
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        if event == Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) {
            let bounds = layout.bounds();

            if cursor.is_over(bounds) {
                let state: &mut State = tree.state.downcast_mut();
                state.cursor_position = cursor.position().unwrap_or_default();
                state.show = !state.show;
                return event::Status::Captured;
            }
        }

        self.underlay.as_widget_mut().on_event(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.underlay.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state: &mut State = tree.state.downcast_mut();

        if !state.show {
            return self.underlay.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                translation,
            );
        }

        let position = state.cursor_position;
        self.overlay.as_widget_mut().diff(&mut tree.children[1]);

        Some(context_menu_overlay(
            position + translation,
            &mut tree.children[1],
            &mut self.overlay,
            state,
        ))

        // Some(
        //     ContextMenuOverlay::new(
        //         position + translation,
        //         &mut tree.children[1],
        //         &mut self.overlay,
        //         state,
        //     )
        //     .overlay(),
        // )
    }
}

impl<'a, Message, Theme, Renderer> From<ContextMenu<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(modal: ContextMenu<'a, Message, Theme, Renderer>) -> Self {
        Element::new(modal)
    }
}

/// The state of the ``context_menu``.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The visibility of the [`ContextMenu`] overlay.
    pub show: bool,
    /// Use for showing the overlay where the click was made.
    pub cursor_position: Point,
}

impl State {
    /// Creates a new [`State`] containing the given state data.
    pub const fn new() -> Self {
        Self {
            show: false,
            cursor_position: Point::ORIGIN,
        }
    }
}

pub fn context_menu_overlay<'a, Message, Theme, Renderer>(
    position: Point,
    tree: &'a mut Tree,
    content: &'a mut Element<'a, Message, Theme, Renderer>,
    state: &'a mut State,
) -> overlay::Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    ContextMenuOverlay::new(
        position,
        &mut tree.children[1],
        content,
        state,
    )
    .overlay()
}

struct ContextMenuOverlay<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    // The position of the element
    position: Point,
    /// The state of the [`ContextMenuOverlay`].
    tree: &'a mut Tree,
    /// The content of the [`ContextMenuOverlay`].
    content: &'a mut Element<'a, Message, Theme, Renderer>,
    /// The state shared between [`ContextMenu`](crate::widget::ContextMenu) and [`ContextMenuOverlay`].
    state: &'a mut State,
}

impl<'a, Message, Theme, Renderer> ContextMenuOverlay<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    /// Creates a new [`ContextMenuOverlay`].
    fn new(
        position: Point,
        tree: &'a mut Tree,
        content: &'a mut Element<'a, Message, Theme, Renderer>,
        state: &'a mut State,
    ) -> Self {
        ContextMenuOverlay {
            position,
            tree,
            content,
            state,
        }
    }

    /// Turn this [`ContextMenuOverlay`] into an overlay [`Element`](overlay::Element).
    #[must_use]
    fn overlay(self) -> overlay::Element<'a, Message, Theme, Renderer> {
        overlay::Element::new(Box::new(self))
    }
}

impl<'a, Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for ContextMenuOverlay<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
        let limits = Limits::new(Size::ZERO, bounds);
        let max_size = limits.max();

        let mut content = self
            .content
            .as_widget()
            .layout(self.tree, renderer, &limits);

        // Try to stay inside borders
        let mut position = self.position;
        if position.x + content.size().width > bounds.width {
            position.x = f32::max(0.0, position.x - content.size().width);
        }
        if position.y + content.size().height > bounds.height {
            position.y = f32::max(0.0, position.y - content.size().height);
        }

        content.move_to_mut(position);

        Node::with_children(max_size, vec![content])
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
    ) {
        let content_layout = layout
            .children()
            .next()
            .expect("widget: Layout should have a content layout.");

        let bounds = layout.bounds();

        // Modal
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            content_layout,
            cursor,
            &bounds,
        );
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<Message>,
    ) -> Status {
        let layout_children = layout
            .children()
            .next()
            .expect("widget: Layout should have a content layout.");

        let mut forward_event_to_children = true;

        let status = match &event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if *key == keyboard::Key::Named(keyboard::key::Named::Escape) {
                    self.state.show = false;
                    forward_event_to_children = false;
                    Status::Captured
                } else {
                    Status::Ignored
                }
            }

            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left | mouse::Button::Right,
            ))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if !cursor.is_over(layout_children.bounds()) {
                    self.state.show = false;
                    forward_event_to_children = false;
                }
                Status::Captured
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // close when released because because button send message on release
                self.state.show = false;
                Status::Captured
            }

            Event::Window(window::Event::Resized { .. }) => {
                self.state.show = false;
                forward_event_to_children = false;
                Status::Captured
            }

            _ => Status::Ignored,
        };

        let child_status = if forward_event_to_children {
            self.content.as_widget_mut().on_event(
                self.tree,
                event,
                layout_children,
                cursor,
                renderer,
                clipboard,
                shell,
                &layout.bounds(),
            )
        } else {
            Status::Ignored
        };

        match child_status {
            Status::Ignored => status,
            Status::Captured => Status::Captured,
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout
                .children()
                .next()
                .expect("widget: Layout should have a content layout."),
            cursor,
            viewport,
            renderer,
        )
    }
}
