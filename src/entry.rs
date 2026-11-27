use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementInputHandler,
    EntityInputHandler, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, ShapedLine, SharedString, TextRun, UTF16Selection, Window,
    canvas, div, fill, font, point, prelude::*, px, rgb, rgba, size,
};

use crate::{ACCENT, BORDER, INK, MONO_FONT, MUTED, PAPER};

const BULLET: char = '•';

pub(crate) struct Entry {
    text: String,
    placeholder: &'static str,
    secret: bool,
    focus: FocusHandle,
    anchor: usize,
    cursor: usize,
    marked: Option<Range<usize>>,
    dragging: bool,
    layout: Option<EntryLayout>,
}

impl Entry {
    pub(crate) fn new(
        initial: impl Into<String>,
        placeholder: &'static str,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut text = initial.into();
        text.retain(|ch| !ch.is_control());
        let end = text.len();
        Self {
            text,
            placeholder,
            secret,
            focus: cx.focus_handle().tab_stop(true),
            anchor: end,
            cursor: end,
            marked: None,
            dragging: false,
            layout: None,
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.text
    }

    pub(crate) fn set(&mut self, value: &str, cx: &mut Context<Self>) {
        self.text.clear();
        self.text
            .extend(value.chars().filter(|ch| !ch.is_control()));
        self.move_cursor(self.text.len(), false);
        cx.notify();
    }

    pub(crate) fn focus(&self, window: &mut Window) {
        self.focus.focus(window);
    }

    fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    fn move_cursor(&mut self, offset: usize, extend: bool) {
        self.marked = None;
        self.cursor = offset;
        if !extend {
            self.anchor = offset;
        }
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(offset, _)| offset)
    }

    fn next(&self) -> usize {
        self.cursor
            + self.text[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8)
    }

    fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> Range<usize> {
        let range = range
            .map(|range| byte_offset(&self.text, range.start)..byte_offset(&self.text, range.end))
            .or_else(|| self.marked.take())
            .unwrap_or_else(|| self.selection());
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        let inserted = range.start..range.start + text.len();
        self.text.replace_range(range, &text);
        self.move_cursor(inserted.end, false);
        inserted
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        let selection = self.selection();
        if (modifiers.platform || modifiers.control) && !modifiers.alt {
            match key {
                "a" => {
                    self.marked = None;
                    self.anchor = 0;
                    self.cursor = self.text.len();
                }
                "c" | "x" => {
                    if !self.secret && !selection.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.text[selection].to_owned(),
                        ));
                        if key == "x" {
                            self.replace(None, "");
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace(None, &text);
                    }
                }
                _ => return,
            }
        } else {
            let extend = modifiers.shift;
            match key {
                "left" => {
                    let offset = if extend || selection.is_empty() {
                        self.previous()
                    } else {
                        selection.start
                    };
                    self.move_cursor(offset, extend);
                }
                "right" => {
                    let offset = if extend || selection.is_empty() {
                        self.next()
                    } else {
                        selection.end
                    };
                    self.move_cursor(offset, extend);
                }
                "home" => self.move_cursor(0, extend),
                "end" => self.move_cursor(self.text.len(), extend),
                "tab" if extend => window.focus_prev(),
                "tab" => window.focus_next(),
                "backspace" | "delete" => {
                    if selection.is_empty() && self.marked.is_none() {
                        self.anchor = if key == "backspace" {
                            self.previous()
                        } else {
                            self.next()
                        };
                    }
                    self.replace(None, "");
                }
                _ => return,
            }
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn byte_at(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.layout.as_ref() else {
            return self.text.len();
        };
        let index = layout
            .line
            .closest_index_for_x(position.x - layout.origin.x);
        if self.secret && !self.text.is_empty() {
            self.text
                .char_indices()
                .nth(index / BULLET.len_utf8())
                .map_or(self.text.len(), |(offset, _)| offset)
        } else {
            self.text.floor_char_boundary(index.min(self.text.len()))
        }
    }

    fn display_offset(&self, offset: usize) -> usize {
        if self.secret {
            self.text[..offset].chars().count() * BULLET.len_utf8()
        } else {
            offset
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(window);
        self.dragging = true;
        self.move_cursor(self.byte_at(event.position), event.modifiers.shift);
        cx.stop_propagation();
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dragging {
            self.move_cursor(self.byte_at(event.position), true);
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.dragging = false;
    }

    fn prepare_layout(&self, bounds: Bounds<Pixels>, window: &mut Window) -> EntryLayout {
        let (label, color) = if self.text.is_empty() {
            (SharedString::from(self.placeholder), MUTED)
        } else if self.secret {
            (
                std::iter::repeat_n(BULLET, self.text.chars().count())
                    .collect::<String>()
                    .into(),
                INK,
            )
        } else {
            (self.text.clone().into(), INK)
        };
        let run = TextRun {
            len: label.len(),
            font: font(MONO_FONT),
            color: rgb(color).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(label, px(13.0), &[run], None);
        let cursor_x = line.x_for_index(self.display_offset(self.cursor));
        let width = (bounds.size.width - px(2.0)).max(px(0.0));
        let scroll = self
            .layout
            .as_ref()
            .filter(|_| !self.text.is_empty())
            .map_or(px(0.0), |layout| layout.scroll)
            .max(cursor_x - width)
            .min(cursor_x)
            .min((line.width - width).max(px(0.0)));
        EntryLayout {
            line,
            origin: bounds.origin - point(scroll, px(0.0)),
            scroll,
        }
    }

    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        layout: EntryLayout,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let selection = self.selection();
            let focused = self.focus.is_focused(window);
            let highlight = |range: Range<usize>, top, height| {
                let start = layout.x(self.display_offset(range.start));
                let end = layout.x(self.display_offset(range.end));
                Bounds::new(
                    point(start.min(end), top),
                    size((end - start).abs(), height),
                )
            };
            if focused && !selection.is_empty() {
                window.paint_quad(fill(
                    highlight(
                        selection.clone(),
                        bounds.top() + px(4.0),
                        bounds.size.height - px(8.0),
                    ),
                    rgba(0x8eb69b66),
                ));
            }
            let _ = layout
                .line
                .paint(layout.origin, bounds.size.height, window, cx);
            if focused && selection.is_empty() {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            layout.x(self.display_offset(self.cursor)),
                            bounds.top() + px(5.0),
                        ),
                        size(px(1.0), bounds.size.height - px(10.0)),
                    ),
                    rgb(ACCENT),
                ));
            }
            if focused && let Some(marked) = self.marked.clone() {
                window.paint_quad(fill(
                    highlight(marked, bounds.bottom() - px(6.0), px(1.0)),
                    rgb(ACCENT),
                ));
            }
        });
        self.layout = Some(layout);
    }
}

