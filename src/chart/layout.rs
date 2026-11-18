//! size tree to sectors: angles, rings, depth limit, the 1% display threshold,
//! zero-width markers for what could not be read

use crate::scan::tree::{NodeId, Tree};

/// share of the full circle under which an item is not drawn and leaves a gap
pub const THRESHOLD: f64 = 0.01;

/// radius of the centre disc, the chart radius being 1
pub const CENTRE: f64 = 0.24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorKind {
    /// a file or folder
    Item,
    /// zero-width marker: something in the folder could not be read
    Unreadable,
    /// zero-width marker: the folder contains mount points, and nothing unreadable
    Mount,
}

/// a piece of ring. angles are in turns, 0 at the top, growing clockwise
#[derive(Debug, Clone, PartialEq)]
pub struct Sector {
    /// the item, or for a marker the folder it is about
    pub node: NodeId,
    /// 1 for the content of the current folder, 2 for the content of its folders...
    pub ring: u32,
    pub start: f64,
    pub end: f64,
    pub kind: SectorKind,
    /// index of the first-ring sector this one is under, which gives its color
    pub group: u32,
    pub folder: bool,
}

impl Sector {
    pub fn is_marker(&self) -> bool {
        self.kind != SectorKind::Item
    }

    pub fn middle(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// inner and outer radius of a ring, the chart radius being 1
pub fn ring_radii(ring: u32, depth: u32) -> (f64, f64) {
    let depth = depth.max(1) as f64;
    let thickness = (1.0 - CENTRE) / depth;
    let inner = CENTRE + thickness * (ring.max(1) - 1) as f64;
    (inner, inner + thickness)
}

/// sectors showing the content of `root`, down to `depth` rings.
///
/// items are placed biggest first from the start of their folder's sector. what is under the
/// threshold is left out, as a gap at the end of the folder's sector
pub fn layout(tree: &Tree, root: NodeId, depth: u32) -> Vec<Sector> {
    let mut sectors = Vec::new();
    place(tree, root, 1, depth.max(1), 0.0, 1.0, None, &mut sectors);
    sectors
}

#[allow(clippy::too_many_arguments)]
fn place(
    tree: &Tree,
    folder: NodeId,
    ring: u32,
    depth: u32,
    start: f64,
    end: f64,
    group: Option<u32>,
    sectors: &mut Vec<Sector>,
) {
    if ring > depth {
        return;
    }
    let node = tree.node(folder);
    if node.total > 0 {
        let mut children: Vec<NodeId> = node.children.clone();
        children.sort_by(|&a, &b| {
            let (a, b) = (tree.node(a), tree.node(b));
            b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name))
        });
        let scale = (end - start) / node.total as f64;
        let mut cursor = start;
        let mut next_group = 0;
        for child in children {
            let child_node = tree.node(child);
            let span = child_node.total as f64 * scale;
            if span < THRESHOLD {
                // the next ones are smaller still
                break;
            }
            let group = group.unwrap_or(next_group);
            next_group += 1;
            // rounding must not let the last sector pass the end of its folder
            let child_end = (cursor + span).min(end);
            sectors.push(Sector {
                node: child,
                ring,
                start: cursor,
                end: child_end,
                kind: SectorKind::Item,
                group,
                folder: child_node.is_folder(),
            });
            if child_node.is_folder() {
                place(tree, child, ring + 1, depth, cursor, child_end, Some(group), sectors);
            }
            cursor = child_end;
        }
    }
    if node.blocked > 0 || node.mounts > 0 {
        sectors.push(Sector {
            node: folder,
            ring,
            start: end,
            end,
            kind: if node.blocked > 0 { SectorKind::Unreadable } else { SectorKind::Mount },
            group: group.unwrap_or(0),
            folder: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::tests::sample;
    use crate::scan::tree::{Kind, Node, ROOT};
    use std::path::Path;

    const EPSILON: f64 = 1e-9;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < EPSILON
    }

    fn name(tree: &Tree, sector: &Sector) -> String {
        tree.node(sector.node).name.to_string_lossy().into_owned()
    }

    fn sector<'a>(tree: &Tree, sectors: &'a [Sector], path: &str) -> &'a Sector {
        let id = tree.find(Path::new(path)).unwrap();
        sectors
            .iter()
            .find(|sector| sector.node == id && !sector.is_marker())
            .unwrap_or_else(|| panic!("no sector for {path}"))
    }

