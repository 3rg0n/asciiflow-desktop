use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    fmt::Write,
};

use super::{ArchitectureGraph, Group, Node};

const CARD_WIDTH: i32 = 238;
const HORIZONTAL_GAP: i32 = 128;
const VERTICAL_GAP: i32 = 74;
const GROUP_PADDING: i32 = 32;
const PAGE_PADDING: i32 = 36;
const PORT_MARGIN: i32 = 20;

pub fn render_html(graph: &ArchitectureGraph) -> Result<String, String> {
    let svg = render_svg(graph)?;
    let title = escaped(&graph.title);
    let mut warnings = String::new();
    if !graph.warnings.is_empty() {
        warnings.push_str("<section class=\"warnings\" aria-labelledby=\"warning-title\"><h2 id=\"warning-title\">Review notes</h2><ul>");
        for warning in &graph.warnings {
            write!(warnings, "<li>{}</li>", escaped(warning)).unwrap();
        }
        warnings.push_str("</ul></section>");
    }
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>{title}</title>
<style>
:root {{ color-scheme: light; font-family: 'Segoe UI', system-ui, sans-serif; color: #17362b; background: #eceee8; }}
body {{ margin: 0; }}
main {{ width: min(96%, 110rem); margin: clamp(.6rem, 3vw, 2rem) auto; }}
.diagram {{ overflow: auto; border: 1px solid #d4dbd0; border-radius: 1rem; background: #f8f9f4; box-shadow: 0 8px 28px #1e362012; }}
.diagram svg {{ display: block; width: 100%; height: auto; min-width: 46rem; }}
.warnings {{ margin: 1rem 0 0; padding: 1rem 1.4rem; background: #fffaf0; border: 1px solid #e7d7ae; border-radius: .8rem; color: #593e16; }}
.warnings h2 {{ margin: 0; font-size: 1rem; }}
.warnings ul {{ margin: .5rem 0 0; padding-left: 1.25rem; line-height: 1.5; }}
@media print {{ :root {{ background: white; }} main {{ width: 100%; margin: 0; }} .diagram {{ overflow: visible; box-shadow: none; }} .diagram svg {{ min-width: 0; }} .warnings {{ break-inside: avoid; }} }}
</style>
<main>
<div class="diagram" tabindex="0" aria-label="Architecture diagram">{svg}</div>
{warnings}
</main>
</html>
"#
    ))
}

pub fn render_svg(graph: &ArchitectureGraph) -> Result<String, String> {
    graph.validate()?;
    let display = DisplayText::new(graph);
    let edges: Vec<_> = graph
        .edges
        .iter()
        .map(|edge| {
            let from = graph
                .nodes
                .iter()
                .position(|node| node.id == edge.from)
                .unwrap();
            let to = graph
                .nodes
                .iter()
                .position(|node| node.id == edge.to)
                .unwrap();
            (from, to)
        })
        .collect();
    let layout = Layout::new(graph, &display, &edges);
    let ports = measure_ports(&layout, &edges);
    let mut router = Router::new(&layout, &ports);
    let routes: Vec<_> = ports
        .iter()
        .map(|&(start, end)| router.route(start, end))
        .collect::<Option<_>>()
        .ok_or("Could not route an architecture connection.")?;
    let mut label_obstacles = layout.obstacles.clone();
    let mut annotations = Vec::new();
    let mut overflow_labels = Vec::new();
    for (index, route) in routes.iter().enumerate() {
        let Some(label) = &graph.edges[index].label else {
            continue;
        };
        let candidates = [wrap(label, 29), wrap(label, 12)];
        let annotation = candidates.into_iter().find_map(|lines| {
            place_edge_label(layout.bounds, route, &lines, &label_obstacles)
                .map(|rect| (lines, rect))
        });
        if let Some((lines, rect)) = annotation {
            annotations.push((lines, rect));
            label_obstacles.push(rect.grow(4));
        } else {
            let (from, to) = edges[index];
            let marker = format!("L{}", overflow_labels.len() + 1);
            let badge = vec![marker.clone()];
            if let Some(rect) = place_edge_label(layout.bounds, route, &badge, &label_obstacles) {
                annotations.push((badge, rect));
                label_obstacles.push(rect.grow(4));
            }
            let entry = format!(
                "{marker} · {} → {} · {label}",
                graph.nodes[from].label, graph.nodes[to].label
            );
            overflow_labels.extend(wrap(&entry, (layout.width - 2 * PAGE_PADDING) / 8));
        }
    }
    let height = layout.height
        + 34
        + if overflow_labels.is_empty() {
            0
        } else {
            70 + overflow_labels.len() as i32 * 18
        };
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-labelledby="diagram-title" font-family="Segoe UI, DejaVu Sans, sans-serif">
<title id="diagram-title">{title}</title>
<defs><pattern id="dots" width="18" height="18" patternUnits="userSpaceOnUse"><circle cx="2" cy="2" r=".85" fill="#dce3d8"/></pattern></defs>
<rect width="{width}" height="{height}" fill="#f8f9f4"/>
<rect y="{header}" width="{width}" height="{height}" fill="url(#dots)"/>
<path d="M0 {header}H{width}" stroke="#dce3d8"/>
"##,
        width = layout.width,
        header = layout.header,
        title = escaped(&graph.title),
    );
    for (line, title) in display.title.iter().enumerate() {
        write!(svg, r##"<text x="{PAGE_PADDING}" y="{}" fill="#17362b" font-size="29" font-weight="700">{}</text>"##, 48 + 34 * line, escaped(title)).unwrap();
    }
    for (index, rect) in &layout.groups {
        draw_group(
            &mut svg,
            &graph.groups[*index],
            &display.groups[*index],
            *rect,
        );
    }
    for (edge, route) in routes.iter().enumerate() {
        let (from, to) = edges[edge];
        let label = graph.edges[edge]
            .label
            .as_deref()
            .map_or(String::new(), |label| format!(" · {label}"));
        let summary = format!(
            "{} → {}{label}",
            graph.nodes[from].label, graph.nodes[to].label
        );
        draw_edge(&mut svg, route, &summary);
    }
    for (index, node) in graph.nodes.iter().enumerate() {
        draw_node(&mut svg, node, &display.nodes[index], layout.nodes[index]);
    }
    for (lines, rect) in annotations {
        draw_edge_label(&mut svg, &lines, rect);
    }
    if !overflow_labels.is_empty() {
        let width = layout.width;
        let footer = layout.height;
        write!(svg, r##"<rect y="{footer}" width="{width}" height="{}" fill="#f8f9f4"/><path d="M0 {footer}H{width}" stroke="#dce3d8"/><text x="{PAGE_PADDING}" y="{}" fill="#17362b" font-size="15" font-weight="700">Connection labels</text>"##, height - footer, footer + 33).unwrap();
        for (index, line) in overflow_labels.iter().enumerate() {
            write!(
                svg,
                r##"<text x="{PAGE_PADDING}" y="{}" fill="#3f5f51" font-size="13">{}</text>"##,
                footer + 57 + 18 * index as i32,
                escaped(line)
            )
            .unwrap();
        }
    }
    write!(svg, r##"<text x="{PAGE_PADDING}" y="{}" fill="#597467" font-size="12">AI-assisted interpretation · Check against the source drawing</text>"##, height - 16).unwrap();
    svg.push_str("</svg>");
    Ok(svg)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn distance(self, other: Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Rect {
    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(self) -> i32 {
        self.x + self.width
    }
    fn bottom(self) -> i32 {
        self.y + self.height
    }

    fn center(self) -> Point {
        Point {
            x: self.x + self.width / 2,
            y: self.y + self.height / 2,
        }
    }

    fn grow(self, margin: i32) -> Self {
        Self::new(
            self.x - margin,
            self.y - margin,
            self.width + 2 * margin,
            self.height + 2 * margin,
        )
    }

    fn translate(self, x: i32, y: i32) -> Self {
        Self::new(self.x + x, self.y + y, self.width, self.height)
    }

    fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }
}

struct DisplayText {
    title: Vec<String>,
    groups: Vec<Vec<String>>,
    nodes: Vec<NodeText>,
}

struct NodeText {
    title: Vec<String>,
    details: Vec<String>,
}

impl DisplayText {
    fn new(graph: &ArchitectureGraph) -> Self {
        Self {
            title: wrap(&graph.title, 48),
            groups: graph
                .groups
                .iter()
                .map(|group| wrap(&group.label, 28))
                .collect(),
            nodes: graph
                .nodes
                .iter()
                .map(|node| NodeText {
                    title: wrap(&node.label, 24),
                    details: node
                        .details
                        .iter()
                        .flat_map(|detail| wrap(detail, 30))
                        .collect(),
                })
                .collect(),
        }
    }
}

impl NodeText {
    fn height(&self) -> i32 {
        let title_height = 20 * (self.title.len() as i32 - 1);
        let detail_height = if self.details.is_empty() {
            0
        } else {
            12 + self.details.len() as i32 * 17
        };
        65 + title_height + detail_height
    }
}

struct Layout {
    width: i32,
    height: i32,
    header: i32,
    bounds: Rect,
    nodes: Vec<Rect>,
    groups: Vec<(usize, Rect)>,
    obstacles: Vec<Rect>,
}

#[derive(Default)]
struct Block {
    width: i32,
    height: i32,
    order: usize,
    nodes: Vec<(usize, Rect)>,
    groups: Vec<(usize, Rect)>,
}

impl Layout {
    fn new(graph: &ArchitectureGraph, display: &DisplayText, edges: &[(usize, usize)]) -> Self {
        let order = node_order(graph.nodes.len(), edges);
        let content = layout_block(graph, display, &order, None);
        let header = 68 + (display.title.len() as i32 - 1) * 34;
        let width = (content.width + 2 * PAGE_PADDING).max(760);
        let height = header + content.height + 2 * PAGE_PADDING;
        let x = (width - content.width) / 2;
        let y = header + PAGE_PADDING;
        let mut nodes = vec![Rect::default(); graph.nodes.len()];
        for (index, rect) in content.nodes {
            nodes[index] = rect.translate(x, y);
        }
        let groups: Vec<_> = content
            .groups
            .into_iter()
            .map(|(index, rect)| (index, rect.translate(x, y)))
            .collect();
        let obstacles = nodes
            .iter()
            .map(|rect| rect.grow(11))
            .chain(
                groups
                    .iter()
                    .map(|(index, rect)| group_label_rect(*rect, &display.groups[*index]).grow(6)),
            )
            .collect();
        let bounds = Rect::new(15, header + 12, width - 30, height - header - 24);
        Self {
            width,
            height,
            header,
            bounds,
            nodes,
            groups,
            obstacles,
        }
    }
}

fn node_order(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut outgoing = vec![Vec::new(); count];
    let mut incoming = vec![0; count];
    for &(from, to) in edges {
        if from != to {
            outgoing[from].push(to);
            incoming[to] += 1;
        }
    }
    let mut queue: VecDeque<_> = (0..count).filter(|&node| incoming[node] == 0).collect();
    let mut order = vec![usize::MAX; count];
    for position in 0..count {
        let node = queue
            .pop_front()
            .unwrap_or_else(|| order.iter().position(|&value| value == usize::MAX).unwrap());
        order[node] = position;
        for &target in &outgoing[node] {
            incoming[target] -= 1;
            if incoming[target] == 0 && order[target] == usize::MAX {
                queue.push_back(target);
            }
        }
    }
    order
}

fn layout_block(
    graph: &ArchitectureGraph,
    display: &DisplayText,
    order: &[usize],
    group_index: Option<usize>,
) -> Block {
    let group_id = group_index.map(|index| graph.groups[index].id.as_str());
    let mut blocks: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.group.as_deref() == group_id)
        .map(|(index, _)| {
            let height = display.nodes[index].height();
            Block {
                width: CARD_WIDTH,
                height,
                order: order[index],
                nodes: vec![(index, Rect::new(0, 0, CARD_WIDTH, height))],
                groups: Vec::new(),
            }
        })
        .collect();
    blocks.extend(
        graph
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.parent.as_deref() == group_id)
            .map(|(index, _)| layout_block(graph, display, order, Some(index))),
    );
    blocks.sort_by_key(|block| block.order);

    let padding = group_index.map_or(0, |_| GROUP_PADDING);
    let top = group_index.map_or(0, |index| 67 + display.groups[index].len() as i32 * 18);
    let row_width = if group_index.is_none() { 1440 } else { 1120 };
    let mut content = Block {
        order: blocks.first().map_or(usize::MAX, |block| block.order),
        ..Block::default()
    };
    let mut horizontal = padding;
    let mut vertical = top;
    let mut row_height = 0;
    for block in blocks {
        if horizontal != padding && horizontal + block.width + padding > row_width {
            horizontal = padding;
            vertical += row_height + VERTICAL_GAP;
            row_height = 0;
        }
        content.nodes.extend(
            block
                .nodes
                .into_iter()
                .map(|(index, rect)| (index, rect.translate(horizontal, vertical))),
        );
        content.groups.extend(
            block
                .groups
                .into_iter()
                .map(|(index, rect)| (index, rect.translate(horizontal, vertical))),
        );
        content.width = content.width.max(horizontal + block.width + padding);
        row_height = row_height.max(block.height);
        horizontal += block.width + HORIZONTAL_GAP;
    }
    content.height = vertical + row_height + padding;
    if let Some(index) = group_index {
        let heading = group_label_rect(Rect::default(), &display.groups[index]);
        content.width = content
            .width
            .max(heading.right() + padding)
            .max(CARD_WIDTH + 2 * padding);
        content
            .groups
            .insert(0, (index, Rect::new(0, 0, content.width, content.height)));
    }
    content
}

#[derive(Clone, Copy)]
enum Side {
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

#[derive(Clone, Copy)]
struct Port {
    anchor: Point,
    outside: Point,
}

fn measure_ports(layout: &Layout, edges: &[(usize, usize)]) -> Vec<(Port, Port)> {
    let sides: Vec<_> = edges
        .iter()
        .map(|&(from, to)| {
            if from == to {
                return (Side::Right, Side::Top);
            }
            let start = layout.nodes[from];
            let end = layout.nodes[to];
            let dx = end.center().x - start.center().x;
            let dy = end.center().y - start.center().y;
            if (start.right() < end.x || end.right() < start.x) && dx.abs() >= dy.abs() {
                if dx > 0 {
                    (Side::Right, Side::Left)
                } else {
                    (Side::Left, Side::Right)
                }
            } else if dy > 0 {
                (Side::Bottom, Side::Top)
            } else {
                (Side::Top, Side::Bottom)
            }
        })
        .collect();
    let mut totals = vec![[0; 4]; layout.nodes.len()];
    for (&(from, to), &(start, end)) in edges.iter().zip(&sides) {
        totals[from][start as usize] += 1;
        totals[to][end as usize] += 1;
    }
    let mut used = vec![[0; 4]; layout.nodes.len()];
    edges
        .iter()
        .zip(sides)
        .map(|(&(from, to), (start, end))| {
            let mut port = |node: usize, side: Side| {
                let index = &mut used[node][side as usize];
                let result = measure_port(
                    layout.nodes[node],
                    side,
                    *index,
                    totals[node][side as usize],
                );
                *index += 1;
                result
            };
            (port(from, start), port(to, end))
        })
        .collect()
}

fn measure_port(rect: Rect, side: Side, index: i32, total: i32) -> Port {
    let horizontal = matches!(side, Side::Left | Side::Right);
    let span = if horizontal { rect.height } else { rect.width };
    let spread = (span - 24).min((total - 1) * 16);
    let offset = spread * index / (total - 1).max(1) - spread / 2;
    let center = rect.center();
    let (anchor, dx, dy) = match side {
        Side::Left => (
            Point {
                x: rect.x,
                y: center.y + offset,
            },
            -PORT_MARGIN,
            0,
        ),
        Side::Right => (
            Point {
                x: rect.right(),
                y: center.y + offset,
            },
            PORT_MARGIN,
            0,
        ),
        Side::Top => (
            Point {
                x: center.x + offset,
                y: rect.y,
            },
            0,
            -PORT_MARGIN,
        ),
        Side::Bottom => (
            Point {
                x: center.x + offset,
                y: rect.bottom(),
            },
            0,
            PORT_MARGIN,
        ),
    };
    Port {
        anchor,
        outside: Point {
            x: anchor.x + dx,
            y: anchor.y + dy,
        },
    }
}

struct Router {
    xs: Vec<i32>,
    ys: Vec<i32>,
    blocked: Vec<[bool; 2]>,
    used: Vec<[u16; 2]>,
}

impl Router {
    fn new(layout: &Layout, ports: &[(Port, Port)]) -> Self {
        // Obstacle boundaries and port coordinates form the possible turns in the grid.
        let mut xs = vec![layout.bounds.x, layout.bounds.right()];
        let mut ys = vec![layout.bounds.y, layout.bounds.bottom()];
        for rect in &layout.obstacles {
            xs.extend([rect.x, rect.right()]);
            ys.extend([rect.y, rect.bottom()]);
        }
        for (start, end) in ports {
            xs.extend([start.outside.x, end.outside.x]);
            ys.extend([start.outside.y, end.outside.y]);
        }
        xs.sort_unstable();
        xs.dedup();
        ys.sort_unstable();
        ys.dedup();
        let grid_size = xs.len() * ys.len();
        let mut router = Self {
            xs,
            ys,
            blocked: vec![[false; 2]; grid_size],
            used: vec![[0; 2]; grid_size],
        };
        for rect in &layout.obstacles {
            let left = router.xs.binary_search(&rect.x).unwrap();
            let right = router.xs.binary_search(&rect.right()).unwrap();
            let top = router.ys.binary_search(&rect.y).unwrap();
            let bottom = router.ys.binary_search(&rect.bottom()).unwrap();
            for y in top..=bottom {
                for x in left..=right {
                    let index = y * router.xs.len() + x;
                    if x < right && y > top && y < bottom {
                        router.blocked[index][0] = true;
                    }
                    if y < bottom && x > left && x < right {
                        router.blocked[index][1] = true;
                    }
                }
            }
        }
        router
    }

    fn position(&self, point: Point) -> usize {
        let x = self.xs.binary_search(&point.x).unwrap();
        let y = self.ys.binary_search(&point.y).unwrap();
        y * self.xs.len() + x
    }

    fn point(&self, position: usize) -> Point {
        Point {
            x: self.xs[position % self.xs.len()],
            y: self.ys[position / self.xs.len()],
        }
    }

    fn route(&mut self, start: Port, end: Port) -> Option<Vec<Point>> {
        let mut route = vec![start.anchor];
        let path = self.find_path(start.outside, end.outside)?;
        for pair in path.windows(2) {
            let axis = usize::from(pair[0].abs_diff(pair[1]) != 1);
            self.used[pair[0].min(pair[1])][axis] += 1;
        }
        route.extend(path.into_iter().map(|position| self.point(position)));
        route.push(end.anchor);
        route.dedup();
        let mut simplified = Vec::with_capacity(route.len());
        simplified.push(route[0]);
        for points in route.windows(3) {
            let [previous, current, next] = [points[0], points[1], points[2]];
            if (previous.x == current.x && current.x == next.x
                || previous.y == current.y && current.y == next.y)
                && previous.distance(current) + current.distance(next) == previous.distance(next)
            {
                continue;
            }
            simplified.push(current);
        }
        simplified.push(*route.last().unwrap());
        Some(simplified)
    }

    fn find_path(&self, start: Point, end: Point) -> Option<Vec<usize>> {
        let start = self.position(start);
        let goal = self.position(end);
        let columns = self.xs.len();
        let rows = self.ys.len();
        let mut queue = BinaryHeap::new();
        let distance = self.point(start).distance(end);
        let mut costs = vec![i32::MAX; self.blocked.len() * 2];
        let mut from = vec![usize::MAX; costs.len()];
        for axis in 0..2 {
            let state = start * 2 + axis;
            costs[state] = 0;
            queue.push(Reverse((distance, 0, state)));
        }
        while let Some(Reverse((_, cost, state))) = queue.pop() {
            if cost != costs[state] {
                continue;
            }
            let position = state / 2;
            if position == goal {
                let mut path = Vec::new();
                let mut current = state;
                while current != usize::MAX {
                    path.push(current / 2);
                    current = from[current];
                }
                path.reverse();
                return Some(path);
            }
            let x = position % columns;
            let y = position / columns;
            let neighbors = [
                (x > 0, position.wrapping_sub(1), 0),
                (x + 1 < columns, position + 1, 0),
                (y > 0, position.wrapping_sub(columns), 1),
                (y + 1 < rows, position + columns, 1),
            ];
            let point = self.point(position);
            for (present, next, axis) in neighbors {
                if !present || self.blocked[position.min(next)][axis] {
                    continue;
                }
                let next_point = self.point(next);
                let step = point.distance(next_point);
                let bend = if axis == state % 2 { 0 } else { 26 };
                let reuse = self.used[position.min(next)][axis].min(3) as i32;
                let next_cost = cost + step + bend + reuse * (step / 8 + 2);
                let next_state = next * 2 + axis;
                if next_cost < costs[next_state] {
                    costs[next_state] = next_cost;
                    from[next_state] = state;
                    queue.push(Reverse((
                        next_cost + next_point.distance(end),
                        next_cost,
                        next_state,
                    )));
                }
            }
        }
        None
    }
}

fn draw_group(svg: &mut String, group: &Group, title: &[String], rect: Rect) {
    let (fill, border, ink, kind, dash) = match group.kind.as_str() {
        "trust_boundary" => ("#faf2df", "#ac8849", "#6a4b20", "TRUST BOUNDARY", "7 5"),
        "network" => ("#eaf0f6", "#8da5bc", "#34566e", "NETWORK", "5 4"),
        "environment" => ("#e8efe5", "#98b097", "#3b6551", "ENVIRONMENT", "5 4"),
        "system" => ("#e9f0e8", "#a3b7a2", "#3b6551", "SYSTEM", "0"),
        "zone" => ("#f2edf5", "#b9a8c4", "#654c73", "ZONE", "5 4"),
        _ => ("#eef0ea", "#b2bcb1", "#53675c", "GROUP", "0"),
    };
    let label = group_label_rect(rect, title);
    write!(svg, r#"<g><title>{}</title><rect x="{}" y="{}" width="{}" height="{}" rx="18" fill="{fill}" fill-opacity=".86" stroke="{border}" stroke-width="1.5" stroke-dasharray="{dash}"/><text x="{}" y="{}" fill="{ink}" font-size="10" font-weight="700" letter-spacing="1.15">{kind}</text>"#,
        escaped(&group.label), rect.x, rect.y, rect.width, rect.height, label.x + 6, label.y + 14,
    ).unwrap();
    for (index, line) in title.iter().enumerate() {
        write!(
            svg,
            r#"<text x="{}" y="{}" fill="{ink}" font-size="15" font-weight="700">{}</text>"#,
            label.x + 6,
            label.y + 34 + 18 * index as i32,
            escaped(line),
        )
        .unwrap();
    }
    svg.push_str("</g>");
}

fn group_label_rect(group: Rect, lines: &[String]) -> Rect {
    let text_width = lines
        .iter()
        .map(|line| display_width(line) * 9)
        .max()
        .unwrap_or_default();
    Rect::new(
        group.x + 15,
        group.y + 12,
        (text_width + 14).max(114),
        22 + lines.len() as i32 * 18,
    )
}

fn draw_node(svg: &mut String, node: &Node, text: &NodeText, rect: Rect) {
    let (accent, tint, kind, icon) = node_style(&node.kind);
    let mut full_label = node.label.clone();
    for line in &node.details {
        full_label.push('\n');
        full_label.push_str(line);
    }
    let (x, y) = (rect.x, rect.y);
    write!(svg,
        r##"<g><title>{}</title><rect x="{x}" y="{}" width="{}" height="{}" rx="11" fill="#244631" fill-opacity=".08"/><rect x="{x}" y="{y}" width="{}" height="{}" rx="11" fill="#fffffc" stroke="#cbd7cc"/><rect x="{}" y="{}" width="22" height="21" rx="5" fill="{tint}"/><g transform="translate({} {})" fill="none" stroke="{accent}" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">{icon}</g><text x="{}" y="{}" fill="{accent}" font-size="10" font-weight="700" letter-spacing=".9">{kind}</text>"##,
        escaped(&full_label), y + 3, rect.width, rect.height, rect.width, rect.height,
        x + 14, y + 12, x + 19, y + 16, x + 44, y + 27,
    ).unwrap();
    for (index, line) in text.title.iter().enumerate() {
        write!(
            svg,
            r##"<text x="{}" y="{}" fill="#17362b" font-size="16" font-weight="700">{}</text>"##,
            x + 16,
            y + 54 + 20 * index as i32,
            escaped(line),
        )
        .unwrap();
    }
    for (index, line) in text.details.iter().enumerate() {
        write!(
            svg,
            r##"<text x="{}" y="{}" fill="#4d6559" font-size="12">{}</text>"##,
            x + 16,
            y + 55 + 20 * text.title.len() as i32 + 17 * index as i32,
            escaped(line),
        )
        .unwrap();
    }
    svg.push_str("</g>");
}

fn node_style(kind: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match kind {
        "client" => (
            "#365a85",
            "#eaf1fc",
            "CLIENT",
            r#"<path d="M1 2h10v7H1zM6 9v2M4 11h4"/>"#,
        ),
        "gateway" => (
            "#765426",
            "#fbf1d9",
            "GATEWAY",
            r#"<path d="M1 3h5m2 0h3M1 6h3m2 0h5M1 9h7m2 0h1M7 1v4M5 4v4M9 7v4"/>"#,
        ),
        "service" => (
            "#326451",
            "#e5f1e8",
            "SERVICE",
            r#"<path d="M1 3 6 1l5 2v6l-5 2-5-2zM1 3l5 2 5-2M6 5v6"/>"#,
        ),
        "process" => (
            "#326451",
            "#e5f1e8",
            "PROCESS",
            r#"<path d="M1 3h10v6H1zM3 6l1 1 2-2M7 6h2"/>"#,
        ),
        "compute" => (
            "#326451",
            "#e5f1e8",
            "COMPUTE",
            r#"<path d="M3 3h6v6H3zM1 4h2m6 0h2M1 8h2m6 0h2M4 1v2m4-2v2M4 9v2m4-2v2"/>"#,
        ),
        "database" => (
            "#5c4b81",
            "#f0eafa",
            "DATABASE",
            r#"<ellipse cx="6" cy="3" rx="4.5" ry="2"/><path d="M1.5 3v6c0 2.7 9 2.7 9 0V3m-9 3c0 2.7 9 2.7 9 0"/>"#,
        ),
        "queue" => (
            "#765426",
            "#fbf1d9",
            "QUEUE",
            r#"<path d="M1 3h10M1 6h10M1 9h7"/>"#,
        ),
        "storage" => (
            "#5c4b81",
            "#f0eafa",
            "STORAGE",
            r#"<path d="M1 5h10v5H1zm0 0 2-3h6l2 3M3 8h1"/>"#,
        ),
        "network" => (
            "#365a85",
            "#eaf1fc",
            "NETWORK",
            r#"<path d="M6 5v2m-4 0h8M2 7v2m8-2v2"/><rect x="4" y="1" width="4" height="4" rx=".8"/><rect x=".5" y="9" width="3" height="2"/><rect x="8.5" y="9" width="3" height="2"/>"#,
        ),
        "external" => (
            "#58665b",
            "#edf0ea",
            "EXTERNAL",
            r#"<path d="M6 2H2v8h8V6M7 1h4v4m0-4L5 7"/>"#,
        ),
        _ => (
            "#58665b",
            "#edf0ea",
            "COMPONENT",
            r#"<path d="M2 2h8v8H2zM4 6h4"/>"#,
        ),
    }
}

fn draw_edge(svg: &mut String, route: &[Point], summary: &str) {
    const INK: &str = "#537264";
    let first = route[0];
    let mut path = format!("M{} {}", first.x, first.y);
    for pair in route.windows(2) {
        if pair[0].x == pair[1].x {
            write!(path, "V{}", pair[1].y).unwrap();
        } else {
            write!(path, "H{}", pair[1].x).unwrap();
        }
    }
    let [before, end] = [route[route.len() - 2], route[route.len() - 1]];
    let dx = (end.x - before.x).signum();
    let dy = (end.y - before.y).signum();
    let tail = Point {
        x: end.x - 10 * dx,
        y: end.y - 10 * dy,
    };
    write!(svg, r#"<g><title>{}</title><path d="{path}" fill="none" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/><path d="M{} {}L{} {}L{} {}Z" fill="{INK}"/></g>"#,
        escaped(summary), end.x, end.y, tail.x + 5 * dy, tail.y + 5 * dx, tail.x - 5 * dy, tail.y - 5 * dx,
    ).unwrap();
}

fn place_edge_label(
    bounds: Rect,
    route: &[Point],
    lines: &[String],
    obstacles: &[Rect],
) -> Option<Rect> {
    let width = lines
        .iter()
        .map(|line| {
            line.chars()
                .map(|character| match character {
                    'W' | 'M' | '@' | '%' => 11,
                    'm' | 'w' | 'A'..='Z' => 9,
                    'i' | 'l' | ' ' | '.' | ',' | ':' | ';' | '!' | '|' => 4,
                    _ => character_width(character) * 6,
                })
                .sum::<i32>()
        })
        .max()
        .unwrap_or_default()
        + 16;
    let height = 8 + lines.len() as i32 * 14;
    let mut segments: Vec<_> = route.windows(2).collect();
    segments.sort_by_key(|pair| Reverse(pair[0].distance(pair[1])));
    for pair in &segments {
        let horizontal = pair[0].y == pair[1].y;
        for eighth in [4, 3, 5, 2, 6] {
            let center = Point {
                x: pair[0].x + (pair[1].x - pair[0].x) * eighth / 8,
                y: pair[0].y + (pair[1].y - pair[0].y) * eighth / 8,
            };
            let candidates = if horizontal {
                [
                    (center.x - width / 2, center.y - height - 8),
                    (center.x - width / 2, center.y + 8),
                ]
            } else {
                [
                    (center.x + 8, center.y - height / 2),
                    (center.x - width - 8, center.y - height / 2),
                ]
            };
            for (x, y) in candidates {
                let rect = Rect::new(x, y, width, height);
                if bounds.contains(rect)
                    && obstacles.iter().all(|obstacle| !rect.intersects(*obstacle))
                {
                    return Some(rect);
                }
            }
        }
    }
    None
}

fn draw_edge_label(svg: &mut String, lines: &[String], rect: Rect) {
    write!(svg, r##"<g><rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="#fffffb" stroke="#d5ded2"/>"##, rect.x, rect.y, rect.width, rect.height).unwrap();
    for (index, line) in lines.iter().enumerate() {
        write!(
            svg,
            r##"<text x="{}" y="{}" text-anchor="middle" fill="#3f5f51" font-size="11">{}</text>"##,
            rect.center().x,
            rect.y + 15 + 14 * index as i32,
            escaped(line),
        )
        .unwrap();
    }
    svg.push_str("</g>");
}

fn wrap(text: &str, limit: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut width = 0;
    for word in text.split_whitespace() {
        let word_width = display_width(word);
        if !line.is_empty() && width + 1 + word_width > limit {
            lines.push(std::mem::take(&mut line));
            width = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            width += 1;
        }
        for character in word.chars() {
            let character_width = character_width(character);
            if width + character_width > limit {
                lines.push(std::mem::take(&mut line));
                width = 0;
            }
            line.push(character);
            width += character_width;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn display_width(text: &str) -> i32 {
    text.chars().map(character_width).sum()
}

fn character_width(character: char) -> i32 {
    match character {
        '\u{1100}'..='\u{115f}'
        | '\u{2e80}'..='\u{a4cf}'
        | '\u{ac00}'..='\u{d7af}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff00}'..='\u{ffef}'
        | '\u{1f300}'..='\u{1faff}' => 2,
        _ => 1,
    }
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::{Edge, parse_graph, tests::SAMPLE_GRAPH};

    #[test]
    fn routes_labeled_edges_around_cards_and_boundary_headings() {
        let mut graph = parse_graph(SAMPLE_GRAPH).unwrap();
        graph.edges.push(Edge {
            from: "database".to_owned(),
            to: "database".to_owned(),
            label: Some("Maintenance".to_owned()),
        });
        let edges = [(0, 1), (1, 2), (2, 3), (3, 2), (3, 3)];
        let display = DisplayText::new(&graph);
        let layout = Layout::new(&graph, &display, &edges);
        let ports = measure_ports(&layout, &edges);
        let mut router = Router::new(&layout, &ports);
        for ((from, to), (start, end)) in edges.into_iter().zip(ports) {
            let route = router.route(start, end).unwrap();
            assert_eq!(route.first().copied(), Some(start.anchor));
            assert_eq!(route.last().copied(), Some(end.anchor));
            for pair in route.windows(2) {
                assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
                let extent = Rect::new(
                    pair[0].x.min(pair[1].x),
                    pair[0].y.min(pair[1].y),
                    (pair[0].x - pair[1].x).abs(),
                    (pair[0].y - pair[1].y).abs(),
                );
                for (index, node) in layout.nodes.iter().enumerate() {
                    assert!(
                        !extent.intersects(*node),
                        "edge {from} → {to} crosses node {index}"
                    );
                }
                for (index, rect) in &layout.groups {
                    assert!(!extent.intersects(group_label_rect(*rect, &display.groups[*index])));
                }
            }
        }
        let svg = render_svg(&graph).unwrap();
        assert!(!svg.contains("Connection labels"));
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(layout.width as u32, tree.size().height() as u32)
                .unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert!(pixmap.pixels().iter().any(|pixel| pixel.red() < 45));
    }
}
