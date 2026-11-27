use gpui::{Context, Div, FontWeight, div, prelude::*, px, rgb};

use crate::{
    BORDER, Editor, MONO_FONT, MUTED, PANEL, PANEL as PAPER_PANEL, Panel, Point, Rect, panel_button,
};

pub(super) const KINDS: [(&str, &str); 4] = [
    ("architecture", "Architecture"),
    ("flowchart", "Flowchart"),
    ("sequence", "Sequence"),
    ("swimlane", "Swimlane"),
];

struct Template {
    kind: &'static str,
    name: &'static str,
    description: &'static str,
    ascii: &'static str,
}

static TEMPLATES: [Template; 4] = [
    Template {
        kind: "flowchart",
        name: "Flowchart",
        description: "Steps connected in the order they run.",
        ascii: "[Request] --> [Validate] --> [Respond]",
    },
    Template {
        kind: "sequence",
        name: "Sequence diagram",
        description: "Time flows downward between participants.",
        ascii: concat!(
            "[Client]                 [Service]                 [Store]\n",
            "   |                         |                        |\n",
            "   |------ Send request ---->|                        |\n",
            "   |                         |--------- Read -------->|\n",
            "   |                         |<-------- Data ---------|\n",
            "   |<------- Response -------|                        |\n",
            "   |                         |                        |",
        ),
    },
    Template {
        kind: "swimlane",
        name: "Swimlane",
        description: "Steps grouped by the team that owns them.",
        ascii: concat!(
            "+-- Customer ------------------------+\n",
            "| [Submit] -------------> [Wait]     |\n",
            "+------------------------------------+\n",
            "      |\n",
            "      v\n",
            "+-- Support -------------------------+\n",
            "| [Review] -------------> [Resolve]   |\n",
            "+-------------------------------------+",
        ),
    },
    Template {
        kind: "architecture",
        name: "System architecture",
        description: "Services and a data store inside a boundary.",
        ascii: concat!(
            "                 +-- Private network --------------------------------+\n",
            "                 |                                                    |\n",
            "[Browser] ------>| [Gateway] ------> [Service] ------> [Data store]  |\n",
            "                 |                                                    |\n",
            "                 +----------------------------------------------------+",
        ),
    },
];

#[cfg(test)]
pub(super) fn conversion_templates() -> impl Iterator<Item = (&'static str, &'static str)> {
    TEMPLATES
        .iter()
        .map(|template| (template.kind, template.ascii))
}

impl Editor {
    pub(super) fn templates_panel(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .children(TEMPLATES.iter().enumerate().map(|(index, template)| {
                div()
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PAPER_PANEL))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(template.name))
                            .child(panel_button(
                                ("create-template", index as u64),
                                "New drawing",
                                cx.listener(move |editor, _, window, cx| {
                                    if editor.change_drawings(cx, |drawings| {
                                        drawings.create(template.name).map(|_| ())
                                    }) {
                                        editor.canvas.import(template.ascii);
                                        editor.add_region(
                                            template_bounds(template.ascii),
                                            Some(template.name.into()),
                                            Some(template.kind.into()),
                                        );
                                        editor.panel = None;
                                        editor.focus.focus(window);
                                    }
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(div().text_color(rgb(MUTED)).child(template.description))
                    .child(
                        div()
                            .id(("template-preview", index as u64))
                            .overflow_x_scroll()
                            .child(ascii_preview(template.ascii)),
                    )
            }))
    }

    pub(super) fn region_toolbar(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let region = &self.drawings.active().regions[index];
        let title = region
            .label
            .clone()
            .unwrap_or_else(|| format!("Region {}", index + 1));
        let kind = KINDS
            .iter()
            .find(|(key, _)| region.kind.as_deref() == Some(*key))
            .map_or("flowchart", |(key, _)| *key);
        let [architecture, flowchart, sequence, swimlane] = KINDS.map(|(key, label)| {
            let label = if kind == key {
                format!("{label} ✓")
            } else {
                label.into()
            };
            panel_button(
                key,
                label,
                cx.listener(move |editor, _, _, cx| {
                    editor.drawings.active_mut().regions[index].kind = Some(key.into());
                    editor.mark_drawing_metadata();
                    cx.notify();
                }),
            )
        });
        div()
            .px(px(12.0))
            .py(px(7.0))
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .gap(px(8.0))
            .flex_wrap()
            .flex_shrink_0()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(architecture)
            .child(flowchart)
            .child(panel_button(
                "convert-mermaid",
                "Mermaid…",
                cx.listener(move |editor, _, _, cx| editor.preview_mermaid(index, cx)),
            ))
            .child(panel_button(
                "render-region",
                "Render with AI",
                cx.listener(|editor, _, window, cx| {
                    editor.command(crate::Command::Show(Panel::Ai), window, cx);
                }),
            ))
            .child(sequence)
            .child(swimlane)
            .child(panel_button(
                "remove-region",
                "Remove region",
                cx.listener(|editor, _, _, cx| {
                    editor.remove_region();
                    cx.notify();
                }),
            ))
    }
}

pub(super) fn ascii_preview(text: &str) -> Div {
    div()
        .p(px(10.0))
        .font_family(MONO_FONT)
        .text_size(px(12.0))
        .line_height(px(17.0))
        .whitespace_nowrap()
        .flex()
        .flex_col()
        .children(text.split('\n').map(|row| {
            // Nonbreaking spaces retain cell alignment across GPUI text runs.
            div().min_h(px(17.0)).child(row.replace(' ', "\u{a0}"))
        }))
}

fn template_bounds(ascii: &str) -> Rect {
    let width = ascii
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1);
    let height = ascii.lines().count();
    Rect::new(
        Point::new(0, 0),
        Point::new(
            width.saturating_sub(1) as i32,
            height.saturating_sub(1) as i32,
        ),
    )
}
