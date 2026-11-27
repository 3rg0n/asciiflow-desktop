use gpui::{ClipboardItem, Context, Div, FontWeight, div, prelude::*, px, rgb};

use crate::{BORDER, Editor, MUTED, PANEL, Panel, mermaid, panel_button, templates::ascii_preview};

pub(super) struct MermaidReview {
    title: String,
    source: String,
    output: Result<String, String>,
}

impl Editor {
    pub(super) fn preview_mermaid(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(region) = self.drawings.active().regions.get(index) else {
            return;
        };
        let source = match self.canvas.region_text(region.rect) {
            Ok(text) => text,
            Err(error) => {
                self.message = Some(error);
                cx.notify();
                return;
            }
        };
        let kind = region.kind.as_deref().unwrap_or("flowchart");
        let title = region
            .label
            .clone()
            .unwrap_or_else(|| format!("Region {}", index + 1));
        let output = if source.len() > 64 * 1024 {
            Err("Choose a Mermaid region smaller than 64 KiB.".into())
        } else {
            mermaid::convert(&source, kind)
        };
        self.mermaid_review = Some(MermaidReview {
            title,
            source,
            output,
        });
        self.panel = Some(Panel::Mermaid);
        cx.notify();
    }

    pub(super) fn mermaid_panel(&self, cx: &mut Context<Self>) -> Div {
        let Some(review) = &self.mermaid_review else {
            return div().child("Create and select a region on the canvas first.");
        };
        let content = match &review.output {
            Ok(source) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Mermaid source"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(panel_button(
                            "copy-mermaid",
                            "Copy Mermaid",
                            cx.listener(|editor, _, _, cx| {
                                if let Some(Ok(text)) =
                                    editor.mermaid_review.as_ref().map(|review| &review.output)
                                {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                    editor.message = Some("Mermaid source copied".into());
                                    cx.notify();
                                }
                            }),
                        ))
                        .child(panel_button(
                            "save-mermaid",
                            "Export .mmd",
                            cx.listener(|editor, _, _, cx| {
                                if let Some(Ok(text)) =
                                    editor.mermaid_review.as_ref().map(|review| &review.output)
                                {
                                    let text = text.clone();
                                    editor.export_generated(
                                        "diagram.mmd",
                                        move || Ok(text.into_bytes()),
                                        cx,
                                    );
                                }
                            }),
                        )),
                )
                .child(
                    div()
                        .id("mermaid-code")
                        .max_h(px(300.0))
                        .overflow_scroll()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(PANEL))
                        .child(ascii_preview(source)),
                ),
            Err(error) => {
                div()
                    .p(px(12.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("This region needs a clearer structure"),
                    )
                    .child(div().text_color(rgb(MUTED)).child(error.clone()))
                    .child(div().text_color(rgb(MUTED)).child(
                        "Open Templates for an editable example of the selected diagram type.",
                    ))
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(format!("Source cells in {}", review.title)),
            )
            .child(
                div()
                    .id("mermaid-input")
                    .max_h(px(205.0))
                    .overflow_scroll()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PANEL))
                    .child(ascii_preview(&review.source)),
            )
            .child(content)
    }
}
