use std::{fmt::Write, ops::Range};

// A recognized box border may sit between a connector and its endpoint.
const BORDER: char = '\0';

pub fn convert(text: &str, kind: &str) -> Result<String, String> {
    let grid = Grid::new(text)?;
    match kind {
        "sequence" => sequence(grid),
        "flowchart" | "swimlane" | "architecture" => flowchart(grid, kind),
        _ => Err("Choose Flowchart, Sequence, Swimlane, or Architecture for this region.".into()),
    }
}

struct Grid {
    rows: Vec<Vec<char>>,
}

impl Grid {
    fn new(text: &str) -> Result<Self, String> {
        let rows: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
        if rows.iter().flatten().any(|ch| ch.is_control()) {
            return Err(
                "Use spaces for alignment and remove control characters from the region.".into(),
            );
        }
        Ok(Self { rows })
    }

    fn at(&self, y: usize, x: usize) -> char {
        self.rows[y].get(x).copied().unwrap_or(' ')
    }

    fn clear(&mut self, y: usize, columns: Range<usize>) {
        for x in columns {
            if let Some(ch) = self.rows[y].get_mut(x).filter(|ch| **ch != BORDER) {
                *ch = ' ';
            }
        }
    }

    fn check_consumed(&self, hint: &str) -> Result<(), String> {
        if let Some((y, x)) = self.rows.iter().enumerate().find_map(|(y, row)| {
            row.iter()
                .position(|ch| *ch != BORDER && !ch.is_whitespace())
                .map(|x| (y, x))
        }) {
            return Err(format!(
                "Could not read line {}, column {}. {hint}",
                y + 1,
                x + 1
            ));
        }
        Ok(())
    }

    fn nodes(&mut self, rows: Range<usize>) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        for (y, row) in self
            .rows
            .iter_mut()
            .enumerate()
            .take(rows.end)
            .skip(rows.start)
        {
            let mut start = None;
            let mut depth = 0;
            for x in 0..row.len() {
                match row[x] {
                    '[' => {
                        start.get_or_insert(x);
                        depth += 1;
                    }
                    ']' if depth > 0 => {
                        depth -= 1;
                        if depth == 0 {
                            let left = start.take().expect("a closing bracket has a start");
                            let label: String = row[left + 1..x].iter().collect();
                            let label = label.trim();
                            if label.is_empty() || label.contains(BORDER) {
                                return Err(format!(
                                    "Give the bracketed item on line {} a label.",
                                    y + 1
                                ));
                            }
                            nodes.push(Node {
                                y,
                                left,
                                right: x,
                                label: label.to_owned(),
                            });
                            row[left..=x].fill(' ');
                        }
                    }
                    ']' => {
                        return Err(format!(
                            "Match the closing bracket on line {} with an opening bracket.",
                            y + 1
                        ));
                    }
                    _ => {}
                }
            }
            if depth != 0 {
                return Err(format!("Close the bracketed label on line {}.", y + 1));
            }
        }
        if nodes.is_empty() {
            return Err("Add a bracketed label such as [Start] inside the region.".into());
        }
        Ok(nodes)
    }

    fn groups(&mut self) -> Result<Vec<Group>, String> {
        let mut groups = Vec::new();
        for top in 0..self.rows.len() {
            let header = &self.rows[top];
            let mut left = 0;
            let mut candidates = Vec::new();
            while let Some(offset) = header[left..]
                .windows(3)
                .position(|part| part == ['+', '-', '-'])
            {
                if header[left..left + offset]
                    .iter()
                    .any(|ch| !ch.is_whitespace())
                {
                    break;
                }
                left += offset;
                let Some(width) = header[left + 3..]
                    .windows(3)
                    .position(|part| part == ['-', '-', '+'])
                else {
                    break;
                };
                let top_right = left + 5 + width;
                let title: String = header[left + 3..top_right - 2].iter().collect();
                let title = title.trim_matches('-').trim();
                if !title.is_empty() {
                    candidates.push((left, top_right, title.to_owned()));
                }
                left = top_right + 1;
            }
            for (left, top_right, title) in candidates {
                let match_right = |grid: &Self, y, marker| {
                    [top_right, top_right.saturating_sub(1), top_right + 1]
                        .into_iter()
                        .find(|&x| grid.at(y, x) == marker)
                };
                let mut bottom = top + 1;
                let mut sides = Vec::new();
                while bottom < self.rows.len() && self.at(bottom, left) == '|' {
                    let Some(right) = match_right(self, bottom, '|') else {
                        break;
                    };
                    sides.push(right);
                    bottom += 1;
                }
                let bottom_right = (bottom < self.rows.len() && self.at(bottom, left) == '+')
                    .then(|| match_right(self, bottom, '+'))
                    .flatten()
                    .filter(|&right| (left + 1..right).all(|x| self.at(bottom, x) == '-'));
                let Some(bottom_right) = bottom_right else {
                    return Err(format!(
                        "Close the '{title}' group with aligned | sides and a +---+ bottom."
                    ));
                };
                self.rows[top][left..=top_right].fill(BORDER);
                self.rows[bottom][left..=bottom_right].fill(BORDER);
                for (y, right) in (top + 1..bottom).zip(sides.iter().copied()) {
                    self.rows[y][left] = BORDER;
                    self.rows[y][right] = BORDER;
                }
                groups.push(Group {
                    title,
                    top,
                    bottom,
                    left,
                    sides,
                });
            }
        }
        Ok(groups)
    }
}

