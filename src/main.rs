#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod architecture;
mod canvas;
mod drawings;
mod editor_ai;
mod editor_files;
mod editor_mermaid;
mod entry;
mod icons;
mod image_export;
mod mermaid;
mod templates;
mod text_files;
mod text_input;

use std::{cell::Cell, collections::HashMap, path::Path, rc::Rc, sync::Arc};

use canvas::{Canvas, Point, Rect, Tool};
use drawings::{Drawings, Region};
use entry::Entry;
use gpui::{
    App, Application, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, Div,
    ElementInputHandler, FontWeight, Image, ImageFormat, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point as UiPoint, Render, ScrollWheelEvent, SharedString,
    Stateful, TextRun, TitlebarOptions, Window, WindowBounds, WindowOptions, actions,
    canvas as paint_canvas, div, fill, font, img, point, prelude::*, px, rgb, size,
};
use icons::Icon;

const APP_NAME: &str = "ASCIIFlow Desktop";

actions!(asciiflow, [Quit]);

const BACKGROUND: u32 = 0xefede7;
const PANEL: u32 = 0xf9f8f4;
const PAPER: u32 = 0xfffdf8;
const BORDER: u32 = 0xe2ded3;
const INK: u32 = 0x26332d;
const MUTED: u32 = 0x676e63;
const FAINT: u32 = 0x777b70;
const ACCENT: u32 = 0x285943;
const ACCENT_HOVER: u32 = 0x214936;
const ACTIVE: u32 = 0xe3ece0;
const HOVER: u32 = 0xeeeee6;
const ORIGIN: Point = Point { x: -4, y: -2 };
const CELL_WIDTH: f32 = 12.0;
const CELL_HEIGHT: f32 = 21.0;
const MIN_ZOOM: u16 = 60;
const MAX_ZOOM: u16 = 180;

#[cfg(target_os = "windows")]
const MONO_FONT: &str = "Consolas";
#[cfg(target_os = "macos")]
const MONO_FONT: &str = "Menlo";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const MONO_FONT: &str = "DejaVu Sans Mono";

struct ToolItem {
    tool: Tool,
    label: &'static str,
    key: &'static str,
    hint: &'static str,
    icon: Icon,
}

