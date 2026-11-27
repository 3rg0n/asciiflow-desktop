use std::{io, time::Duration};

use gpui::{Context, Div, FontWeight, IntoElement, Window, div, prelude::*, px, rgb};

use crate::{
    BORDER, Canvas, Drawings, Editor, MUTED, Panel, Rect, Region, image_export, panel_button,
    text_files,
};

impl Editor {
    pub(super) fn flush_drawing(&mut self) -> bool {
        let revision = self.canvas.revision();
        if self.storage_protected || (revision == self.disk_revision && !self.metadata_dirty) {
            return !self.storage_protected;
        }
        if revision != self.disk_revision {
            self.drawings.active_mut().cells = self.canvas.committed_cells();
        }
        self.autosave_attempted = Some(revision);
        match self.drawings.save() {
            Ok(()) => {
                self.disk_revision = revision;
                self.metadata_dirty = false;
                self.storage_error = None;
                true
            }
            Err(error) => {
                self.storage_error = Some(format!("Autosave failed. {error}"));
                false
            }
        }
    }

    pub(super) fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        let revision = self.canvas.revision();
        if self.storage_protected
            || self.autosave_pending
            || (revision == self.disk_revision && !self.metadata_dirty)
            || self.autosave_attempted == Some(revision)
        {
            return;
        }
        self.autosave_pending = true;
        let timer = cx.background_executor().timer(Duration::from_millis(450));
        cx.spawn(async move |editor, cx| {
            timer.await;
            let _ = editor.update(cx, |editor, cx| {
                editor.autosave_pending = false;
                editor.flush_drawing();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn mark_drawing_metadata(&mut self) {
        self.metadata_dirty = true;
        self.autosave_attempted = None;
    }

    pub(super) fn change_drawings(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Drawings) -> io::Result<()>,
    ) -> bool {
        let previous = self.drawings.active_id();
        let mut updated = self.drawings.clone();
        if self.canvas.revision() != self.disk_revision {
            updated.active_mut().cells = self.canvas.committed_cells();
        }
        if let Err(error) = change(&mut updated) {
            self.message = Some(error.to_string());
            return false;
        }
        if !self.storage_protected
            && let Err(error) = updated.save()
        {
            self.storage_error = Some(format!("Could not save the library change. {error}"));
            return false;
        }
        self.drawings = updated;
        let active = self.drawings.active();
        self.drawing_name
            .update(cx, |entry, cx| entry.set(&active.name, cx));
        if active.id != previous {
            let first = active
                .cells
                .iter()
                .map(|(point, _)| *point)
                .min_by_key(|point| (point.y, point.x));
            self.canvas = Canvas::from_cells(active.cells.iter().copied());
            self.active_region = (!active.regions.is_empty()).then_some(0);
            self.drag = crate::Drag::None;
            self.reset_view();
            if let Some(point) = first {
                self.origin =
                    crate::Point::new(point.x.saturating_sub(4), point.y.saturating_sub(2));
                self.cursor = point;
                self.column_start = point.x;
            }
        }
        self.disk_revision = self.canvas.revision();
        self.autosave_attempted = None;
        self.metadata_dirty = self.storage_protected;
        if !self.storage_protected {
            self.storage_error = None;
        }
        cx.notify();
        true
    }

    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.canvas.revision() == self.disk_revision && !self.metadata_dirty
            || self.flush_drawing()
        {
            return true;
        }
        if !self.close_pending {
            self.close_pending = true;
            let answer = window.prompt(
                gpui::PromptLevel::Warning,
                "Close without saving?",
                self.storage_error.as_deref(),
                &["Keep open", "Close without saving"],
                cx,
            );
            cx.spawn_in(window, async move |editor, cx| {
                let discard = answer.await == Ok(1);
                let _ = editor.update_in(cx, |editor, window, cx| {
                    editor.close_pending = false;
                    if discard {
                        window.remove_window();
                    } else {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        false
    }

    pub(super) fn add_region(&mut self, rect: Rect, label: Option<String>, kind: Option<String>) {
        let regions = &mut self.drawings.active_mut().regions;
        self.active_region = Some(regions.len());
        regions.push(Region { rect, label, kind });
        self.mark_drawing_metadata();
    }

    pub(super) fn remove_region(&mut self) {
        if let Some(index) = self.active_region.take() {
            self.drawings.active_mut().regions.remove(index);
            self.mark_drawing_metadata();
        }
    }

    pub(super) fn drawing_panel(&self, cx: &mut Context<Self>) -> Div {
        let active = self.drawings.active_id();
        let active_name = self.drawings.active().name.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Drawings are saved locally as you edit."),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_1().child(self.drawing_name.clone()))
                    .child(panel_button(
                        "rename-drawing",
                        "Rename",
                        cx.listener(|editor, _, _, cx| {
                            let name = editor.drawing_name.read(cx).value().to_owned();
                            editor.change_drawings(cx, |drawings| {
                                drawings.rename(drawings.active_id(), &name)
                            });
                            cx.notify();
                        }),
                    ))
                    .child(panel_button(
                        "duplicate-drawing",
                        "Duplicate",
                        cx.listener(|editor, _, _, cx| {
                            let id = editor.drawings.active_id();
                            let name: String =
                                editor.drawings.active().name.chars().take(110).collect();
                            let name = format!("{name} copy");
                            editor.change_drawings(cx, |drawings| {
                                drawings.duplicate(id, &name).map(|_| ())
                            });
                            cx.notify();
                        }),
                    )),
            )
            .children(self.drawings.iter().map(|drawing| {
                let id = drawing.id;
                let label = if id == active {
                    format!("{}  ·  Open", drawing.name)
                } else {
                    drawing.name.clone()
                };
                panel_button(
                    ("open-drawing", id),
                    label,
                    cx.listener(move |editor, _, window, cx| {
                        if id == editor.drawings.active_id()
                            || editor.change_drawings(cx, |drawings| drawings.open(id))
                        {
                            editor.panel = None;
                            editor.focus.focus(window);
                        }
                        cx.notify();
                    }),
                )
            }))
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .pt(px(8.0))
                    .child(panel_button(
                        "delete-drawing",
                        "Delete current drawing",
                        cx.listener(move |editor, _, window, cx| {
                            editor.confirm_delete(active, &active_name, window, cx);
                        }),
                    )),
            )
    }

    fn confirm_delete(&mut self, id: u64, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let detail = format!("The saved drawing “{name}” will be removed.");
        let prompt = window.prompt(
            gpui::PromptLevel::Warning,
            "Delete drawing?",
            Some(&detail),
            &["Cancel", "Delete"],
            cx,
        );
        cx.spawn(async move |editor, cx| {
            if prompt.await == Ok(1) {
                let _ = editor.update(cx, |editor, cx| {
                    editor.change_drawings(cx, |drawings| drawings.delete(id));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn export_panel(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Export the full canvas, cropped to the drawing."),
            )
            .child(panel_button(
                "export-text",
                "Plain text (.txt)",
                cx.listener(|editor, _, _, cx| editor.save_text(cx)),
            ))
            .child(panel_button(
                "export-svg",
                "Vector image (.svg)",
                cx.listener(|editor, _, _, cx| editor.save_image(false, cx)),
            ))
            .child(panel_button(
                "export-png",
                "Raster image (.png)",
                cx.listener(|editor, _, _, cx| editor.save_image(true, cx)),
            ))
    }

    fn save_image(&mut self, png: bool, cx: &mut Context<Self>) {
        if self.file_pending {
            return;
        }
        let text = self.canvas.export();
        let name = if png { "diagram.png" } else { "diagram.svg" };
        self.export_generated(
            name,
            move || {
                let text = text?;
                if png {
                    image_export::png(&text)
                } else {
                    image_export::svg(&text).map(String::into_bytes)
                }
            },
            cx,
        );
    }

    pub(super) fn export_generated(
        &mut self,
        name: &'static str,
        generate: impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.file_pending {
            return;
        }
        let result = text_files::save_generated(name, generate, cx);
        self.file_pending = true;
        cx.spawn(async move |editor, cx| {
            let result = result.await;
            let _ = editor.update(cx, |editor, cx| {
                editor.file_pending = false;
                match result {
                    Ok(Some(path)) => editor.record_export(&path),
                    Ok(None) => {}
                    Err(error) => editor.message = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn panel_frame(&self, panel: Panel, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, contents) = match panel {
            Panel::Drawings => ("Drawings", self.drawing_panel(cx)),
            Panel::Export => ("Export canvas", self.export_panel(cx)),
            Panel::Ai => ("Diagram rendering", self.ai_panel(cx)),
            Panel::Settings => ("Settings", self.settings_panel(cx)),
            Panel::Templates => ("ASCII templates", self.templates_panel(cx)),
            Panel::Review => ("Review architecture diagram", self.review_panel(cx)),
            Panel::Mermaid => ("Mermaid preview", self.mermaid_panel(cx)),
        };
        div()
            .id("editor-panel")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .m(px(16.0))
            .p(px(18.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(crate::PAPER))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(panel_button(
                        "close-panel",
                        "Back to canvas",
                        cx.listener(|editor, _, window, cx| {
                            editor.panel = None;
                            editor.focus.focus(window);
                            cx.notify();
                        }),
                    )),
            )
            .child(contents)
    }
}
