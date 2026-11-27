use std::ops::Range;

use gpui::{
    Bounds, Context, EntityInputHandler, Pixels, Point as UiPoint, UTF16Selection, Window, point,
    px, size,
};

use crate::{Editor, Point, Tool};

#[derive(Default)]
pub(crate) struct Composition {
    text: String,
    selection: Range<usize>,
}

impl Composition {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn character_at(&self, start: Point, point: Point) -> Option<char> {
        if start.y != point.y || point.x < start.x || self.text.is_empty() {
            return None;
        }
        self.text
            .chars()
            .nth((i64::from(point.x) - i64::from(start.x)) as usize)
    }

    pub(crate) fn caret_at(&self, start: Point) -> Point {
        Point::new(
            start
                .x
                .saturating_add(cells_before(&self.text, self.selection.end) as i32),
            start.y,
        )
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text: Vec<_> = self.composition.text.encode_utf16().collect();
        let start = range.start.min(text.len());
        let end = range.end.clamp(start, text.len());
        *adjusted_range = Some(start..end);
        String::from_utf16(&text[start..end]).ok()
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        (self.tool == Tool::Text).then(|| UTF16Selection {
            range: self.composition.selection.clone(),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let length = self.composition.text.encode_utf16().count();
        (length > 0).then_some(0..length)
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let text = std::mem::take(&mut self.composition.text);
        self.composition.clear();
        self.commit_input(&text, cx);
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = replace(&self.composition.text, range, text);
        self.composition.clear();
        self.commit_input(&text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tool == Tool::Text {
            self.composition.text = replace(&self.composition.text, range, text);
            let length = self.composition.text.encode_utf16().count();
            self.composition.selection = selection.unwrap_or(length..length);
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.tool != Tool::Text {
            return None;
        }
        let (width, height) = self.cell_size();
        let start = cells_before(&self.composition.text, range.start);
        let end = cells_before(&self.composition.text, range.end);
        Some(Bounds::new(
            bounds.origin
                + point(
                    px(
                        ((i64::from(self.cursor.x) - i64::from(self.origin.x)) as f32
                            + start as f32)
                            * width,
                    ),
                    px((i64::from(self.cursor.y) - i64::from(self.origin.y)) as f32 * height),
                ),
            size(
                px((end.saturating_sub(start) as f32 * width).max(1.0)),
                px(height),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: UiPoint<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if self.tool != Tool::Text || !self.canvas_bounds.get().contains(&position) {
            return None;
        }
        let cell = self.cell_at(position);
        if cell.y != self.cursor.y || cell.x < self.cursor.x {
            return None;
        }
        Some(
            self.composition
                .text
                .chars()
                .take((i64::from(cell.x) - i64::from(self.cursor.x)) as usize)
                .map(char::len_utf16)
                .sum(),
        )
    }
}

impl Editor {
    fn commit_input(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.tool == Tool::Text && !text.is_empty() && !text.chars().any(char::is_control) {
            self.canvas.insert_text(self.cursor, text);
            self.move_cursor(Point::new(
                self.cursor.x.saturating_add(text.chars().count() as i32),
                self.cursor.y,
            ));
            self.message = None;
            self.hover = None;
        }
        cx.notify();
    }
}

fn replace(previous: &str, range: Option<Range<usize>>, replacement: &str) -> String {
    let Some(range) = range else {
        return replacement.to_owned();
    };
    let mut text: Vec<_> = previous.encode_utf16().collect();
    let start = range.start.min(text.len());
    let end = range.end.clamp(start, text.len());
    text.splice(start..end, replacement.encode_utf16());
    String::from_utf16_lossy(&text)
}

fn cells_before(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    text.chars()
        .take_while(|ch| {
            units += ch.len_utf16();
            units <= utf16_offset
        })
        .count()
}