static TOOLS: [ToolItem; 8] = [
    ToolItem {
        tool: Tool::Select,
        label: "Select",
        key: "S",
        hint: "Drag to select an area. Drag inside a selection to move it.",
        icon: Icon::Select,
    },
    ToolItem {
        tool: Tool::Box,
        label: "Box",
        key: "B",
        hint: "Drag from corner to corner to draw a box.",
        icon: Icon::Box,
    },
    ToolItem {
        tool: Tool::Line,
        label: "Line",
        key: "L",
        hint: "Drag between two cells to draw a line.",
        icon: Icon::Line,
    },
    ToolItem {
        tool: Tool::Arrow,
        label: "Arrow",
        key: "A",
        hint: "Drag toward the cell where the arrow should point.",
        icon: Icon::Arrow,
    },
    ToolItem {
        tool: Tool::Draw,
        label: "Draw",
        key: "D",
        hint: "Drag to draw freehand.",
        icon: Icon::Draw,
    },
    ToolItem {
        tool: Tool::Text,
        label: "Text",
        key: "T",
        hint: "Click a cell and type. Press Escape to leave text mode.",
        icon: Icon::Text,
    },
    ToolItem {
        tool: Tool::Erase,
        label: "Erase",
        key: "E",
        hint: "Drag over the cells you want to erase.",
        icon: Icon::Erase,
    },
    ToolItem {
        tool: Tool::Region,
        label: "Region",
        key: "R",
        hint: "Drag a diagram region, or click a region to select it.",
        icon: Icon::Select,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Panel {
    Drawings,
    Export,
    Ai,
    Templates,
    Review,
    Mermaid,
}

#[derive(Clone, Copy)]
enum Command {
    New,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Open,
    Save,
    Show(Panel),
    ToggleGrid,
    ZoomIn,
    ZoomOut,
    ResetView,
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Edit,
    Region {
        start: Point,
        end: Point,
    },
    Pan {
        start: UiPoint<Pixels>,
        origin: Point,
    },
}

struct Editor {
    canvas: Canvas,
    logo: Arc<Image>,
    focus: gpui::FocusHandle,
    canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    tool: Tool,
    origin: Point,
    cursor: Point,
    column_start: i32,
    composition: text_input::Composition,
    hover: Option<Point>,
    zoom: u16,
    grid: bool,
    drag: Drag,
    scroll_remainder: (f32, f32),
    drawings: Drawings,
    disk_revision: u64,
    metadata_dirty: bool,
    autosave_pending: bool,
    autosave_attempted: Option<u64>,
    storage_error: Option<String>,
    storage_protected: bool,
    close_pending: bool,
    _storage_lock: Option<std::fs::File>,
    drawing_name: gpui::Entity<Entry>,
    panel: Option<Panel>,
    active_region: Option<usize>,
    endpoint: gpui::Entity<Entry>,
    key_endpoint: String,
    _endpoint_tracking: gpui::Subscription,
    model: gpui::Entity<Entry>,
    api_key: gpui::Entity<Entry>,
    conversion_pending: bool,
    strict_schema: bool,
    review: Option<editor_ai::Review>,
    review_generation: u64,
    mermaid_review: Option<editor_mermaid::MermaidReview>,
    message: Option<String>,
    file_pending: bool,
}

impl Editor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        let (drawings, storage_error, storage_lock) =
            match Drawings::lock().and_then(|lock| Ok((Drawings::load()?, lock))) {
                Ok((drawings, lock)) => (drawings, None, Some(lock)),
                Err(error) => (
                    Drawings::default(),
                    Some(format!(
                        "Could not load saved drawings. Autosave is off. {error}"
                    )),
                    None,
                ),
            };
        let drawing_name =
            cx.new(|cx| Entry::new(drawings.active().name.clone(), "Drawing name", false, cx));
        let settings = editor_ai::AiSettings::load();
        let key_endpoint = settings.base_url.trim().to_owned();
        let endpoint =
            cx.new(|cx| Entry::new(settings.base_url, "https://provider.example/v1", false, cx));
        let model = cx.new(|cx| Entry::new(settings.model, "Model name", false, cx));
        let api_key = cx.new(|cx| {
            Entry::new(
                editor_ai::environment_key(),
                "Optional session key",
                true,
                cx,
            )
        });
        let endpoint_tracking = cx.observe(&endpoint, |editor, endpoint, cx| {
            let url = endpoint.read(cx).value().trim().to_owned();
            if editor.key_endpoint != url {
                editor.key_endpoint = url;
                if !editor.api_key.read(cx).value().is_empty() {
                    editor.api_key.update(cx, |entry, cx| entry.set("", cx));
                    editor.message =
                        Some("Endpoint changed. Enter its API key again if needed.".into());
                    cx.notify();
                }
            }
        });
        let first = drawings
            .active()
            .cells
            .iter()
            .map(|(point, _)| *point)
            .min_by_key(|point| (point.y, point.x));
        let origin = first.map_or(ORIGIN, |point| {
            Point::new(point.x.saturating_sub(4), point.y.saturating_sub(2))
        });
        let cursor = first.unwrap_or(Point::new(0, 0));
        Self {
            canvas: Canvas::from_cells(drawings.active().cells.iter().copied()),
            logo: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/asciiflow-mark.png").to_vec(),
            )),
            focus,
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            tool: Tool::Box,
            origin,
            cursor,
            column_start: cursor.x,
            composition: text_input::Composition::default(),
            hover: None,
            zoom: 100,
            grid: true,
            drag: Drag::None,
            scroll_remainder: (0.0, 0.0),
            active_region: (!drawings.active().regions.is_empty()).then_some(0),
            drawings,
            disk_revision: 0,
            metadata_dirty: false,
            autosave_pending: false,
            autosave_attempted: None,
            storage_protected: storage_error.is_some(),
            close_pending: false,
            storage_error,
            _storage_lock: storage_lock,
            drawing_name,
            panel: None,
            endpoint,
            key_endpoint,
            _endpoint_tracking: endpoint_tracking,
            model,
            api_key,
            conversion_pending: false,
            strict_schema: settings.strict_schema,
            review: None,
            review_generation: 0,
            mermaid_review: None,
            message: None,
            file_pending: false,
        }
    }

    fn choose_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.canvas.cancel();
        self.canvas.clear_selection();
        self.composition.clear();
        self.drag = Drag::None;
        self.tool = tool;
        self.column_start = self.cursor.x;
        self.message = None;
        cx.notify();
    }

    fn cell_size(&self) -> (f32, f32) {
        let scale = f32::from(self.zoom) / 100.0;
        (CELL_WIDTH * scale, CELL_HEIGHT * scale)
    }

    fn cell_at(&self, position: UiPoint<Pixels>) -> Point {
        let bounds = self.canvas_bounds.get();
        let (width, height) = self.cell_size();
        let offset = position - bounds.origin;
        let x = f32::from(offset.x).clamp(0.0, (f32::from(bounds.size.width) - 1.0).max(0.0));
        let y = f32::from(offset.y).clamp(0.0, (f32::from(bounds.size.height) - 1.0).max(0.0));
        Point::new(
            self.origin.x.saturating_add((x / width).floor() as i32),
            self.origin.y.saturating_add((y / height).floor() as i32),
        )
    }

    fn move_cursor(&mut self, point: Point) {
        self.cursor = point;
        let bounds = self.canvas_bounds.get();
        let (width, height) = self.cell_size();
        let columns = ((f32::from(bounds.size.width) / width).floor() as i32).max(1);
        let rows = ((f32::from(bounds.size.height) / height).floor() as i32).max(1);
        self.origin.x = self
            .origin
            .x
            .clamp(point.x.saturating_sub(columns - 1), point.x);
        self.origin.y = self
            .origin
            .y
            .clamp(point.y.saturating_sub(rows - 1), point.y);
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window);
        self.message = None;
        self.composition.clear();
        if event.button == MouseButton::Middle {
            self.drag = Drag::Pan {
                start: event.position,
                origin: self.origin,
            };
        } else {
            let point = self.cell_at(event.position);
            self.cursor = point;
            self.column_start = point.x;
            if self.tool == Tool::Region {
                self.drag = Drag::Region {
                    start: point,
                    end: point,
                };
            } else if self.tool != Tool::Text {
                self.canvas.begin(self.tool, point);
                self.drag = Drag::Edit;
            }
        }
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let point = self.cell_at(event.position);
        let same_cell = self.hover == Some(point);
        self.hover = Some(point);
        match self.drag {
            Drag::Region { start, .. } if !same_cell => {
                self.drag = Drag::Region { start, end: point };
            }
            Drag::Edit if !same_cell => {
                self.cursor = point;
                self.canvas.update(point);
            }
            Drag::Pan { start, origin } => {
                let delta = event.position - start;
                let (width, height) = self.cell_size();
                self.origin = Point::new(
                    origin
                        .x
                        .saturating_sub((f32::from(delta.x) / width).round() as i32),
                    origin
                        .y
                        .saturating_sub((f32::from(delta.y) / height).round() as i32),
                );
                self.hover = Some(self.cell_at(event.position));
            }
            _ if same_cell => return,
            _ => {}
        }
        cx.notify();
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.drag {
            Drag::Region { start, .. } if event.button == MouseButton::Left => {
                let end = self.cell_at(event.position);
                if start == end {
                    self.active_region = self
                        .drawings
                        .active()
                        .regions
                        .iter()
                        .rposition(|region| region.rect.contains(end));
                } else {
                    self.add_region(Rect::new(start, end), None, None);
                }
            }
            Drag::Edit if event.button == MouseButton::Left => {
                self.cursor = self.cell_at(event.position);
                self.canvas.end(self.cursor);
            }
            Drag::Pan { .. } if event.button == MouseButton::Middle => {}
            _ => return,
        }
        self.drag = Drag::None;
        cx.notify();
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.drag, Drag::None) {
            return;
        }
        let delta = event.delta.pixel_delta(px(CELL_HEIGHT));
        let mut x = f32::from(delta.x);
        let mut y = f32::from(delta.y);
        if event.modifiers.control || event.modifiers.platform {
            let zoom = if y > 0.0 {
                self.zoom.saturating_add(10)
            } else if y < 0.0 {
                self.zoom.saturating_sub(10)
            } else {
                self.zoom
            };
            self.set_zoom(zoom, Some(event.position));
        } else {
            if event.modifiers.shift && x == 0.0 {
                (x, y) = (y, 0.0);
            }
            let (width, height) = self.cell_size();
            self.scroll_remainder.0 -= x / width;
            self.scroll_remainder.1 -= y / height;
            let dx = self.scroll_remainder.0 as i32;
            let dy = self.scroll_remainder.1 as i32;
            self.scroll_remainder.0 -= dx as f32;
            self.scroll_remainder.1 -= dy as f32;
            self.origin.x = self.origin.x.saturating_add(dx);
            self.origin.y = self.origin.y.saturating_add(dy);
        }
        self.hover = Some(self.cell_at(event.position));
        cx.notify();
    }

    fn set_zoom(&mut self, zoom: u16, anchor: Option<UiPoint<Pixels>>) {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let anchor = anchor.map(|position| (position, self.cell_at(position)));
        self.zoom = zoom;
        if let Some((position, before)) = anchor {
            let after = self.cell_at(position);
            self.origin.x = self
                .origin
                .x
                .saturating_add(before.x.saturating_sub(after.x));
            self.origin.y = self
                .origin
                .y
                .saturating_add(before.y.saturating_sub(after.y));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control {
            let command = match (key, modifiers.shift) {
                ("z", false) => Some(Command::Undo),
                ("z", true) | ("y", _) => Some(Command::Redo),
                ("x", _) => Some(Command::Cut),
                ("c", _) => Some(Command::Copy),
                ("v", _) => Some(Command::Paste),
                ("o", _) => Some(Command::Open),
                ("n", _) => Some(Command::New),
                ("s", _) => Some(Command::Save),
                ("0", _) => Some(Command::ResetView),
                ("=", _) | ("+", _) => Some(Command::ZoomIn),
                ("-", _) => Some(Command::ZoomOut),
                ("a", _) => {
                    self.choose_tool(Tool::Select, cx);
                    self.canvas.select_all();
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                _ => None,
            };
            if let Some(command) = command {
                self.command(command, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }
            return;
        }

        match key {
            "escape" => {
                self.panel = None;
                self.choose_tool(Tool::Select, cx);
            }
            "left" => self.move_cursor(Point::new(self.cursor.x.saturating_sub(1), self.cursor.y)),
            "right" => self.move_cursor(Point::new(self.cursor.x.saturating_add(1), self.cursor.y)),
            "up" => self.move_cursor(Point::new(self.cursor.x, self.cursor.y.saturating_sub(1))),
            "down" => self.move_cursor(Point::new(self.cursor.x, self.cursor.y.saturating_add(1))),
            "backspace" | "delete" => {
                if !self.canvas.delete_selection() && self.tool == Tool::Text {
                    if key == "backspace" {
                        self.move_cursor(Point::new(
                            self.cursor.x.saturating_sub(1),
                            self.cursor.y,
                        ));
                    }
                    self.canvas.begin(Tool::Erase, self.cursor);
                    self.canvas.end(self.cursor);
                }
            }
            "enter" if self.tool == Tool::Text => {
                self.move_cursor(Point::new(
                    self.column_start,
                    self.cursor.y.saturating_add(1),
                ));
            }
            _ if self.tool == Tool::Text => return,
            _ if !modifiers.alt => {
                let Some(item) = TOOLS.iter().find(|item| item.key.eq_ignore_ascii_case(key))
                else {
                    return;
                };
                self.choose_tool(item.tool, cx);
            }
            _ => return,
        }
        self.message = None;
        self.hover = None;
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window);
        self.message = None;
        self.composition.clear();
        match command {
            Command::New => {
                self.change_drawings(cx, |drawings| drawings.create("Untitled").map(|_| ()));
                self.panel = None;
            }
            Command::Undo => {
                if self.canvas.undo() {
                    self.message = Some("Change undone".into());
                }
            }
            Command::Redo => {
                if self.canvas.redo() {
                    self.message = Some("Change redone".into());
                }
            }
            Command::Cut => match self.canvas.cut_selection() {
                Ok(Some(text)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.message = Some("Selection cut".into());
                }
                Ok(None) => {}
                Err(error) => self.message = Some(error),
            },
            Command::Copy => {
                let export = self
                    .canvas
                    .copy_selection()
                    .and_then(|selection| match selection {
                        Some(text) => Ok((text, "Selection copied")),
                        None => self.canvas.export().map(|text| (text, "Diagram copied")),
                    });
                match export {
                    Ok((text, label)) if !text.is_empty() => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.message = Some(label.into());
                    }
                    Ok(_) => self.message = Some("Canvas is empty".into()),
                    Err(error) => self.message = Some(error),
                }
            }
            Command::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    if text.len() > canvas::MAX_TEXT_BYTES {
                        self.message = Some("The clipboard text exceeds 2 MiB.".into());
                    } else {
                        self.canvas.insert_text(self.cursor, &text);
                        self.message = Some("Text pasted at the cursor".into());
                    }
                }
            }
            Command::Open => self.open_text(cx),
            Command::Save => self.save_text(cx),
            Command::Show(panel) => {
                self.panel = (self.panel != Some(panel)).then_some(panel);
                if self.panel == Some(Panel::Drawings) {
                    self.drawing_name.read(cx).focus(window);
                }
            }
            Command::ToggleGrid => self.grid = !self.grid,
            Command::ZoomIn => self.set_zoom(self.zoom.saturating_add(10), None),
            Command::ZoomOut => self.set_zoom(self.zoom.saturating_sub(10), None),
            Command::ResetView => self.reset_view(),
        }
        cx.notify();
    }

    fn reset_view(&mut self) {
        self.origin = ORIGIN;
        self.cursor = Point::new(0, 0);
        self.column_start = 0;
        self.composition.clear();
        self.zoom = 100;
        self.hover = None;
        self.scroll_remainder = (0.0, 0.0);
    }

    fn open_text(&mut self, cx: &mut Context<Self>) {
        if self.file_pending {
            return;
        }
        self.file_pending = true;
        let result = text_files::open(cx);
        cx.spawn(async move |editor, cx| {
            let result = result.await;
            let _ = editor.update(cx, |editor, cx| {
                editor.file_pending = false;
                match result {
                    Ok(Some(text)) => {
                        if editor.change_drawings(cx, |drawings| {
                            drawings.create("Imported diagram").map(|_| ())
                        }) {
                            editor.canvas.import(&text);
                            editor.panel = None;
                            editor.message = Some("Text file opened as a new drawing".into());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => editor.message = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_text(&mut self, cx: &mut Context<Self>) {
        if self.file_pending {
            return;
        }
        let text = self.canvas.export();
        self.export_generated("diagram.txt", move || text.map(String::into_bytes), cx);
    }

    fn record_export(&mut self, path: &Path) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("text file");
        self.message = Some(format!("Exported {name}"));
    }

    fn workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds_cell = self.canvas_bounds.clone();
        let editor = cx.entity();
        let input_editor = editor.clone();
        let focus = self.focus.clone();
        let sheet = paint_canvas(
            move |bounds, _, cx| {
                bounds_cell.set(bounds);
                Sheet::new(editor.read(cx), bounds)
            },
            move |bounds, sheet, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, input_editor), cx);
                sheet.paint(bounds, window, cx);
            },
        )
        .size_full();

        div()
            .id("sheet")
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(PAPER))
            .cursor(match (self.tool, self.drag) {
                (_, Drag::Pan { .. }) => CursorStyle::ClosedHand,
                (Tool::Text, _) => CursorStyle::IBeam,
                (_, _) => CursorStyle::Crosshair,
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_hover(cx.listener(|editor, hovered, _, cx| {
                if !hovered && editor.hover.take().is_some() {
                    cx.notify();
                }
            }))
            .child(sheet)
    }

    fn header(&self, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(64.0))
            .flex_shrink_0()
            .px(px(22.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.0))
                    .child(img(self.logo.clone()).size(px(32.0)).flex_shrink_0())
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("ASCIIFlow"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(button(
                        "new",
                        "New",
                        Icon::New,
                        Command::New,
                        Button::Normal,
                        cx,
                    ))
                    .child(div().mx(px(3.0)).w(px(1.0)).h(px(24.0)).bg(rgb(BORDER)))
                    .child(button(
                        "undo",
                        "Undo",
                        Icon::Undo,
                        Command::Undo,
                        Button::enabled(self.canvas.has_undo()),
                        cx,
                    ))
                    .child(button(
                        "redo",
                        "Redo",
                        Icon::Redo,
                        Command::Redo,
                        Button::enabled(self.canvas.has_redo()),
                        cx,
                    ))
                    .child(div().mx(px(3.0)).w(px(1.0)).h(px(24.0)).bg(rgb(BORDER)))
                    .child(button(
                        "import",
                        "Open",
                        Icon::Open,
                        Command::Open,
                        Button::enabled(!self.file_pending),
                        cx,
                    ))
                    .child(button(
                        "export",
                        "Export",
                        Icon::Export,
                        Command::Show(Panel::Export),
                        Button::enabled(!self.file_pending),
                        cx,
                    ))
                    .child(button(
                        "copy",
                        "Copy ASCII",
                        Icon::Copy,
                        Command::Copy,
                        Button::Primary,
                        cx,
                    )),
            )
    }

    fn toolbox(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("editor-toolbox")
            .w(px(179.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .pt(px(22.0))
            .px(px(12.0))
            .pb(px(16.0))
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .px(px(9.0))
                    .pb(px(12.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("TOOLS"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(TOOLS.iter().map(|item| self.tool_button(item, cx))),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(
                        [
                            ("library", "Drawings", Panel::Drawings),
                            ("templates", "Templates", Panel::Templates),
                            ("ai-rendering", "Diagram rendering", Panel::Ai),
                        ]
                        .map(|(id, label, panel)| {
                            panel_button(
                                id,
                                label,
                                cx.listener(move |editor, _, window, cx| {
                                    editor.command(Command::Show(panel), window, cx);
                                }),
                            )
                        }),
                    ),
            )
    }

    fn tool_button(&self, item: &ToolItem, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = item.tool == self.tool;
        let tool = item.tool;
        div()
            .id(item.label)
            .h(px(37.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .text_color(rgb(if active { ACCENT } else { INK }))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .when(active, |el| el.bg(rgb(ACTIVE)))
            .when(!active, |el| el.hover(|style| style.bg(rgb(HOVER))))
            .child(item.icon.element(if active { ACCENT } else { MUTED }))
            .child(div().flex_1().child(item.label))
            .child(
                div()
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(if active { 0xbdd1bf } else { BORDER }))
                    .text_size(px(11.0))
                    .font_family(MONO_FONT)
                    .text_color(rgb(if active { ACCENT } else { FAINT }))
                    .child(item.key),
            )
            .on_click(cx.listener(move |editor, _, window, cx| {
                editor.focus.focus(window);
                editor.panel = None;
                editor.choose_tool(tool, cx);
            }))
    }

    fn artboard(&self, cx: &mut Context<Self>) -> Div {
        let title: SharedString = self.drawings.active().name.clone().into();
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .m(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PAPER))
            .overflow_hidden()
            .child(
                div()
                    .h(px(46.0))
                    .flex_shrink_0()
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Icon::Document.element(FAINT))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(title)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .child(button(
                                "grid",
                                "Grid",
                                Icon::Grid,
                                Command::ToggleGrid,
                                if self.grid {
                                    Button::Selected
                                } else {
                                    Button::Normal
                                },
                                cx,
                            ))
                            .child(div().w(px(1.0)).h(px(17.0)).mx(px(5.0)).bg(rgb(BORDER)))
                            .child(button(
                                "zoom-out",
                                "",
                                Icon::Minus,
                                Command::ZoomOut,
                                Button::enabled(self.zoom > MIN_ZOOM),
                                cx,
                            ))
                            .child(
                                div()
                                    .id("zoom-reset")
                                    .w(px(47.0))
                                    .h(px(30.0))
                                    .rounded(px(5.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(HOVER)))
                                    .font_family(MONO_FONT)
                                    .text_size(px(12.0))
                                    .child(format!("{}%", self.zoom))
                                    .on_click(cx.listener(|editor, _, window, cx| {
                                        editor.command(Command::ResetView, window, cx)
                                    })),
                            )
                            .child(button(
                                "zoom-in",
                                "",
                                Icon::Plus,
                                Command::ZoomIn,
                                Button::enabled(self.zoom < MAX_ZOOM),
                                cx,
                            )),
                    ),
            )
            .child(self.workspace(cx))
            .when_some(self.active_region, |artboard, index| {
                artboard.child(self.region_toolbar(index, cx))
            })
    }

    fn status(&self) -> Div {
        let tool = TOOLS
            .iter()
            .find(|item| item.tool == self.tool)
            .expect("tool is listed");
        let message = self
            .storage_error
            .as_ref()
            .or(self.message.as_ref())
            .cloned()
            .unwrap_or_else(|| tool.hint.into());
        let point = self.hover.unwrap_or(self.cursor);
        div()
            .h(px(31.0))
            .flex_shrink_0()
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(BORDER))
            .text_size(px(11.0))
            .text_color(rgb(MUTED))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .min_w_0()
                    .child(div().size(px(6.0)).rounded_full().bg(rgb(ACCENT)))
                    .child(div().truncate().child(message)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(MONO_FONT)
                    .child(format!("x {}   y {}", point.x, point.y)),
            )
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.schedule_autosave(cx);
        let content = match self.panel {
            Some(panel) => self.panel_frame(panel, cx).into_any_element(),
            None => self.artboard(cx).into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(INK))
            .text_size(px(13.0))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(self.header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.toolbox(cx))
                    .child(content),
            )
            .child(self.status())
    }
}

fn panel_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(PANEL))
        .hover(|style| style.bg(rgb(HOVER)))
        .cursor_pointer()
        .child(div().truncate().child(label.into()))
        .on_click(on_click)
}

