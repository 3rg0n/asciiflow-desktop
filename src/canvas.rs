use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

pub(crate) const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
const HISTORY_EDITS: usize = 256;
const HISTORY_CELLS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn translated(self, dx: i64, dy: i64) -> Self {
        fn offset(value: i32, delta: i64) -> i32 {
            (i64::from(value) + delta).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        }

        Self::new(offset(self.x, dx), offset(self.y, dy))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn new(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn contains(self, point: Point) -> bool {
        (self.min.x..=self.max.x).contains(&point.x) && (self.min.y..=self.max.y).contains(&point.y)
    }

    fn translated(self, dx: i64, dy: i64) -> Self {
        Self::new(self.min.translated(dx, dy), self.max.translated(dx, dy))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tool {
    Select,
    Draw,
    Line,
    Arrow,
    Box,
    Text,
    Erase,
    Region,
}

type Cells = BTreeMap<Point, char>;

struct Change {
    point: Point,
    before: char,
    after: char,
}

struct Gesture {
    kind: GestureKind,
    start: Point,
    end: Point,
    // A space erases a committed cell. Missing entries leave it alone.
    preview: Cells,
}

enum GestureKind {
    Stroke(Tool),
    Shape(Tool),
    Marquee,
    Move { rect: Rect, cells: Cells },
}

#[derive(Default)]
pub struct Canvas {
    cells: Cells,
    gesture: Option<Gesture>,
    selection: Option<Rect>,
    undo: VecDeque<Vec<Change>>,
    redo: Vec<Vec<Change>>,
    revision: u64,
}

impl Canvas {
    pub fn from_cells(cells: impl IntoIterator<Item = (Point, char)>) -> Self {
        Self {
            cells: cells.into_iter().filter(|(_, ch)| *ch != ' ').collect(),
            ..Self::default()
        }
    }

    pub fn committed_cells(&self) -> Vec<(Point, char)> {
        self.cells.iter().map(|(&point, &ch)| (point, ch)).collect()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn region_text(&self, rect: Rect) -> Result<String, String> {
        let width = i64::from(rect.max.x) - i64::from(rect.min.x) + 1;
        let height = i64::from(rect.max.y) - i64::from(rect.min.y) + 1;
        if width < 1 || height < 1 || width.saturating_mul(height) > 250_000 {
            return Err("Select a smaller diagram region.".into());
        }
        render(
            self.cells
                .iter()
                .filter_map(|(&point, &ch)| rect.contains(point).then_some((point, ch))),
            rect.min,
        )
    }

    pub fn get(&self, point: Point) -> char {
        let preview = self
            .gesture
            .as_ref()
            .and_then(|gesture| gesture.preview.get(&point));
        preview.copied().unwrap_or_else(|| cell(&self.cells, point))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Point, char)> + '_ {
        let preview = self.gesture.as_ref().map(|gesture| &gesture.preview);
        self.cells
            .iter()
            .filter(move |(point, _)| preview.is_none_or(|cells| !cells.contains_key(point)))
            .chain(preview.into_iter().flatten().filter(|(_, ch)| **ch != ' '))
            .map(|(&point, &ch)| (point, ch))
    }

    pub fn begin(&mut self, tool: Tool, point: Point) {
        self.cancel();
        let kind = match tool {
            Tool::Select => match self.selection.filter(|rect| rect.contains(point)) {
                Some(rect) => GestureKind::Move {
                    rect,
                    cells: self.selected_cells(rect),
                },
                None => GestureKind::Marquee,
            },
            Tool::Draw | Tool::Erase => GestureKind::Stroke(tool),
            Tool::Line | Tool::Arrow | Tool::Box => GestureKind::Shape(tool),
            Tool::Text | Tool::Region => {
                self.selection = None;
                return;
            }
        };
        let mut preview = Cells::new();
        match kind {
            GestureKind::Stroke(Tool::Draw) => {
                preview.insert(point, '+');
            }
            GestureKind::Stroke(Tool::Erase) => {
                preview.insert(point, ' ');
            }
            GestureKind::Shape(tool) => draw_shape(&self.cells, &mut preview, tool, point, point),
            _ => {}
        }
        self.gesture = Some(Gesture {
            kind,
            start: point,
            end: point,
            preview,
        });
    }

    pub fn update(&mut self, point: Point) {
        let Some(gesture) = &mut self.gesture else {
            return;
        };
        if point == gesture.end {
            return;
        }
        let preview = &mut gesture.preview;
        match &gesture.kind {
            GestureKind::Stroke(Tool::Draw) => {
                if gesture.end == gesture.start && preview.len() == 1 {
                    preview.clear();
                }
                draw_path(&self.cells, preview, gesture.end, point);
            }
            GestureKind::Stroke(Tool::Erase) => {
                visit_path(gesture.end, point, |position, _| {
                    preview.insert(position, ' ');
                });
            }
            GestureKind::Shape(tool) => {
                preview.clear();
                draw_shape(&self.cells, preview, *tool, gesture.start, point);
            }
            GestureKind::Move { rect, cells } => {
                preview.clear();
                let (dx, dy) = selection_delta(*rect, gesture.start, point);
                for &position in cells.keys() {
                    preview.insert(position, ' ');
                }
                for (&position, &ch) in cells {
                    preview.insert(position.translated(dx, dy), ch);
                }
            }
            GestureKind::Marquee => {}
            GestureKind::Stroke(_) => unreachable!("only draw and erase create strokes"),
        }
        gesture.end = point;
    }

    pub fn end(&mut self, point: Point) {
        self.update(point);
        let selection = self.selection();
        if let Some(gesture) = self.gesture.take() {
            self.commit(gesture.preview);
            self.selection = selection;
        }
    }

    pub fn cancel(&mut self) {
        self.gesture = None;
    }

    pub fn insert_text(&mut self, start: Point, text: &str) {
        self.cancel();
        self.selection = None;
        self.commit(text_cells(start, text));
    }

    pub fn import(&mut self, text: &str) {
        self.cancel();
        self.selection = None;
        let mut edits = self
            .cells
            .keys()
            .map(|&point| (point, ' '))
            .collect::<Cells>();
        edits.extend(text_cells(Point::new(0, 0), text));
        self.commit(edits);
    }

    pub fn export(&self) -> Result<String, String> {
        match bounds(self.iter().map(|(point, _)| point)) {
            Some(rect) => render(self.iter(), rect.min),
            None => Ok(String::new()),
        }
    }

    pub fn selection(&self) -> Option<Rect> {
        match &self.gesture {
            Some(Gesture {
                kind: GestureKind::Marquee,
                start,
                end,
                ..
            }) => Some(Rect::new(*start, *end)),
            Some(Gesture {
                kind: GestureKind::Move { rect, .. },
                start,
                end,
                ..
            }) => {
                let (dx, dy) = selection_delta(*rect, *start, *end);
                Some(rect.translated(dx, dy))
            }
            Some(_) => None,
            None => self.selection,
        }
    }

    pub fn select_all(&mut self) {
        self.cancel();
        self.selection = bounds(self.cells.keys().copied());
    }

    pub fn clear_selection(&mut self) {
        self.cancel();
        self.selection = None;
    }

    pub fn copy_selection(&self) -> Result<Option<String>, String> {
        self.selection()
            .map(|rect| {
                render(
                    self.iter().filter(|(point, _)| rect.contains(*point)),
                    rect.min,
                )
            })
            .transpose()
    }

    pub fn cut_selection(&mut self) -> Result<Option<String>, String> {
        self.cancel();
        let text = self.copy_selection()?;
        self.delete_selection();
        Ok(text)
    }

    pub fn delete_selection(&mut self) -> bool {
        self.cancel();
        let Some(rect) = self.selection.take() else {
            return false;
        };
        self.commit(
            self.selected_cells(rect)
                .into_keys()
                .map(|point| (point, ' '))
                .collect(),
        )
    }

    pub fn has_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn has_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        self.cancel();
        let Some(changes) = self.undo.pop_back() else {
            return false;
        };
        for change in &changes {
            put(&mut self.cells, change.point, change.before);
        }
        self.selection = None;
        self.redo.push(changes);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.cancel();
        let Some(changes) = self.redo.pop() else {
            return false;
        };
        for change in &changes {
            put(&mut self.cells, change.point, change.after);
        }
        self.selection = None;
        self.undo.push_back(changes);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn commit(&mut self, edits: Cells) -> bool {
        let mut changes = Vec::new();
        for (point, after) in edits {
            let before = cell(&self.cells, point);
            if before != after {
                put(&mut self.cells, point, after);
                changes.push(Change {
                    point,
                    before,
                    after,
                });
            }
        }
        if changes.is_empty() {
            return false;
        }
        self.redo.clear();
        self.undo.push_back(changes);
        let mut retained: usize = self.undo.iter().map(Vec::len).sum();
        while self.undo.len() > HISTORY_EDITS || retained > HISTORY_CELLS {
            retained -= self
                .undo
                .pop_front()
                .expect("history is over its limit")
                .len();
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn selected_cells(&self, rect: Rect) -> Cells {
        self.cells
            .iter()
            .filter(|(point, _)| rect.contains(**point))
            .map(|(&point, &ch)| (point, ch))
            .collect()
    }
}

fn cell(cells: &Cells, point: Point) -> char {
    cells.get(&point).copied().unwrap_or(' ')
}

fn put(cells: &mut Cells, point: Point, ch: char) {
    if ch == ' ' {
        cells.remove(&point);
    } else {
        cells.insert(point, ch);
    }
}

fn bounds(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
    let mut points = points.into_iter();
    let first = points.next()?;
    Some(points.fold(Rect::new(first, first), |rect, point| Rect {
        min: Point::new(rect.min.x.min(point.x), rect.min.y.min(point.y)),
        max: Point::new(rect.max.x.max(point.x), rect.max.y.max(point.y)),
    }))
}

fn selection_delta(rect: Rect, start: Point, end: Point) -> (i64, i64) {
    fn clamp_axis(min: i32, max: i32, start: i32, end: i32) -> i64 {
        (i64::from(end) - i64::from(start)).clamp(
            i64::from(i32::MIN) - i64::from(min),
            i64::from(i32::MAX) - i64::from(max),
        )
    }

    (
        clamp_axis(rect.min.x, rect.max.x, start.x, end.x),
        clamp_axis(rect.min.y, rect.max.y, start.y, end.y),
    )
}

fn text_cells(start: Point, text: &str) -> Cells {
    let mut cells = Cells::new();
    let mut chars = text.chars().peekable();
    let (mut dx, mut dy) = (0, 0);
    while let Some(ch) = chars.next() {
        match ch {
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                dx = 0;
                dy += 1;
            }
            '\t' => {
                let tab_end = dx + 4 - dx % 4;
                for column in dx..tab_end {
                    cells.insert(start.translated(column, dy), ' ');
                }
                dx = tab_end;
            }
            ch if !ch.is_control() => {
                cells.insert(start.translated(dx, dy), ch);
                dx += 1;
            }
            _ => {}
        }
    }
    cells
}

fn render(cells: impl IntoIterator<Item = (Point, char)>, origin: Point) -> Result<String, String> {
    let mut positions: Vec<_> = cells.into_iter().collect();
    positions.sort_unstable_by_key(|(point, _)| (point.y, point.x));
    let mut output = String::new();
    let mut row = origin.y;
    let mut column = i64::from(origin.x);
    for (point, ch) in positions {
        let row_gap = (i64::from(point.y) - i64::from(row)) as usize;
        if row_gap != 0 {
            column = i64::from(origin.x);
        }
        let column_gap = (i64::from(point.x) - column) as usize;
        if output
            .len()
            .saturating_add(row_gap)
            .saturating_add(column_gap)
            .saturating_add(ch.len_utf8())
            > MAX_TEXT_BYTES
        {
            return Err(
                "The diagram exceeds the 2 MiB text limit. Copy a smaller selection.".into(),
            );
        }
        if point.y != row {
            output.extend(std::iter::repeat_n('\n', row_gap));
            row = point.y;
        }
        output.extend(std::iter::repeat_n(' ', column_gap));
        output.push(ch);
        column = i64::from(point.x) + 1;
    }
    Ok(output)
}

fn visit_segment(start: Point, end: Point, mut visit: impl FnMut(Point, char)) {
    if start.y == end.y {
        for x in start.x.min(end.x)..=start.x.max(end.x) {
            visit(Point::new(x, start.y), '-');
        }
    } else {
        for y in start.y.min(end.y)..=start.y.max(end.y) {
            visit(Point::new(start.x, y), '|');
        }
    }
}

fn visit_path(start: Point, end: Point, mut visit: impl FnMut(Point, char)) {
    if start.x == end.x || start.y == end.y {
        visit_segment(start, end, visit);
    } else {
        let corner = Point::new(end.x, start.y);
        visit_segment(start, corner, &mut visit);
        visit_segment(corner, end, visit);
    }
}

fn draw_path(cells: &Cells, preview: &mut Cells, start: Point, end: Point) {
    visit_path(start, end, |point, ch| draw_cell(cells, preview, point, ch));
}

fn draw_cell(cells: &Cells, preview: &mut Cells, point: Point, ch: char) {
    let old = preview
        .get(&point)
        .copied()
        .unwrap_or_else(|| cell(cells, point));
    let glyph = match (old, ch) {
        ('+', _) | ('-', '|') | ('|', '-') => '+',
        _ => ch,
    };
    preview.insert(point, glyph);
}

fn draw_shape(cells: &Cells, preview: &mut Cells, tool: Tool, start: Point, end: Point) {
    if tool == Tool::Box && start.x != end.x && start.y != end.y {
        let rect = Rect::new(start, end);
        let top_right = Point::new(rect.max.x, rect.min.y);
        let bottom_left = Point::new(rect.min.x, rect.max.y);
        for (from, to) in [
            (rect.min, top_right),
            (top_right, rect.max),
            (rect.max, bottom_left),
            (bottom_left, rect.min),
        ] {
            visit_segment(from, to, |point, ch| draw_cell(cells, preview, point, ch));
        }
        return;
    }

    draw_path(cells, preview, start, end);
    if tool == Tool::Arrow {
        let head = match end.y.cmp(&start.y) {
            std::cmp::Ordering::Less => '^',
            std::cmp::Ordering::Greater => 'v',
            std::cmp::Ordering::Equal if end.x < start.x => '<',
            std::cmp::Ordering::Equal => '>',
        };
        preview.insert(end, head);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_excludes_surrounding_cells_and_copies_only_committed_text() {
        let mut canvas = Canvas::default();
        canvas.insert_text(Point::new(0, 0), "X [Source] --> [Target] X");
        let region = Rect::new(Point::new(2, 0), Point::new(22, 0));
        canvas.begin(Tool::Erase, Point::new(2, 0));
        assert_eq!(
            canvas.region_text(region).unwrap().trim_end(),
            "[Source] --> [Target]"
        );
    }

    #[test]
    fn distant_cells_do_not_allocate_unbounded_text() {
        let canvas = Canvas::from_cells([
            (Point::new(i32::MIN, 0), 'a'),
            (Point::new(i32::MAX, 0), 'b'),
        ]);
        assert!(canvas.export().is_err());
    }

    fn drag(canvas: &mut Canvas, tool: Tool, from: (i32, i32), to: (i32, i32)) {
        canvas.begin(tool, Point::new(from.0, from.1));
        canvas.end(Point::new(to.0, to.1));
    }

    #[test]
    fn preview_replaces_previous_shape_and_cancel_preserves_history() {
        let mut canvas = Canvas::default();
        let start = Point::new(0, 0);
        let first_end = Point::new(4, 0);
        canvas.begin(Tool::Box, start);
        canvas.update(first_end);
        assert_eq!(canvas.get(first_end), '-');
        canvas.update(Point::new(2, 2));
        assert_eq!(canvas.get(first_end), ' ');
        assert_eq!(canvas.export().unwrap(), "+-+\n| |\n+-+");
        assert!(!canvas.has_undo());
        canvas.cancel();
        assert_eq!(canvas.export().unwrap(), "");
        assert!(!canvas.has_undo());
    }

    #[test]
    fn arrow_joins_orthogonal_lines_and_points_toward_end() {
        let mut canvas = Canvas::default();
        drag(&mut canvas, Tool::Line, (2, 0), (2, 3));
        drag(&mut canvas, Tool::Arrow, (0, 1), (4, 3));
        assert_eq!(canvas.export().unwrap(), "  |\n--+-+\n  | |\n  | v");
        assert!(canvas.undo());
        assert_eq!(canvas.export().unwrap(), "|\n|\n|\n|");
        assert!(canvas.has_redo());
        assert!(canvas.redo());
        assert_eq!(canvas.get(Point::new(4, 3)), 'v');
    }

    #[test]
    fn freehand_drag_and_erase_each_commit_once() {
        let mut canvas = Canvas::default();
        let corner = Point::new(2, 0);
        canvas.begin(Tool::Draw, Point::new(0, 0));
        canvas.update(corner);
        canvas.end(Point::new(2, 2));
        assert_eq!(canvas.export().unwrap(), "--+\n  |\n  |");
        drag(&mut canvas, Tool::Erase, (2, 0), (2, 2));
        assert_eq!(canvas.export().unwrap(), "--");
        assert!(canvas.undo());
        assert_eq!(canvas.get(corner), '+');
        assert!(canvas.undo());
        assert_eq!(canvas.export().unwrap(), "");
        assert!(!canvas.has_undo());
    }

    #[test]
    fn overlapping_move_retains_cells_and_leaves_other_cells_intact() {
        let mut canvas = Canvas::default();
        canvas.import("A BX");
        drag(&mut canvas, Tool::Select, (0, 0), (2, 0));
        assert_eq!(canvas.copy_selection().unwrap().as_deref(), Some("A B"));
        drag(&mut canvas, Tool::Select, (0, 0), (1, 0));
        assert_eq!(canvas.get(Point::new(0, 0)), ' ');
        assert_eq!(canvas.get(Point::new(1, 0)), 'A');
        assert_eq!(canvas.get(Point::new(2, 0)), ' ');
        assert_eq!(canvas.get(Point::new(3, 0)), 'B');
        assert_eq!(
            canvas.selection(),
            Some(Rect::new(Point::new(1, 0), Point::new(3, 0)))
        );
        assert!(canvas.undo());
        assert_eq!(canvas.export().unwrap(), "A BX");
        assert!(canvas.redo());
        canvas.select_all();
        assert_eq!(canvas.cut_selection().unwrap().as_deref(), Some("A B"));
        assert_eq!(canvas.export().unwrap(), "");
    }

    #[test]
    fn import_and_text_preserve_internal_spacing_and_support_undo() {
        let mut canvas = Canvas::default();
        canvas.import("\r\n  α\tβ  \r\n\r\n   γ \r");
        assert_eq!(canvas.get(Point::new(2, 1)), 'α');
        assert_eq!(canvas.get(Point::new(4, 1)), 'β');
        assert_eq!(canvas.export().unwrap(), "α β\n\n γ");
        canvas.insert_text(Point::new(2, 1), " θ");
        assert_eq!(canvas.export().unwrap(), "θβ\n\nγ");
        canvas.import("");
        assert_eq!(canvas.export().unwrap(), "");
        assert!(canvas.undo());
        assert!(canvas.undo());
        assert_eq!(canvas.export().unwrap(), "α β\n\n γ");
        canvas.insert_text(Point::new(4, 1), "β");
        assert!(canvas.has_redo());
    }
}
