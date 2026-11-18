//! placement of the names around the chart and of the arrows pointing to their sectors
//!
//! only the first ring is named: the items of the current folder. coordinates are in pixels,
//! relative to the centre of the chart, y growing downward.
//!
//! in portrait, names are above and below the chart, one per line. the names of the sectors
//! closest to the sides are the furthest from the chart, so that their arrows go around the
//! ones of the sectors closer to the middle. a name on the left half starts at its arrow and
//! goes right, a name on the right half ends at its arrow.
//!
//! in landscape, names are in a column on each side, at the height of their sector when
//! there is room, pushed apart otherwise.

use super::layout::{ring_radii, Sector};

/// the room around the chart
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub landscape: bool,
    /// radius of the chart
    pub radius: f64,
    /// height of a line of text
    pub line: f64,
    /// portrait: half of the width names can use, centred on the chart.
    /// landscape: width of each column of names
    pub width: f64,
    /// landscape: half of the height names can use, centred on the chart
    pub height: f64,
    /// portrait: most lines above the chart, and as many below
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// index of its sector
    pub sector: usize,
    /// where the text starts, or ends when `right`
    pub x: f64,
    /// middle of the line of text
    pub y: f64,
    /// the text is aligned to the right and ends at `x`
    pub right: bool,
    /// most width the text can take
    pub width: f64,
    /// the arrow goes from `attach`, next to the text, to `elbow` on the rim of the chart,
    /// then straight to `tip` in the sector
    pub attach: (f64, f64),
    pub elbow: (f64, f64),
    pub tip: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placement {
    pub labels: Vec<Label>,
    /// portrait: heights taken by the names above and below the chart
    pub above: f64,
    pub below: f64,
}

/// a sector to name
struct Candidate {
    sector: usize,
    span: f64,
    elbow: (f64, f64),
    tip: (f64, f64),
}

/// point of the chart at an angle in turns and a radius
fn point(turn: f64, radius: f64) -> (f64, f64) {
    let angle = turn * std::f64::consts::TAU;
    (radius * angle.sin(), -radius * angle.cos())
}

/// names and arrows for the first ring of `sectors`
pub fn place(sectors: &[Sector], depth: u32, area: &Area) -> Placement {
    let (inner, outer) = ring_radii(1, depth);
    let candidates: Vec<Candidate> = sectors
        .iter()
        .enumerate()
        .filter(|(_, sector)| sector.ring == 1 && !sector.is_marker())
        .map(|(index, sector)| Candidate {
            sector: index,
            span: sector.end - sector.start,
            elbow: point(sector.middle(), area.radius),
            tip: point(sector.middle(), area.radius * (inner + outer) / 2.0),
        })
        .collect();
    if area.landscape {
        landscape(candidates, area)
    } else {
        portrait(candidates, area)
    }
}

/// keep the `most` biggest
fn biggest(mut candidates: Vec<Candidate>, most: usize) -> Vec<Candidate> {
    candidates.sort_by(|a, b| b.span.total_cmp(&a.span));
    candidates.truncate(most);
    candidates
}