#[derive(Clone, Copy)]
enum Button {
    Normal,
    Disabled,
    Selected,
    Primary,
}

impl Button {
    fn enabled(enabled: bool) -> Self {
        if enabled {
            Self::Normal
        } else {
            Self::Disabled
        }
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    command: Command,
    style: Button,
    cx: &mut Context<Editor>,
) -> impl IntoElement {
    let (foreground, background, hover) = match style {
        Button::Primary => (PAPER, Some(ACCENT), ACCENT_HOVER),
        Button::Disabled => (0x929489, None, PANEL),
        Button::Selected => (ACCENT, Some(ACTIVE), 0xd8e7d8),
        Button::Normal => (INK, None, HOVER),
    };
    let enabled = !matches!(style, Button::Disabled);
    div()
        .id(id)
        .h(px(32.0))
        .px(px(if label.is_empty() { 7.0 } else { 9.0 }))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(foreground))
        .when_some(background, |el, color| el.bg(rgb(color)))
        .child(icon.element(foreground))
        .when(!label.is_empty(), |el| el.child(label))
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .on_click(
                    cx.listener(move |editor, _, window, cx| editor.command(command, window, cx)),
                )
        })
}

struct Sheet {
    width: f32,
    height: f32,
    origin: Point,
    grid: bool,
    columns: i32,
    rows: i32,
    cells: Vec<(Point, char)>,
    selection: Option<Rect>,
    regions: Vec<Rect>,
    active_region: Option<Rect>,
    region_drag: Option<Rect>,
    cursor: Point,
    is_text: bool,
    hover: Option<Point>,
}

