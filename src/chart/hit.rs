//! which sector is under a touch point

use super::layout::{ring_radii, Sector, CENTRE};

/// what a point of the chart is on
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// the centre disc
    Centre,
    /// index in the sectors
    Sector(usize),
    /// a gap, or outside the chart
    Nothing,
}

/// distance to a marker's line, along the ring, within which a touch is on it.
/// the chart radius being 1
pub const MARKER_REACH: f64 = 0.035;

/// angle of a point in turns, 0 at the top, growing clockwise. y grows downward
pub fn turn_of(x: f64, y: f64) -> f64 {
    let turn = x.atan2(-y) / std::f64::consts::TAU;
    if turn < 0.0 {
        turn + 1.0
    } else {
        turn
    }
}

/// shortest distance between two angles, in turns
fn turn_distance(a: f64, b: f64) -> f64 {
    let distance = (a - b).abs() % 1.0;
    distance.min(1.0 - distance)
}

/// what is at (`x`, `y`), relative to the centre of the chart, its radius being 1 and y
/// growing downward
pub fn hit(sectors: &[Sector], depth: u32, x: f64, y: f64) -> Hit {
    let radius = (x * x + y * y).sqrt();
    if radius < CENTRE {
        return Hit::Centre;
    }
    if radius > 1.0 {
        return Hit::Nothing;
    }
    let turn = turn_of(x, y);

    // markers have no width: the closest one within reach wins over what is under it
    let mut marker: Option<(usize, f64)> = None;
    for (index, sector) in sectors.iter().enumerate() {
        if !sector.is_marker() {
            continue;
        }
        let (inner, outer) = ring_radii(sector.ring, depth);
        if radius < inner || radius >= outer {
            continue;
        }
        let distance = turn_distance(turn, sector.start) * std::f64::consts::TAU * radius;
        if distance <= MARKER_REACH && marker.map_or(true, |(_, best)| distance < best) {
            marker = Some((index, distance));
        }
    }
    if let Some((index, _)) = marker {
        return Hit::Sector(index);
    }

    for (index, sector) in sectors.iter().enumerate() {
        if sector.is_marker() {
            continue;
        }
        let (inner, outer) = ring_radii(sector.ring, depth);
        if radius >= inner && radius < outer && turn >= sector.start && turn < sector.end {
            return Hit::Sector(index);
        }
    }
    Hit::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::layout::{layout, SectorKind};
    use crate::scan::tree::tests::sample;
    use crate::scan::tree::{Kind, Node, Tree, ROOT};
    use std::path::Path;

    /// point at an angle in turns and a radius
    fn point(turn: f64, radius: f64) -> (f64, f64) {
        let angle = turn * std::f64::consts::TAU;
        (radius * angle.sin(), -radius * angle.cos())
    }

    fn at(sectors: &[Sector], depth: u32, turn: f64, radius: f64) -> Hit {
        let (x, y) = point(turn, radius);
        hit(sectors, depth, x, y)
    }

    fn index(tree: &Tree, sectors: &[Sector], path: &str) -> usize {
        let id = tree.find(Path::new(path)).unwrap();
        sectors.iter().position(|sector| sector.node == id && !sector.is_marker()).unwrap()
    }

    #[test]
    fn turns_start_at_the_top_and_grow_clockwise() {
        assert!(turn_of(0.0, -1.0).abs() < 1e-9);
        assert!((turn_of(1.0, 0.0) - 0.25).abs() < 1e-9);
        assert!((turn_of(0.0, 1.0) - 0.5).abs() < 1e-9);
        assert!((turn_of(-1.0, 0.0) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn centre_and_outside() {
        let tree = sample();
        let sectors = layout(&tree, ROOT, 3);
        assert_eq!(hit(&sectors, 3, 0.0, 0.0), Hit::Centre);
        assert_eq!(at(&sectors, 3, 0.3, CENTRE - 0.01), Hit::Centre);
        assert_eq!(at(&sectors, 3, 0.3, 1.01), Hit::Nothing);
        assert_eq!(hit(&sectors, 3, 5.0, -3.0), Hit::Nothing);
    }

    #[test]
    fn every_ring_gives_its_sector() {
        let tree = sample();
        let depth = 3;
        let sectors = layout(&tree, ROOT, depth);
        // first ring: big 0..0.91, small 0.91..0.97, d 0.97..0.99
        let (inner, outer) = ring_radii(1, depth);
        let middle = (inner + outer) / 2.0;
        assert_eq!(at(&sectors, depth, 0.5, middle), Hit::Sector(index(&tree, &sectors, "/data/big")));
        assert_eq!(at(&sectors, depth, 0.94, middle), Hit::Sector(index(&tree, &sectors, "/data/small")));
        assert_eq!(at(&sectors, depth, 0.98, middle), Hit::Sector(index(&tree, &sectors, "/data/d")));
        // second ring: a 0..0.6, b 0.6..0.9, c 0.91..0.96
        let (inner, outer) = ring_radii(2, depth);
        let middle = (inner + outer) / 2.0;
        assert_eq!(at(&sectors, depth, 0.3, middle), Hit::Sector(index(&tree, &sectors, "/data/big/a")));
        assert_eq!(at(&sectors, depth, 0.7, middle), Hit::Sector(index(&tree, &sectors, "/data/big/b")));
        assert_eq!(at(&sectors, depth, 0.93, middle), Hit::Sector(index(&tree, &sectors, "/data/small/c")));
        // nothing in the third ring
        let (inner, outer) = ring_radii(3, depth);
        assert_eq!(at(&sectors, depth, 0.3, (inner + outer) / 2.0), Hit::Nothing);
    }

    #[test]
    fn gaps_give_nothing() {
        let tree = sample();
        let depth = 2;
        let sectors = layout(&tree, ROOT, depth);
        let (inner, outer) = ring_radii(1, depth);
        // after d, the own size of the current folder
        assert_eq!(at(&sectors, depth, 0.995, (inner + outer) / 2.0), Hit::Nothing);
        let (inner, outer) = ring_radii(2, depth);
        // after b, the own size of big
        assert_eq!(at(&sectors, depth, 0.905, (inner + outer) / 2.0), Hit::Nothing);
        // d is a file, nothing above it
        assert_eq!(at(&sectors, depth, 0.98, (inner + outer) / 2.0), Hit::Nothing);
    }

    #[test]
    fn borders_belong_to_the_sector_that_starts_there() {
        let tree = sample();
        let depth = 1;
        let sectors = layout(&tree, ROOT, depth);
        let big = index(&tree, &sectors, "/data/big");
        let small = index(&tree, &sectors, "/data/small");
        assert_eq!(at(&sectors, depth, 0.91 - 1e-6, 0.6), Hit::Sector(big));
        assert_eq!(at(&sectors, depth, 0.91 + 1e-6, 0.6), Hit::Sector(small));
        assert_eq!(at(&sectors, depth, 1e-6, 0.6), Hit::Sector(big));
        // radial borders
        assert_eq!(at(&sectors, depth, 0.5, CENTRE + 1e-6), Hit::Sector(big));
        assert_eq!(at(&sectors, depth, 0.5, 1.0 - 1e-6), Hit::Sector(big));
    }

    #[test]
    fn markers_are_reached_from_both_sides() {
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let mut locked = Node::new("locked", Kind::Folder, 0);
        locked.unreadable = true;
        tree.add(big, locked);
        tree.compute();
        let depth = 3;
        let sectors = layout(&tree, ROOT, depth);
        let first = sectors.iter().position(|s| s.kind == SectorKind::Unreadable && s.ring == 1).unwrap();
        let second = sectors.iter().position(|s| s.kind == SectorKind::Unreadable && s.ring == 2).unwrap();

        // the first ring marker is at the top (turn 1 = turn 0)
        let (inner, outer) = ring_radii(1, depth);
        let middle = (inner + outer) / 2.0;
        assert_eq!(at(&sectors, depth, 0.004, middle), Hit::Sector(first));
        assert_eq!(at(&sectors, depth, 0.996, middle), Hit::Sector(first));
        // further away it is the sector under
        assert_eq!(at(&sectors, depth, 0.05, middle), Hit::Sector(index(&tree, &sectors, "/data/big")));

        // the second ring marker is at the end of big (0.91), not reachable from the first ring
        let (inner, outer) = ring_radii(2, depth);
        let middle = (inner + outer) / 2.0;
        assert_eq!(at(&sectors, depth, 0.912, middle), Hit::Sector(second));
        assert_eq!(at(&sectors, depth, 0.908, middle), Hit::Sector(second));
        let (inner, outer) = ring_radii(1, depth);
        assert_eq!(
            at(&sectors, depth, 0.912, (inner + outer) / 2.0),
            Hit::Sector(index(&tree, &sectors, "/data/small"))
        );
    }

    #[test]
    fn no_sector_no_hit() {
        assert_eq!(hit(&[], 3, 0.5, 0.5), Hit::Nothing);
        assert_eq!(hit(&[], 3, 0.0, 0.1), Hit::Centre);
    }
}