    #[test]
    fn first_ring_is_proportional_and_biggest_first() {
        let tree = sample();
        let sectors = layout(&tree, ROOT, 1);
        let names: Vec<String> = sectors.iter().map(|sector| name(&tree, sector)).collect();
        assert_eq!(names, ["big", "small", "d"]);
        assert!(close(sectors[0].start, 0.0));
        assert!(close(sectors[0].end, 0.91));
        assert!(close(sectors[1].start, 0.91));
        assert!(close(sectors[1].end, 0.97));
        assert!(close(sectors[2].end, 0.99));
        assert!(sectors.iter().all(|sector| sector.ring == 1));
        let groups: Vec<u32> = sectors.iter().map(|sector| sector.group).collect();
        assert_eq!(groups, [0, 1, 2]);
        assert!(sectors[0].folder && sectors[1].folder && !sectors[2].folder);
    }

    #[test]
    fn inner_content_stays_inside_its_folder() {
        let tree = sample();
        let sectors = layout(&tree, ROOT, 3);
        let big = sector(&tree, &sectors, "/data/big");
        let a = sector(&tree, &sectors, "/data/big/a");
        let b = sector(&tree, &sectors, "/data/big/b");
        assert_eq!((a.ring, b.ring), (2, 2));
        assert!(close(a.start, big.start));
        assert!(close(a.end, 0.60));
        assert!(close(b.start, a.end));
        assert!(close(b.end, 0.90));
        // the folder's own size is the gap left at the end
        assert!(b.end < big.end);
        assert_eq!((a.group, b.group), (big.group, big.group));

        let small = sector(&tree, &sectors, "/data/small");
        let c = sector(&tree, &sectors, "/data/small/c");
        assert!(close(c.start, small.start));
        assert!(close(c.end, small.start + 0.05));
        assert_eq!(c.group, small.group);
        assert_eq!(sectors.len(), 6);
    }

    #[test]
    fn depth_limits_the_rings() {
        let tree = sample();
        assert_eq!(layout(&tree, ROOT, 1).len(), 3);
        assert_eq!(layout(&tree, ROOT, 2).len(), 6);
        // nothing deeper than the tree
        assert_eq!(layout(&tree, ROOT, 5).len(), 6);
        // a depth of 0 is taken as 1
        assert_eq!(layout(&tree, ROOT, 0).len(), 3);
    }

    #[test]
    fn items_under_the_threshold_leave_a_gap() {
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        tree.add(ROOT, Node::new("most", Kind::File, 9_850));
        tree.add(ROOT, Node::new("limit", Kind::File, 100));
        for index in 0..5 {
            tree.add(ROOT, Node::new(format!("tiny{index}"), Kind::File, 10));
        }
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        let names: Vec<String> = sectors.iter().map(|sector| name(&tree, sector)).collect();
        // exactly 1% is drawn, less is not
        assert_eq!(names, ["most", "limit"]);
        assert!(close(sectors[1].end, 0.995));
    }

    #[test]
    fn threshold_is_a_share_of_the_full_circle() {
        // half of a folder that is 1.5% of the circle is under the threshold
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        tree.add(ROOT, Node::new("most", Kind::File, 9_850));
        let folder = tree.add(ROOT, Node::new("folder", Kind::Folder, 0));
        tree.add(folder, Node::new("x", Kind::File, 75));
        tree.add(folder, Node::new("y", Kind::File, 75));
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        assert_eq!(sectors.len(), 2);
    }