struct Node {
    y: usize,
    left: usize,
    right: usize,
    label: String,
}

struct Group {
    title: String,
    top: usize,
    bottom: usize,
    left: usize,
    sides: Vec<usize>,
}

impl Group {
    fn contains(&self, node: &Node) -> bool {
        node.y > self.top
            && node.y < self.bottom
            && node.left > self.left
            && node.right < self.sides[node.y - self.top - 1]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
    Both,
}

struct Arrow<'a> {
    direction: Direction,
    label: &'a str,
}

fn horizontal_arrow(text: &str) -> Option<Arrow<'_>> {
    let text = text.trim();
    let left = text.starts_with('<');
    let right = text.ends_with('>');
    let direction = match (left, right) {
        (false, true) => Direction::Forward,
        (true, false) => Direction::Backward,
        (true, true) => Direction::Both,
        (false, false) => return None,
    };
    let body = &text[usize::from(left)..text.len() - usize::from(right)];
    if !body.starts_with('-') || !body.ends_with('-') {
        return None;
    }
    let interior = body.trim_matches('-');
    let label = interior.trim();
    if !label.is_empty() && !(interior.starts_with(' ') && interior.ends_with(' ')) {
        return None;
    }
    Some(Arrow { direction, label })
}

fn flowchart(mut grid: Grid, kind: &str) -> Result<String, String> {
    let groups = grid.groups()?;
    if kind == "swimlane" && groups.is_empty() {
        return Err("Draw each swimlane as a +-- Team --+ box around its bracketed steps.".into());
    }
    let nodes = grid.nodes(0..grid.rows.len())?;
    let mut edges = Vec::new();
    for (i, pair) in nodes.windows(2).enumerate() {
        let [left, right] = pair else { unreachable!() };
        if left.y != right.y {
            continue;
        }
        let columns = left.right + 1..right.left;
        let span: String = columns
            .clone()
            .map(|x| grid.at(left.y, x))
            .filter(|ch| *ch != BORDER)
            .collect();
        if let Some(arrow) = horizontal_arrow(&span) {
            edges.push((i, i + 1, arrow.direction, arrow.label.to_owned()));
            grid.clear(left.y, columns);
        }
    }

    let horizontal_edges = edges.len();
    let arrowheads: Vec<_> = grid
        .rows
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter().enumerate().filter_map(move |(x, ch)| match ch {
                'v' => Some((y, x, Direction::Forward)),
                '^' => Some((y, x, Direction::Backward)),
                _ => None,
            })
        })
        .collect();
    for (y, x, direction) in arrowheads {
        let stem_above = direction == Direction::Forward;
        let above = (0..y).rev().find(|&row| {
            let ch = grid.at(row, x);
            ch != BORDER && !(stem_above && ch == '|')
        });
        let below = (y + 1..grid.rows.len()).find(|&row| {
            let ch = grid.at(row, x);
            ch != BORDER && !(!stem_above && ch == '|')
        });
        let (Some(above), Some(below)) = (above, below) else {
            continue;
        };
        let (Some(from), Some(to)) = (node_at(&nodes, above, x), node_at(&nodes, below, x)) else {
            continue;
        };
        edges.push((from, to, direction, String::new()));
        for y in above + 1..below {
            grid.clear(y, x..x + 1);
        }
    }

    grid.check_consumed(
        "Use [Node] --> [Node] or connect vertically aligned nodes with | and v or ^.",
    )?;

    let mut output = String::from(if edges.len() == horizontal_edges {
        "flowchart LR\n"
    } else {
        "flowchart TD\n"
    });
    let mut group_membership = vec![None; nodes.len()];
    for (i, group) in groups.iter().enumerate() {
        writeln!(output, "    subgraph g{i}[\"{}\"]", escape(&group.title)).unwrap();
        writeln!(output, "        direction LR").unwrap();
        for (j, node) in nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| group.contains(node))
        {
            if group_membership[j].replace(i).is_some() {
                return Err("Place each bracketed node inside one group. Nested or overlapping groups are unsupported.".into());
            }
            writeln!(output, "        n{j}[\"{}\"]", escape(&node.label)).unwrap();
        }
        writeln!(output, "    end").unwrap();
    }
    for (i, node) in nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| group_membership[*i].is_none())
    {
        writeln!(output, "    n{i}[\"{}\"]", escape(&node.label)).unwrap();
    }
    for (from, to, direction, label) in edges {
        let (from, to, arrow) = match direction {
            Direction::Forward => (from, to, "-->"),
            Direction::Backward => (to, from, "-->"),
            Direction::Both => (from, to, "<-->"),
        };
        if label.is_empty() {
            writeln!(output, "    n{from} {arrow} n{to}").unwrap();
        } else {
            writeln!(output, "    n{from} {arrow}|\"{}\"| n{to}", escape(&label)).unwrap();
        }
    }
    Ok(output)
}