impl Sheet {
    fn new(editor: &Editor, bounds: Bounds<Pixels>) -> Self {
        let (width, height) = editor.cell_size();
        let columns = (f32::from(bounds.size.width) / width).ceil() as i32;
        let rows = (f32::from(bounds.size.height) / height).ceil() as i32;
        let origin = editor.origin;
        let mut cells = Vec::new();
        for y in origin.y..origin.y.saturating_add(rows) {
            for x in origin.x..origin.x.saturating_add(columns) {
                let point = Point::new(x, y);
                let ch = editor
                    .composition
                    .character_at(editor.cursor, point)
                    .unwrap_or_else(|| editor.canvas.get(point));
                if ch != ' ' {
                    cells.push((point, ch));
                }
            }
        }
        Self {
            width,
            height,
            origin,
            grid: editor.grid,
            columns,
            rows,
            cells,
            selection: editor.canvas.selection(),
            regions: editor
                .drawings
                .active()
                .regions
                .iter()
                .map(|region| region.rect)
                .collect(),
            active_region: editor
                .active_region
                .and_then(|index| editor.drawings.active().regions.get(index))
                .map(|region| region.rect),
            region_drag: match editor.drag {
                Drag::Region { start, end } => Some(Rect::new(start, end)),
                _ => None,
            },
            cursor: editor.composition.caret_at(editor.cursor),
            is_text: editor.tool == Tool::Text,
            hover: editor.hover,
        }
    }