struct EntryLayout {
    line: ShapedLine,
    origin: Point<Pixels>,
    scroll: Pixels,
}

impl EntryLayout {
    fn x(&self, display_offset: usize) -> Pixels {
        self.origin.x + self.line.x_for_index(display_offset)
    }
}

impl Render for Entry {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = cx.entity();
        let input = entry.clone();
        let focus = self.focus.clone();
        div()
            .w_full()
            .min_w_0()
            .h(px(32.0))
            .flex_shrink_0()
            .px(px(8.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(rgb(BORDER))
            .focus(|style| style.border_color(rgb(ACCENT)))
            .bg(rgb(PAPER))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                canvas(
                    move |bounds, window, cx| entry.read(cx).prepare_layout(bounds, window),
                    move |bounds, layout, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, input.clone()),
                            cx,
                        );
                        input.update(cx, |entry, cx| entry.paint(bounds, layout, window, cx));
                    },
                )
                .size_full(),
            )
    }
}

impl EntityInputHandler for Entry {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if self.secret {
            return None;
        }
        let start = byte_offset(&self.text, range.start);
        let end = byte_offset(&self.text, range.end);
        *adjusted = Some(self.utf16_range(start..end));
        Some(self.text[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16_range(self.selection()),
            reversed: self.cursor < self.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.utf16_range(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.is_empty() || text.chars().any(|ch| !ch.is_control()) {
            self.replace(range, text);
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inserted = self.replace(range, text);
        if let Some(selected) = selected {
            let text = &self.text[inserted.clone()];
            self.anchor = inserted.start + byte_offset(text, selected.start);
            self.cursor = inserted.start + byte_offset(text, selected.end);
        }
        self.marked = (!inserted.is_empty()).then_some(inserted);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let start = layout.x(self.display_offset(byte_offset(&self.text, range.start)));
        let end = layout.x(self.display_offset(byte_offset(&self.text, range.end)));
        Some(Bounds::new(
            point(start.min(end), bounds.top()),
            size((end - start).abs().max(px(1.0)), bounds.size.height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.text[..self.byte_at(position)].encode_utf16().count())
    }
}

impl Entry {
    fn utf16_range(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }
}

fn byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    for (offset, ch) in text.char_indices() {
        if units >= utf16_offset {
            return offset;
        }
        units += ch.len_utf16();
    }
    text.len()
}
