use super::theme;
use crate::history::DictationEntry;
use crate::state::AppState;
use gpui::{div, prelude::*, ClipboardItem, Context, Entity, Window};
use std::time::Duration;

/// Rows are laid out in a plain scroll container, so cap what we render.
const MAX_ROWS: usize = 200;

pub struct HistoryView {
    state: Entity<AppState>,
    copied: Option<usize>,
}

impl HistoryView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        HistoryView {
            state,
            copied: None,
        }
    }

    fn copy(&mut self, index: usize, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some(index);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
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

    fn row(&self, index: usize, entry: DictationEntry, copied: bool, cx: &Context<Self>) -> impl IntoElement {
        let text = entry.text.clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded_md()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(format_timestamp(&entry.timestamp))
                    .child(format!(
                        "{:.1}s audio · {} words · {} ms",
                        entry.audio_duration_s, entry.word_count, entry.transcribe_ms
                    ))
                    .child(div().flex_1())
                    .child(
                        theme::button(("copy", index), if copied { "Copied" } else { "Copy" }, copied)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy(index, text.clone(), cx);
                            })),
                    ),
            )
            .child(div().text_sm().text_color(theme::text()).child(entry.text))
    }
}

impl Render for HistoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (total, shown): (usize, Vec<DictationEntry>) = {
            let entries = self.state.read(cx).history.entries();
            (entries.len(), entries.iter().take(MAX_ROWS).cloned().collect())
        };
        let copied = self.copied;

        div()
            .id("history")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .when(total == 0, |el| {
                el.child(
                    div()
                        .flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .text_color(theme::muted())
                        .child("No dictations yet. Hold Right Alt and speak."),
                )
            })
            .children(
                shown
                    .into_iter()
                    .enumerate()
                    .map(|(i, entry)| self.row(i, entry, copied == Some(i), cx)),
            )
            .when(total > MAX_ROWS, |el| {
                el.child(
                    div()
                        .py_2()
                        .text_xs()
                        .text_color(theme::muted())
                        .child(format!("Showing the {MAX_ROWS} most recent of {total} entries.")),
                )
            })
    }
}

/// "2026-07-22T18:28:03.923229" -> "2026-07-22 18:28"
fn format_timestamp(ts: &str) -> String {
    ts.get(..16)
        .map(|s| s.replace('T', " "))
        .unwrap_or_else(|| ts.to_string())
}