    #[test]
    fn a_hundred_equal_items_all_fit() {
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        for index in 0..100 {
            tree.add(ROOT, Node::new(format!("file{index:03}"), Kind::File, 4096));
        }
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        assert_eq!(sectors.len(), 100);
        assert!(close(sectors[99].end, 1.0));
        assert!(sectors.iter().all(|sector| sector.end <= 1.0));
        // equal sizes are ordered by name
        assert_eq!(name(&tree, &sectors[0]), "file000");
        assert_eq!(name(&tree, &sectors[99]), "file099");
        for pair in sectors.windows(2) {
            assert!(close(pair[0].end, pair[1].start));
        }
    }

    #[test]
    fn an_empty_folder_has_no_sector() {
        let tree = Tree::new("/data", Node::new("data", Kind::Folder, 4096));
        assert!(layout(&tree, ROOT, 3).is_empty());
        let nothing = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        assert!(layout(&nothing, ROOT, 3).is_empty());
    }

    #[test]
    fn a_single_file_takes_the_circle() {
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 0));
        tree.add(ROOT, Node::new("only", Kind::File, 123));
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        assert_eq!(sectors.len(), 1);
        assert!(close(sectors[0].start, 0.0) && close(sectors[0].end, 1.0));
    }

    #[test]
    fn folders_with_unreadable_content_get_a_marker_at_their_end() {
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let mut locked = Node::new("locked", Kind::Folder, 0);
        locked.unreadable = true;
        tree.add(big, locked);
        tree.compute();

        let sectors = layout(&tree, ROOT, 3);
        let markers: Vec<&Sector> = sectors.iter().filter(|sector| sector.is_marker()).collect();
        // one in the first ring for the current folder, one in the second for `big`.
        // `locked` itself is too small to be drawn, so nothing in the third
        assert_eq!(markers.len(), 2);
        let first = markers.iter().find(|marker| marker.ring == 1).unwrap();
        assert_eq!(first.node, ROOT);
        assert_eq!(first.kind, SectorKind::Unreadable);
        assert!(close(first.start, 1.0) && close(first.end, 1.0));
        let second = markers.iter().find(|marker| marker.ring == 2).unwrap();
        assert_eq!(second.node, big);
        let big_sector = sector(&tree, &sectors, "/data/big");
        assert!(close(second.start, big_sector.end) && close(second.end, big_sector.end));
        assert_eq!(second.group, big_sector.group);
    }

    #[test]
    fn mount_points_get_a_marker_of_their_own_kind() {
        let mut tree = sample();
        let small = tree.find(Path::new("/data/small")).unwrap();
        let mut card = Node::new("card", Kind::Folder, 0);
        card.mount = true;
        tree.add(small, card);
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        let markers: Vec<&Sector> = sectors.iter().filter(|sector| sector.is_marker()).collect();
        assert_eq!(markers.len(), 2);
        assert!(markers.iter().all(|marker| marker.kind == SectorKind::Mount));

        // unreadable content takes over the marker
        tree.node_mut(small).failed.push("ghost".into());
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        assert!(sectors
            .iter()
            .filter(|sector| sector.is_marker())
            .all(|marker| marker.kind == SectorKind::Unreadable));
    }

    #[test]
    fn an_unreadable_empty_folder_still_shows_its_marker() {
        let mut root = Node::new("data", Kind::Folder, 0);
        root.unreadable = true;
        let mut tree = Tree::new("/data", root);
        tree.compute();
        let sectors = layout(&tree, ROOT, 3);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0].kind, SectorKind::Unreadable);
    }

    #[test]
    fn layout_can_start_from_a_folder_inside() {
        let tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let sectors = layout(&tree, big, 3);
        assert_eq!(sectors.len(), 2);
        assert!(close(sectors[0].end, 600.0 / 910.0));
        assert_eq!(sectors[0].ring, 1);
    }

    #[test]
    fn rings_share_the_radius_outside_the_centre() {
        let (inner, outer) = ring_radii(1, 3);
        assert!(close(inner, CENTRE));
        let (next, last) = ring_radii(3, 3);
        assert!(close(last, 1.0));
        assert!(close(outer - inner, last - next));
        let (inner, outer) = ring_radii(1, 1);
        assert!(close(inner, CENTRE) && close(outer, 1.0));
    }
}
