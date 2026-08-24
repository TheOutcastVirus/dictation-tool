use super::theme;
use crate::history::DictationEntry;
use crate::state::AppState;
use gpui::{div, prelude::*, px, AnyElement, ClipboardItem, Context, Entity, FontWeight, Window};
use std::time::Duration;

/// Rows are laid out in a plain scroll container, so cap what we render.
const MAX_ROWS: usize = 200;

pub struct HistoryView {
    state: Entity<AppState>,
    copied: Option<usize>,
    /// Delete asks once. The first click arms the row, the second one
    /// removes it; anything else disarms it a few seconds later.
    armed: Option<usize>,
}

impl HistoryView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        HistoryView {
            state,
            copied: None,
            armed: None,
        }
    }

    fn copy(&mut self, index: usize, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some(index);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1600))
                .await;
            this.update(cx, |view, cx| {
                if view.copied == Some(index) {
                    view.copied = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// One dictation: its measurements in the data face, the words
    /// underneath in the reading face. No box around it -- the space
    /// between entries is what separates them.
    fn delete(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.armed != Some(index) {
            self.armed = Some(index);
            cx.notify();
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(3500))
                    .await;
                this.update(cx, |view, cx| {
                    if view.armed == Some(index) {
                        view.armed = None;
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
            return;
        }

        self.armed = None;
        // Indices are positions in the list, so anything remembered about
        // another row is stale the moment one is removed.
        self.copied = None;
        self.state.update(cx, |state, cx| {
            state.history.delete(index);
            cx.notify();
        });
        cx.notify();
    }

    fn row(
        &self,
        index: usize,
        entry: DictationEntry,
        copied: bool,
        armed: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let text = entry.text.clone();
        div()
            .id(("entry", index))
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2p5()
            .rounded_md()
            .hover(|s| s.bg(theme::panel()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .font_family(theme::DATA)
                    .text_size(px(11.))
                    .text_color(theme::bone_faint())
                    .child(time_of(&entry.timestamp))
                    .child(format!(
                        "{:.1}s  {} words  {} ms",
                        entry.audio_duration_s, entry.word_count, entry.transcribe_ms
                    ))
                    .child(div().flex_1())
                    .child(
                        row_button(("copy", index), if copied { "Copied" } else { "Copy" })
                            .when(copied, |el| {
                                el.bg(theme::bone())
                                    .border_color(theme::bone())
                                    .text_color(theme::ink())
                                    .font_weight(FontWeight::BOLD)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy(index, text.clone(), cx);
                            })),
                    )
                    .child(
                        row_button(("delete", index), if armed { "Delete?" } else { "Delete" })
                            .when(armed, |el| {
                                el.border_color(theme::signal())
                                    .text_color(theme::signal())
                                    .font_weight(FontWeight::BOLD)
                            })
                            .when(!armed, |el| {
                                el.hover(|s| s.bg(theme::edge()).text_color(theme::signal()))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete(index, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .line_height(px(24.))
                    .text_color(theme::bone())
                    .child(entry.text),
            )
    }
}

/// Both row controls share one shape and one fixed width, so every row's
/// buttons land on the same two columns whatever their label says.
fn row_button(id: (&'static str, usize), label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(30.))
        .w(px(84.))
        .rounded_md()
        .cursor_pointer()
        .font_family(theme::DISPLAY)
        .text_size(px(13.))
        .bg(theme::lift())
        .border_1()
        .border_color(theme::edge())
        .text_color(theme::bone())
        .hover(|s| s.bg(theme::edge()))
        .child(label)
}

impl HistoryView {
    /// Entries newest first, with the date announced once per day rather
    /// than repeated on every line.
    fn rows(
        &self,
        entries: Vec<DictationEntry>,
        copied: Option<usize>,
        armed: Option<usize>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::with_capacity(entries.len() + 2);
        let mut current_day: Option<String> = None;
        for (i, entry) in entries.into_iter().enumerate() {
            let day = entry.timestamp.get(..10).unwrap_or_default().to_string();
            if current_day.as_deref() != Some(day.as_str()) {
                out.push(day_heading(&day, out.is_empty()).into_any_element());
                current_day = Some(day);
            }
            out.push(
                self.row(i, entry, copied == Some(i), armed == Some(i), cx)
                    .into_any_element(),
            );
        }
        out
    }
}

fn day_heading(day: &str, first: bool) -> impl IntoElement {
    div()
        .px_3()
        .pb_1()
        .when(!first, |el| el.pt_5())
        .font_family(theme::DATA)
        .text_size(px(11.))
        .text_color(theme::bone_faint())
        .child(day.to_string())
}

impl Render for HistoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (total, shown): (usize, Vec<DictationEntry>) = {
            let entries = self.state.read(cx).history.entries();
            (
                entries.len(),
                entries.iter().take(MAX_ROWS).cloned().collect(),
            )
        };
        let (copied, armed) = (self.copied, self.armed);

        div()
            .id("history")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .px_3()
            .pt_3()
            .pb_2()
            .gap_1()
            .when(total == 0, |el| el.child(empty_state()))
            .children(self.rows(shown, copied, armed, cx))
            .when(total > MAX_ROWS, |el| {
                el.child(
                    div()
                        .px_3()
                        .py_3()
                        .font_family(theme::DATA)
                        .text_size(px(11.))
                        .text_color(theme::bone_faint())
                        .child(format!("{MAX_ROWS} of {total} shown")),
                )
            })
    }
}

fn empty_state() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px(17.))
                .text_color(theme::bone_dim())
                .child("Nothing recorded yet."),
        )
}

/// "2026-07-22T18:28:03.923229" -> "18:28". The day is announced once by
/// `day_heading`, so the row only needs the time.
fn time_of(ts: &str) -> String {
    ts.get(11..16).unwrap_or(ts).to_string()
}