fn node_at(nodes: &[Node], y: usize, x: usize) -> Option<usize> {
    let index = nodes.partition_point(|node| (node.y, node.right) < (y, x));
    nodes
        .get(index)
        .filter(|node| node.y == y && node.left <= x)
        .map(|_| index)
}

fn sequence(mut grid: Grid) -> Result<String, String> {
    let header = grid
        .rows
        .iter()
        .position(|row| row.iter().any(|ch| !ch.is_whitespace()))
        .ok_or("Put the bracketed sequence participants on a header row.")?;
    let participants = grid.nodes(header..header + 1)?;
    let mut output = String::from("sequenceDiagram\n");
    for (i, node) in participants.iter().enumerate() {
        writeln!(
            output,
            "    participant p{i} as {}",
            sequence_escape(&node.label)
        )
        .unwrap();
    }
    let lifelines = grid.rows.iter().skip(header + 1).find_map(|row| {
        let columns: Vec<_> = row
            .iter()
            .enumerate()
            .filter_map(|(x, ch)| (*ch == '|').then_some(x))
            .collect();
        if columns.len() != participants.len() {
            return None;
        }
        for (i, &x) in columns.iter().enumerate() {
            let distance = |node: &Node| x.abs_diff(x.clamp(node.left, node.right));
            let own_distance = distance(&participants[i]);
            if [i.checked_sub(1), Some(i + 1)]
                .into_iter()
                .flatten()
                .filter_map(|neighbor| participants.get(neighbor))
                .any(|other| distance(other) <= own_distance)
            {
                return None;
            }
        }
        Some(columns)
    });
    let Some(lifelines) = lifelines else {
        grid.check_consumed("Align a | lifeline under each bracketed participant.")?;
        return Ok(output);
    };

    for y in header + 1..grid.rows.len() {
        let bars: Vec<_> = lifelines
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, x)| grid.at(y, *x) == '|')
            .collect();
        for &(_, x) in &bars {
            grid.clear(y, x..x + 1);
        }
        for pair in bars.windows(2) {
            let [(left, start), (right, end)] = pair else {
                unreachable!()
            };
            let columns = start + 1..*end;
            let span: String = columns.clone().map(|x| grid.at(y, x)).collect();
            if let Some(arrow) = horizontal_arrow(&span) {
                let (from, to) = match arrow.direction {
                    Direction::Forward => (left, right),
                    Direction::Backward => (right, left),
                    Direction::Both => {
                        return Err(format!(
                            "Draw each message on line {} with one arrowhead.",
                            y + 1
                        ));
                    }
                };
                writeln!(
                    output,
                    "    p{from}->>p{to}: {}",
                    sequence_escape(arrow.label)
                )
                .unwrap();
                grid.clear(y, columns);
            }
        }
    }
    grid.check_consumed("Draw each message between two | lifelines with --> or <--.")?;
    Ok(output)
}