fn portrait(candidates: Vec<Candidate>, area: &Area) -> Placement {
    let (above, below): (Vec<Candidate>, Vec<Candidate>) =
        candidates.into_iter().partition(|candidate| candidate.elbow.1 < 0.0);
    let mut placement = Placement::default();
    for (band, direction) in [(above, -1.0), (below, 1.0)] {
        let mut band = biggest(band, area.rows);
        // closest to the sides first: they get the lines furthest from the chart
        band.sort_by(|a, b| b.elbow.0.abs().total_cmp(&a.elbow.0.abs()));
        let count = band.len();
        let pad = area.line * 0.25;
        let apart = area.line * 0.5;
        let edge = area.width - pad;
        // arrows already placed on each half, from the side toward the middle
        let mut left: Vec<f64> = Vec::new();
        let mut right: Vec<f64> = Vec::new();
        for (index, candidate) in band.into_iter().enumerate() {
            let row = count - 1 - index;
            let y = direction * (area.radius + area.line * (row as f64 + 1.0));
            let on_right = candidate.elbow.0 >= 0.0;
            // arrows of a same half keep some distance, moving toward the middle
            let x = if on_right {
                let x = candidate.elbow.0.min(edge);
                right.last().map_or(x, |&previous| x.min(previous - apart))
            } else {
                let x = candidate.elbow.0.max(-edge);
                left.last().map_or(x, |&previous| x.max(previous + apart))
            };
            // the text stops before the arrows that cross its line: the ones placed before
            let label = if on_right {
                let limit = left.iter().chain(right.iter()).copied().filter(|&other| other < x)
                    .fold(-edge, f64::max);
                Label {
                    sector: candidate.sector,
                    x: x - pad,
                    y,
                    right: true,
                    width: (x - pad - (limit + pad)).max(0.0),
                    attach: (x, y),
                    elbow: candidate.elbow,
                    tip: candidate.tip,
                }
            } else {
                let limit = left.iter().chain(right.iter()).copied().filter(|&other| other > x)
                    .fold(edge, f64::min);
                Label {
                    sector: candidate.sector,
                    x: x + pad,
                    y,
                    right: false,
                    width: (limit - pad - (x + pad)).max(0.0),
                    attach: (x, y),
                    elbow: candidate.elbow,
                    tip: candidate.tip,
                }
            };
            if on_right {
                right.push(x);
            } else {
                left.push(x);
            }
            placement.labels.push(label);
        }
        let height = if count == 0 { 0.0 } else { area.line * (count as f64 + 1.0) };
        if direction < 0.0 {
            placement.above = height;
        } else {
            placement.below = height;
        }
    }
    placement
}

