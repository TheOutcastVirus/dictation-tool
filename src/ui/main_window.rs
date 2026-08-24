use super::{history_view::HistoryView, settings_view::SettingsView, status_bar, theme, waveform};
use crate::state::AppState;
use gpui::{
    canvas, div, point, prelude::*, px, Bounds, Context, CursorStyle, Decorations, Div, ElementId,
    Entity, FontWeight, HitboxBehavior, MouseButton, Pixels, Point, ResizeEdge, SharedString, Size,
    Stateful, Window,
};

/// Width of the invisible grab band along the window edges. Kept smaller than
/// the header's padding so it never sits under the window control buttons.
const RESIZE_INSET: Pixels = px(5.);
/// Height of the waveform band under the header.
const BAND_HEIGHT: Pixels = px(58.);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    History,
    Settings,
}

pub struct MainWindow {
    state: Entity<AppState>,
    tab: Tab,
    history: Entity<HistoryView>,
    settings: Entity<SettingsView>,
}

impl MainWindow {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let history = cx.new(|cx| HistoryView::new(state.clone(), cx));
        let settings = cx.new(|cx| SettingsView::new(state.clone(), cx));
        MainWindow {
            state,
            tab: Tab::History,
            history,
            settings,
        }
    }

    /// Tabs are type, not pills: the current one is simply the one set in
    /// full ink. No chip behind it, no marker under it.
    fn tab_button(&self, tab: Tab, label: &'static str, cx: &Context<Self>) -> impl IntoElement {
        let active = self.tab == tab;
        div()
            .id(SharedString::from(label))
            .flex_none()
            .px_2()
            .py_1()
            .cursor_pointer()
            .when(active, |el| el.font_weight(FontWeight::BOLD))
            .text_color(if active { theme::bone() } else { theme::bone_faint() })
            .hover(|s| s.text_color(theme::bone()))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
    }

    /// The app's own titlebar: tabs on the left, a drag region in the middle,
    /// window controls on the right. `client_side` is false if the window
    /// manager insisted on drawing a frame anyway (no compositor), in which
    /// case the drag region and controls are left out.
    fn title_bar(&self, client_side: bool, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(38.))
            .px_2()
            .gap_1()
            .bg(theme::panel())
            .child(self.tab_button(Tab::History, "History", cx))
            .child(self.tab_button(Tab::Settings, "Settings", cx))
            .child(
                div()
                    .id("titlebar-drag")
                    .flex()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .justify_end()
                    .when(client_side, |el| {
                        el.on_mouse_down(MouseButton::Left, |event, window, _| {
                            // Near an edge the root's resize handler takes it.
                            let size = window.window_bounds().get_bounds().size;
                            if resize_edge(event.position, size).is_none() {
                                window.start_window_move();
                            }
                        })
                        .on_click(|event, window, _| {
                            if event.is_right_click() {
                                window.show_window_menu(event.position());
                            }
                        })
                    })
                    // Nothing is written here. With no system titlebar the
                    // name would only repeat what the window already is.
                    .child(div()),
            )
            .when(client_side, |el| {
                el.child(
                    window_button("minimize", "\u{2212}")
                        .on_click(|_, window, _| window.minimize_window()),
                )
                .child(
                    window_button("close", "\u{00d7}")
                        .hover(|s| s.bg(theme::signal()).text_color(theme::ink()))
                        .on_click(|_, window, _| window.remove_window()),
                )
            })
    }
}

impl MainWindow {
    /// The trace, inset from the window edges so it reads as an object
    /// sitting in the page rather than a rule drawn across it, with the
    /// length of what it is showing beside it like a tape counter.
    ///
    /// Before anything has been recorded there is no audio to draw, and a
    /// flat line across an empty strip would be a decoration. The strip
    /// carries the one instruction the app has instead, until the first
    /// utterance replaces it for good.
    fn band(&self, levels: Vec<f32>, trace: waveform::Trace) -> impl IntoElement {
        let nothing_recorded = levels.is_empty() && trace == waveform::Trace::Held;
        let seconds = levels.len() as f32 * crate::audio::LEVEL_INTERVAL_MS as f32 / 1000.0;

        div()
            .flex()
            .flex_none()
            .items_center()
            .h(BAND_HEIGHT)
            .w_full()
            .px_4()
            .gap_3()
            .when(nothing_recorded, |el| {
                el.justify_center().child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::bone_faint())
                        .child("Hold Right Alt to dictate"),
                )
            })
            .when(!nothing_recorded, |el| {
                el.child(
                    div()
                        .flex_1()
                        .h_full()
                        .min_w_0()
                        .child(waveform::render(levels, trace)),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .justify_end()
                        .w(px(52.))
                        .font_family(theme::DATA)
                        .text_size(px(11.))
                        .text_color(theme::bone_faint())
                        .child(format!("{seconds:.1}s")),
                )
            })
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.tab {
            Tab::History => self.history.clone().into_any_element(),
            Tab::Settings => self.settings.clone().into_any_element(),
        };
        let client_side = matches!(window.window_decorations(), Decorations::Client { .. });
        let (levels, trace) = self.state.read(cx).trace();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::ink())
            .font_family(theme::DISPLAY)
            .text_color(theme::bone())
            .text_sm()
            .when(client_side, |el| {
                // Without a WM frame the window needs its own outline and its
                // own resize grips along the edges.
                el.border_1()
                    .border_color(theme::edge())
                    .child(resize_grips())
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        let size = window.window_bounds().get_bounds().size;
                        if let Some(edge) = resize_edge(event.position, size) {
                            window.start_window_resize(edge);
                        }
                    })
            })
            .child(self.title_bar(client_side, cx))
            .child(self.band(levels, trace))
            .child(div().flex_1().min_h_0().child(content))
            .child(status_bar::render(self.state.read(cx)))
    }
}

/// Square icon button for the window controls. Caller attaches `.on_click`.
fn window_button(id: impl Into<ElementId>, glyph: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .size(px(24.))
        .items_center()
        .justify_center()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .text_color(theme::bone_faint())
        .hover(|s| s.bg(theme::lift()).text_color(theme::bone()))
        .child(glyph)
}

/// Transparent full-window overlay that only sets the resize cursor as the
/// pointer crosses an edge; it never swallows clicks.
fn resize_grips() -> impl IntoElement {
    canvas(
        |_, window, _| {
            window.insert_hitbox(
                Bounds::new(point(px(0.), px(0.)), window.window_bounds().get_bounds().size),
                HitboxBehavior::Normal,
            )
        },
        |_, hitbox, window, _| {
            let size = window.window_bounds().get_bounds().size;
            let Some(edge) = resize_edge(window.mouse_position(), size) else {
                return;
            };
            let cursor = match edge {
                ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
                ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
            };
            window.set_cursor_style(cursor, &hitbox);
        },
    )
    .absolute()
    .size_full()
}

/// Which edge (if any) the given window-relative point is grabbing.
fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let (near_top, near_bottom) = (pos.y < RESIZE_INSET, pos.y > size.height - RESIZE_INSET);
    let (near_left, near_right) = (pos.x < RESIZE_INSET, pos.x > size.width - RESIZE_INSET);
    Some(match (near_top, near_bottom, near_left, near_right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}