    fn cell_bounds(&self, bounds: Bounds<Pixels>, cell: Point) -> Bounds<Pixels> {
        Bounds::new(
            bounds.origin
                + point(
                    px((i64::from(cell.x) - i64::from(self.origin.x)) as f32 * self.width),
                    px((i64::from(cell.y) - i64::from(self.origin.y)) as f32 * self.height),
                ),
            size(px(self.width), px(self.height)),
        )
    }

    fn paint(self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for region in &self.regions {
                let rect = Bounds::from_corners(
                    self.cell_bounds(bounds, region.min).origin,
                    self.cell_bounds(bounds, region.max).bottom_right(),
                );
                let active = self.active_region == Some(*region);
                window.paint_quad(fill(
                    rect,
                    gpui::rgba(if active { 0x6f9bb52d } else { 0x6f9bb515 }),
                ));
                outline(rect, if active { 0x437795 } else { 0x8cabbc }, window);
            }
            if let Some(region) = self.region_drag {
                let rect = Bounds::from_corners(
                    self.cell_bounds(bounds, region.min).origin,
                    self.cell_bounds(bounds, region.max).bottom_right(),
                );
                window.paint_quad(fill(rect, gpui::rgba(0x6f9bb533)));
                outline(rect, 0x437795, window);
            }
            if self.grid {
                for row in 0..self.rows {
                    for col in 0..self.columns {
                        let major = self.origin.x.saturating_add(col) % 5 == 0
                            && self.origin.y.saturating_add(row) % 5 == 0;
                        let origin = bounds.origin
                            + point(
                                px(col as f32 * self.width + self.width * 0.5),
                                px(row as f32 * self.height + self.height * 0.5),
                            );
                        let diameter = if major { 1.7 } else { 1.1 };
                        window.paint_quad(fill(
                            Bounds::new(origin, size(px(diameter), px(diameter))),
                            rgb(if major { 0xc7c8b8 } else { 0xe7e3d7 }),
                        ));
                    }
                }
            }

            if let Some(selection) = self.selection {
                let min = self.cell_bounds(bounds, selection.min);
                let max = self.cell_bounds(bounds, selection.max);
                let rect = Bounds::from_corners(min.origin, max.bottom_right());
                window.paint_quad(fill(rect, gpui::rgba(0x8eb69b42)));
                outline(rect, 0x80a88a, window);
            }

            if let Some(hover) = self.hover.filter(|hover| *hover != self.cursor) {
                outline(self.cell_bounds(bounds, hover), 0xc8d6c6, window);
            }

            let cursor = self.cell_bounds(bounds, self.cursor);
            if self.is_text {
                window.paint_quad(fill(
                    Bounds::new(
                        cursor.origin + point(px(1.0), px(2.0)),
                        size(px(1.5), px(self.height - 4.0)),
                    ),
                    rgb(ACCENT),
                ));
            } else {
                outline(cursor, 0x9ab29e, window);
            }

            let text_system = window.text_system().clone();
            let font_size = px(self.height * 0.76);
            let line_height = px(self.height);
            let mut shaped = HashMap::new();
            for (cell, ch) in &self.cells {
                let line = shaped.entry(*ch).or_insert_with(|| {
                    let text = ch.to_string();
                    let run = TextRun {
                        len: text.len(),
                        font: font(MONO_FONT),
                        color: rgb(INK).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    text_system.shape_line(text.into(), font_size, &[run], None)
                });
                let cell_bounds = self.cell_bounds(bounds, *cell);
                let x = (cell_bounds.size.width - line.width) / 2.0;
                let origin = cell_bounds.origin + point(x, px(0.0));
                let _ = line.paint(origin, line_height, window, cx);
            }
        });
    }
}