fn landscape(candidates: Vec<Candidate>, area: &Area) -> Placement {
    let (left, right): (Vec<Candidate>, Vec<Candidate>) =
        candidates.into_iter().partition(|candidate| candidate.elbow.0 < 0.0);
    let most = (2.0 * area.height / area.line).floor().max(0.0) as usize;
    let lead = area.line;
    let pad = area.line * 0.25;
    let mut placement = Placement::default();
    for (column, direction) in [(left, -1.0), (right, 1.0)] {
        let mut column = biggest(column, most);
        column.sort_by(|a, b| a.elbow.1.total_cmp(&b.elbow.1));
        // at the height of the sector, then pushed down by the previous name,
        // then pushed up by the bottom of the area
        let mut heights: Vec<f64> = Vec::with_capacity(column.len());
        let top = -area.height + area.line / 2.0;
        let bottom = area.height - area.line / 2.0;
        for candidate in &column {
            let wanted = candidate.elbow.1.max(top);
            heights.push(heights.last().map_or(wanted, |&previous| wanted.max(previous + area.line)));
        }
        let mut limit = bottom;
        for height in heights.iter_mut().rev() {
            *height = height.min(limit);
            limit = *height - area.line;
        }
        let x = direction * (area.radius + lead);
        for (candidate, y) in column.into_iter().zip(heights) {
            placement.labels.push(Label {
                sector: candidate.sector,
                x: x + direction * pad,
                y,
                right: direction < 0.0,
                width: (area.width - pad).max(0.0),
                attach: (x, y),
                elbow: candidate.elbow,
                tip: candidate.tip,
            });
        }
    }
    placement
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::layout::layout;
    use crate::scan::tree::{Kind, Node, Tree, ROOT};

    /// sectors of a folder of files with the given sizes
    fn sectors_of(sizes: &[u64]) -> Vec<Sector> {
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        for (index, &size) in sizes.iter().enumerate() {
            tree.add(ROOT, Node::new(format!("file{index:02}"), Kind::File, size));
        }
        let folder = tree.add(ROOT, Node::new("folder", Kind::Folder, 0));
        tree.add(folder, Node::new("inside", Kind::File, 300));
        let mut locked = Node::new("locked", Kind::Folder, 0);
        locked.unreadable = true;
        tree.add(ROOT, locked);
        tree.compute();
        layout(&tree, ROOT, 3)
    }

    fn portrait_area(rows: usize) -> Area {
        Area { landscape: false, radius: 300.0, line: 40.0, width: 340.0, height: 0.0, rows }
    }

    fn landscape_area() -> Area {
        Area { landscape: true, radius: 250.0, line: 40.0, width: 200.0, height: 300.0, rows: 0 }
    }

    /// horizontal extent of the text
    fn extent(label: &Label) -> (f64, f64) {
        if label.right {
            (label.x - label.width, label.x)
        } else {
            (label.x, label.x + label.width)
        }
    }

    #[test]
    fn only_the_first_ring_is_named() {
        let sectors = sectors_of(&[500, 300, 200]);
        let placement = place(&sectors, 3, &portrait_area(6));
        // three files and the folder; not what is inside the folder, not the marker
        assert_eq!(placement.labels.len(), 4);
        for label in &placement.labels {
            assert_eq!(sectors[label.sector].ring, 1);
            assert!(!sectors[label.sector].is_marker());
        }
    }

    #[test]
    fn portrait_names_are_above_and_below_on_their_own_lines() {
        let area = portrait_area(6);
        let sectors = sectors_of(&[300, 250, 200, 150, 100, 80, 60, 40]);
        let placement = place(&sectors, 3, &area);
        assert_eq!(placement.labels.len(), 9);
        let mut lines: Vec<i64> = Vec::new();
        for label in &placement.labels {
            // on the side of the chart its sector is on
            assert_eq!(label.y < 0.0, label.elbow.1 < 0.0);
            assert!(label.y.abs() >= area.radius + area.line);
            let line = (label.y / area.line).round() as i64;
            assert!(!lines.contains(&line), "two names on the same line");
            lines.push(line);
            let (start, end) = extent(label);
            assert!(start >= -area.width && end <= area.width, "{start}..{end}");
            assert!(label.width > 0.0);
        }
        let above = placement.labels.iter().filter(|label| label.y < 0.0).count() as f64;
        let below = placement.labels.iter().filter(|label| label.y > 0.0).count() as f64;
        assert_eq!(placement.above, area.line * (above + 1.0));
        assert_eq!(placement.below, area.line * (below + 1.0));
        for label in &placement.labels {
            let limit = if label.y < 0.0 { placement.above } else { placement.below };
            assert!(label.y.abs() + area.line / 2.0 <= area.radius + limit);
        }
    }

    #[test]
    fn portrait_arrows_of_the_sides_go_around_the_others() {
        let area = portrait_area(8);
        let sectors = sectors_of(&[300, 250, 200, 150, 100, 80, 60, 40]);
        let placement = place(&sectors, 3, &area);
        for band in [-1.0, 1.0] {
            let labels: Vec<&Label> =
                placement.labels.iter().filter(|label| label.y * band > 0.0).collect();
            for a in &labels {
                for b in &labels {
                    if a.y.abs() > b.y.abs() {
                        // a is further from the chart: its arrow is further from the middle
                        assert!(a.attach.0.abs() >= b.attach.0.abs() - area.line * 0.5 - 1e-9);
                        // and does not go through the text of b
                        let (start, end) = extent(b);
                        assert!(
                            a.attach.0 <= start + 1e-9 || a.attach.0 >= end - 1e-9,
                            "arrow at {} crosses the text {start}..{end}",
                            a.attach.0
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn portrait_text_goes_away_from_its_arrow_toward_the_middle() {
        let area = portrait_area(8);
        let sectors = sectors_of(&[300, 250, 200, 150]);
        let placement = place(&sectors, 3, &area);
        for label in &placement.labels {
            assert_eq!(label.right, label.attach.0 >= 0.0);
            if label.right {
                assert!(label.x < label.attach.0);
            } else {
                assert!(label.x > label.attach.0);
            }
            assert_eq!(label.attach.1, label.y);
        }
    }

    #[test]
    fn portrait_keeps_the_biggest_when_lines_are_missing() {
        let area = portrait_area(2);
        let sectors = sectors_of(&[100; 20]);
        let placement = place(&sectors, 3, &area);
        let above = placement.labels.iter().filter(|label| label.y < 0.0).count();
        let below = placement.labels.iter().filter(|label| label.y > 0.0).count();
        assert_eq!((above, below), (2, 2));

        // the folder (300) is the biggest and at the top right: it is kept
        let folder = sectors.iter().position(|sector| sector.folder).unwrap();
        assert!(placement.labels.iter().any(|label| label.sector == folder));
        assert_eq!(placement.above, area.line * 3.0);
    }

    #[test]
    fn arrows_point_inside_their_sector() {
        for area in [portrait_area(6), landscape_area()] {
            let sectors = sectors_of(&[300, 250, 200, 150]);
            let placement = place(&sectors, 3, &area);
            for label in &placement.labels {
                let sector = &sectors[label.sector];
                let radius = (label.elbow.0.powi(2) + label.elbow.1.powi(2)).sqrt();
                assert!((radius - area.radius).abs() < 1e-6);
                let turn = crate::chart::hit::turn_of(label.tip.0, label.tip.1);
                assert!(turn > sector.start && turn < sector.end);
                let found = crate::chart::hit::hit(
                    &sectors,
                    3,
                    label.tip.0 / area.radius,
                    label.tip.1 / area.radius,
                );
                assert_eq!(found, crate::chart::hit::Hit::Sector(label.sector));
            }
        }
    }

    #[test]
    fn landscape_names_are_in_columns_without_overlap() {
        let area = landscape_area();
        let sectors = sectors_of(&[300, 250, 200, 150, 100, 80, 60, 40, 40, 40, 40]);
        let placement = place(&sectors, 3, &area);
        assert_eq!(placement.labels.len(), 12);
        assert_eq!((placement.above, placement.below), (0.0, 0.0));
        for side in [-1.0, 1.0] {
            let mut column: Vec<&Label> =
                placement.labels.iter().filter(|label| label.attach.0 * side > 0.0).collect();
            assert!(!column.is_empty());
            column.sort_by(|a, b| a.y.total_cmp(&b.y));
            for pair in column.windows(2) {
                assert!(pair[1].y - pair[0].y >= area.line - 1e-9);
                // same order as the sectors, so arrows do not cross
                assert!(pair[1].elbow.1 >= pair[0].elbow.1);
            }
            for label in &column {
                assert_eq!(label.elbow.0 * side >= 0.0, true);
                assert!(label.y - area.line / 2.0 >= -area.height - 1e-9);
                assert!(label.y + area.line / 2.0 <= area.height + 1e-9);
                assert_eq!(label.right, side < 0.0);
                let (start, end) = extent(label);
                assert!(start.abs() >= area.radius && end.abs() >= area.radius);
                assert!(start.abs() <= area.radius + area.line + area.width + 1e-9);
                assert!(end.abs() <= area.radius + area.line + area.width + 1e-9);
            }
        }
    }

    #[test]
    fn landscape_keeps_the_biggest_when_lines_are_missing() {
        let mut area = landscape_area();
        area.height = 60.0; // three lines
        let sectors = sectors_of(&[100; 30]);
        let placement = place(&sectors, 3, &area);
        for side in [-1.0, 1.0] {
            let column: Vec<&Label> =
                placement.labels.iter().filter(|label| label.attach.0 * side > 0.0).collect();
            assert_eq!(column.len(), 3);
        }
    }

    #[test]
    fn nothing_to_name() {
        let placement = place(&[], 3, &portrait_area(6));
        assert!(placement.labels.is_empty());
        assert_eq!((placement.above, placement.below), (0.0, 0.0));
        assert!(place(&[], 3, &landscape_area()).labels.is_empty());
    }
}