fn escape(label: &str) -> String {
    let mut escaped = String::new();
    for ch in label.chars() {
        if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.' | ',' | '!') {
            escaped.push(ch);
        } else {
            write!(escaped, "#{};", u32::from(ch)).unwrap();
        }
    }
    escaped
}

fn sequence_escape(label: &str) -> String {
    // Sequence labels are unquoted. Encode the reserved token as an entity.
    let mut escaped = String::new();
    for part in label.split_inclusive(' ') {
        if part.trim_end_matches(' ') == "end" {
            escaped.push_str("#101;");
            escaped.push_str(&part[1..]);
        } else {
            escaped.push_str(&escape(part));
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::convert;

    #[test]
    fn converts_shipped_templates_with_their_actual_connections() {
        for (kind, ascii) in crate::templates::conversion_templates() {
            let result = convert(ascii, kind).unwrap_or_else(|error| panic!("{kind}: {error}"));
            let expected: &[&str] = match kind {
                "flowchart" => &["n0[\"Request\"]", "n0 --> n1", "n1 --> n2"],
                "sequence" => &[
                    "participant p0 as Client",
                    "p0->>p1: Send request",
                    "p1->>p2: Read",
                    "p2->>p1: Data",
                    "p1->>p0: Response",
                ],
                "swimlane" => &[
                    "subgraph g0[\"Customer\"]",
                    "subgraph g1[\"Support\"]",
                    "n0 --> n1",
                    "n2 --> n3",
                    "n0 --> n2",
                ],
                "architecture" => &[
                    "subgraph g0[\"Private network\"]",
                    "n1[\"Gateway\"]",
                    "n0[\"Browser\"]",
                    "n0 --> n1",
                    "n1 --> n2",
                    "n2 --> n3",
                ],
                _ => panic!("Unknown template {kind}"),
            };
            let mut position = 0;
            for part in expected {
                position += result[position..]
                    .find(part)
                    .unwrap_or_else(|| panic!("{kind} is missing {part}:\n{result}"))
                    + part.len();
            }
            let arrow_count = if kind == "sequence" {
                result.matches("->>").count()
            } else {
                result.matches("-->").count()
            };
            assert_eq!(
                arrow_count,
                match kind {
                    "flowchart" => 2,
                    "sequence" => 4,
                    _ => 3,
                }
            );
        }
    }

    #[test]
    fn preserves_directions_and_escapes_edited_labels() {
        let graph = convert("[First #1] <-- Yes --> [Second \"item\"]", "flowchart").unwrap();
        assert!(graph.contains("n0[\"First #35;1\"]"));
        assert!(graph.contains("n1[\"Second #34;item#34;\"]"));
        assert!(graph.contains("n0 <-->|\"Yes\"| n1"));

        let sequence = convert(
            "[end]                    [Store]\n  |                         |\n  |---- Send [one]; %% ---->|",
            "sequence",
        ).unwrap();
        assert!(sequence.contains("participant p0 as #101;nd"));
        assert!(sequence.contains("p0->>p1: Send #91;one#93;#59; #37;#37;"));
    }

    #[test]
    fn disconnected_and_ambiguous_shapes_never_gain_links() {
        let disconnected = convert("[A]     [B]", "flowchart").unwrap();
        assert!(!disconnected.contains("-->"));
        assert!(convert("[A] -->     ", "flowchart").is_err());
        assert!(convert("[A] --+\n      |\n     [B]", "flowchart").is_err());
        assert!(convert("[A]\n |\n v\n\n   [B]", "flowchart").is_err());
    }
}