fn outline(bounds: Bounds<Pixels>, color: u32, window: &mut Window) {
    let stroke = px(1.0);
    let color = rgb(color);
    let horizontal = size(bounds.size.width, stroke);
    let vertical = size(stroke, bounds.size.height);
    window.paint_quad(fill(Bounds::new(bounds.origin, horizontal), color));
    window.paint_quad(fill(
        Bounds::new(point(bounds.left(), bounds.bottom() - stroke), horizontal),
        color,
    ));
    window.paint_quad(fill(Bounds::new(bounds.origin, vertical), color));
    window.paint_quad(fill(
        Bounds::new(point(bounds.right() - stroke, bounds.top()), vertical),
        color,
    ));
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1120.0), px(740.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(750.0), px(455.0))),
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            app_id: Some("io.asciiflow.desktop".into()),
            ..Default::default()
        };

        let window = match cx.open_window(options, |window, cx| {
            let editor = cx.new(|cx| Editor::new(window, cx));
            let closing = editor.downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                closing
                    .update(cx, |editor, cx| editor.request_close(window, cx))
                    .unwrap_or(true)
            });
            editor
        }) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("Could not open {APP_NAME}: {error}");
                cx.quit();
                return;
            }
        };
        // Quit through the same save check as closing the window. If the save
        // fails, the editor asks first and closing the window ends the app.
        // Defer the check because the action can arrive while the window is
        // still handling the key event.
        cx.on_action(move |_: &Quit, cx| {
            cx.defer(move |cx| {
                match window.update(cx, |editor, window, cx| editor.request_close(window, cx)) {
                    Ok(true) => cx.quit(),
                    Ok(false) => {}
                    Err(_) if cx.windows().is_empty() => cx.quit(),
                    Err(_) => {}
                }
            });
        });
        #[cfg(target_os = "macos")]
        {
            use gpui::{Menu, MenuItem, SystemMenuType};
            cx.bind_keys([gpui::KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus(vec![Menu {
                name: APP_NAME.into(),
                items: vec![
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action(format!("Quit {APP_NAME}"), Quit),
                ],
            }]);
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
