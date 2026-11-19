//! what the main page shows and how it reacts, without Qt: current folder, tree, sectors,
//! labels, hovered and selected sector

use std::path::{Path, PathBuf};

use crate::actions::delete::{refusal, Refusal};
use crate::chart::hit::{hit, Hit};
use crate::chart::labels::{first_ring, place, Area, Placement};
use crate::chart::layout::{layout, Sector, SectorKind};
use crate::scan::owner;
use crate::scan::tree::{Kind, Node, NodeId, Tree, ROOT};

pub const DEFAULT_DEPTH: u32 = 3;
pub const MAX_DEPTH: u32 = 6;

/// what changing the current folder needs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open {
    /// the folder was already known, its chart is shown
    Shown,
    /// its sizes have to be computed
    Scan,
}

/// what releasing the finger did
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// on the centre: go to the parent folder
    Parent,
    /// on a sector, now selected
    Selected,
    /// on a gap: nothing is selected anymore
    Cleared,
    /// out of the chart: nothing changes
    Nothing,
}

/// what the details panel shows about the selected sector
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Details {
    /// `file`, `folder`, `link`, `other`, or for markers `unreadable`, `mount`
    pub kind: &'static str,
    pub name: String,
    pub path: String,
    /// bytes used with the content
    pub size: u64,
    /// modification date, milliseconds since the epoch
    pub modified: i64,
    /// as `rwxr-xr-x`
    pub permissions: String,
    /// `user`, or `user:group` when they differ
    pub owner: String,
    /// number of items directly inside a folder
    pub count: usize,
    /// a folder that can become the current one
    pub enterable: bool,
    /// marker: paths that could not be read
    pub blocked: Vec<String>,
    /// marker: paths of mount points
    pub mounts: Vec<String>,
    /// why it cannot be deleted or compressed, empty when it can
    pub refusal: String,
}

pub struct State {
    /// home folder of the user, never deleted
    pub home: PathBuf,
    pub folder: PathBuf,
    /// sizes of the current folder, when known
    pub tree: Option<Tree>,
    pub depth: u32,
    pub sectors: Vec<Sector>,
    pub hovered: Option<usize>,
    pub selected: Option<usize>,
    pub area: Option<Area>,
    pub placement: Placement,
}

impl State {
    pub fn new(home: impl Into<PathBuf>, folder: impl Into<PathBuf>, depth: u32) -> Self {
        State {
            home: home.into(),
            folder: folder.into(),
            tree: None,
            depth: depth.clamp(1, MAX_DEPTH),
            sectors: Vec::new(),
            hovered: None,
            selected: None,
            area: None,
            placement: Placement::default(),
        }
    }

    /// recompute sectors and labels, keeping the selection when its sector is still there
    fn relayout(&mut self) {
        let selected = self.selected.and_then(|index| self.sectors.get(index)).map(|sector| (sector.node, sector.kind));
        self.sectors = match &self.tree {
            Some(tree) => layout(tree, ROOT, self.depth),
            None => Vec::new(),
        };
        self.hovered = None;
        self.selected = selected.and_then(|key| {
            self.sectors.iter().position(|sector| (sector.node, sector.kind) == key)
        });
        self.place_labels();
    }

    /// the sectors to name: the drawn items of the selected folder, of the current folder
    /// when nothing drawn is in the selection
    pub fn named(&self) -> Vec<usize> {
        let inside: Vec<usize> = match (&self.tree, self.selected_sector()) {
            (Some(tree), Some(selected)) if !selected.is_marker() => (0..self.sectors.len())
                .filter(|&index| {
                    let sector = &self.sectors[index];
                    !sector.is_marker() && tree.node(sector.node).parent == Some(selected.node)
                })
                .collect(),
            _ => Vec::new(),
        };
        if inside.is_empty() {
            first_ring(&self.sectors)
        } else {
            inside
        }
    }

    fn place_labels(&mut self) {
        self.placement = match &self.area {
            Some(area) => place(&self.sectors, &self.named(), self.depth, area),
            None => Placement::default(),
        };
    }

    /// change the current folder. a folder of the current tree is shown from what is known
    pub fn set_folder(&mut self, folder: &Path) -> Open {
        let known = self.tree.as_ref().and_then(|tree| {
            let id = tree.find(folder)?;
            let node = tree.node(id);
            // a mount point was not entered: nothing is known of its content
            (node.is_folder() && !node.mount).then(|| tree.subtree(id))
        });
        self.folder = folder.to_path_buf();
        self.selected = None;
        match known {
            Some(tree) => {
                self.tree = Some(tree);
                self.relayout();
                Open::Shown
            }
            None => {
                self.tree = None;
                self.relayout();
                Open::Scan
            }
        }
    }

    /// take the sizes of the current folder. a tree of another folder is ignored
    pub fn set_tree(&mut self, tree: Tree) -> bool {
        if tree.path != self.folder {
            return false;
        }
        // sizes computed again: what was selected stays selected when it is still there
        let selected = self.selected_sector().map(|sector| sector.kind).zip(self.selected_item_or_folder());
        self.tree = Some(tree);
        self.selected = None;
        self.relayout();
        if let (Some((kind, path)), Some(tree)) = (selected, &self.tree) {
            if let Some(id) = tree.find(&path) {
                self.selected = self.sectors.iter().position(|sector| sector.node == id && sector.kind == kind);
                self.place_labels();
            }
        }
        true
    }

    pub fn set_depth(&mut self, depth: u32) -> bool {
        let depth = depth.clamp(1, MAX_DEPTH);
        if depth == self.depth {
            return false;
        }
        self.depth = depth;
        self.relayout();
        true
    }

    pub fn set_area(&mut self, area: Area) -> bool {
        if self.area.as_ref() == Some(&area) {
            return false;
        }
        self.area = Some(area);
        self.place_labels();
        true
    }

    /// the folder containing the current one
    pub fn parent(&self) -> Option<PathBuf> {
        self.folder.parent().map(Path::to_path_buf)
    }

    /// bytes used by the current folder
    pub fn total(&self) -> u64 {
        self.tree.as_ref().map_or(0, |tree| tree.node(ROOT).total)
    }

    /// the finger is at (`x`, `y`) from the centre of the chart, its radius being 1.
    /// tells whether the hovered sector changed
    pub fn hover(&mut self, x: f64, y: f64) -> bool {
        let hovered = match hit(&self.sectors, self.depth, x, y) {
            Hit::Sector(index) => Some(index),
            Hit::Centre | Hit::Nothing => None,
        };
        let changed = hovered != self.hovered;
        self.hovered = hovered;
        changed
    }

    pub fn hover_nothing(&mut self) -> bool {
        self.hovered.take().is_some()
    }

    /// the finger left the screen at (`x`, `y`)
    pub fn release(&mut self, x: f64, y: f64) -> Release {
        self.hovered = None;
        match hit(&self.sectors, self.depth, x, y) {
            Hit::Centre => Release::Parent,
            Hit::Sector(index) => {
                self.selected = Some(index);
                self.place_labels();
                Release::Selected
            }
            Hit::Nothing if x * x + y * y > 1.0 => Release::Nothing,
            Hit::Nothing => {
                self.selected = None;
                self.place_labels();
                Release::Cleared
            }
        }
    }

    /// select a sector by its index, as a tap on its name does
    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&index| index < self.sectors.len());
        self.place_labels();
    }

    fn selected_sector(&self) -> Option<&Sector> {
        self.sectors.get(self.selected?)
    }

    /// path of the node of the selected sector, the folder of a marker
    fn selected_item_or_folder(&self) -> Option<PathBuf> {
        let sector = self.selected_sector()?;
        Some(self.tree.as_ref()?.path_of(sector.node))
    }

    /// node and path of the selected item. markers are not items
    pub fn selected_item(&self) -> Option<(NodeId, PathBuf)> {
        let sector = self.selected_sector()?;
        if sector.is_marker() {
            return None;
        }
        Some((sector.node, self.tree.as_ref()?.path_of(sector.node)))
    }

    /// why a node must not be deleted or compressed
    fn refusal_of(&self, id: NodeId, path: &Path) -> Option<Refusal> {
        if let Some(refusal) = refusal(path, &self.home) {
            return Some(refusal);
        }
        if id == ROOT {
            return Some(Refusal::Current);
        }
        // deleting through a mount point would empty another storage
        (self.tree.as_ref()?.node(id).mounts > 0).then_some(Refusal::Mount)
    }

    /// why the selected item must not be deleted or compressed
    pub fn refusal(&self) -> Option<Refusal> {
        let (id, path) = self.selected_item()?;
        self.refusal_of(id, &path)
    }

    /// whether the item at `path` can be deleted or compressed, or the message saying why not.
    /// only what the tree knows can be: the chart is what the user decides from
    pub fn removable(&self, path: &Path) -> Result<(), String> {
        let id = self
            .tree
            .as_ref()
            .and_then(|tree| tree.find(path))
            .ok_or("This is not in the chart anymore")?;
        match self.refusal_of(id, path) {
            Some(refusal) => Err(refusal.to_string()),
            None => Ok(()),
        }
    }

    pub fn details(&self) -> Option<Details> {
        let sector = self.selected_sector()?;
        let tree = self.tree.as_ref()?;
        let node = tree.node(sector.node);
        let path = tree.path_of(sector.node);
        let name = if sector.node == ROOT {
            path.file_name().map_or_else(|| "/".to_owned(), |name| name.to_string_lossy().into_owned())
        } else {
            node.name.to_string_lossy().into_owned()
        };
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        if sector.is_marker() {
            return Some(Details {
                kind: if sector.kind == SectorKind::Unreadable { "unreadable" } else { "mount" },
                name,
                path: text(path),
                blocked: tree.blocked_paths(sector.node).into_iter().map(text).collect(),
                mounts: tree.mount_paths(sector.node).into_iter().map(text).collect(),
                ..Details::default()
            });
        }
        let user = owner::user(node.uid);
        let group = owner::group(node.gid);
        Some(Details {
            kind: match node.kind {
                Kind::File => "file",
                Kind::Folder => "folder",
                Kind::Link => "link",
                Kind::Other => "other",
            },
            name,
            path: text(path),
            size: node.total,
            modified: node.modified.div_euclid(1_000_000),
            permissions: owner::mode_to_string(node.mode),
            owner: if user == group { user } else { format!("{user}:{group}") },
            count: node.children.len(),
            enterable: node.is_folder(),
            blocked: Vec::new(),
            mounts: Vec::new(),
            refusal: self.refusal().map(|refusal| refusal.to_string()).unwrap_or_default(),
        })
    }

    /// a folder was modified by the app itself: take its new date and size, so that it
    /// does not look changed by something else. to call before `removed` or `replaced`
    pub fn touched(&mut self, folder: &Path, now: &Node) {
        if let Some(tree) = &mut self.tree {
            if let Some(id) = tree.find(folder) {
                let node = tree.node_mut(id);
                node.modified = now.modified;
                if !node.mount {
                    node.own = now.own;
                }
            }
        }
    }

    /// an item was deleted
    pub fn removed(&mut self, path: &Path) {
        if let Some(tree) = &mut self.tree {
            if let Some(id) = tree.find(path) {
                tree.remove(id);
            }
        }
        self.relayout();
    }

    /// an item was compressed into the file described by `archive`
    pub fn replaced(&mut self, path: &Path, archive: Node) {
        if let Some(tree) = &mut self.tree {
            if let Some(id) = tree.find(path) {
                tree.replace(id, archive);
            }
        }
        // the archive keeps the node of what it replaces, so it stays selected when drawn
        self.relayout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::layout::ring_radii;
    use crate::scan::tree::tests::sample;

    fn state() -> State {
        let mut state = State::new("/home/user", "/data", 3);
        assert!(state.set_tree(sample()));
        state
    }

    /// point of the chart in the middle of a ring, at an angle in turns
    fn point(state: &State, ring: u32, turn: f64) -> (f64, f64) {
        let (inner, outer) = ring_radii(ring, state.depth);
        let radius = (inner + outer) / 2.0;
        let angle = turn * std::f64::consts::TAU;
        (radius * angle.sin(), -radius * angle.cos())
    }

    fn tap(state: &mut State, ring: u32, turn: f64) -> Release {
        let (x, y) = point(state, ring, turn);
        state.release(x, y)
    }

    fn selected_name(state: &State) -> Option<String> {
        state.details().map(|details| details.name)
    }

    #[test]
    fn a_tree_gives_sectors() {
        let state = state();
        assert_eq!(state.sectors.len(), 6);
        assert_eq!(state.total(), 1000);
        assert_eq!(state.selected, None);
        assert!(state.details().is_none());
    }

    #[test]
    fn a_tree_of_another_folder_is_refused() {
        let mut state = State::new("/home/user", "/elsewhere", 3);
        assert!(!state.set_tree(sample()));
        assert!(state.sectors.is_empty());
        assert_eq!(state.total(), 0);
    }

    #[test]
    fn hovering_follows_the_finger() {
        let mut state = state();
        let (x, y) = point(&state, 1, 0.5);
        assert!(state.hover(x, y));
        let big = state.hovered.unwrap();
        assert!(!state.hover(x, y));
        let (x, y) = point(&state, 2, 0.7);
        assert!(state.hover(x, y));
        assert_ne!(state.hovered, Some(big));
        // the centre and gaps hover nothing
        assert!(state.hover(0.0, 0.0));
        assert_eq!(state.hovered, None);
        assert!(!state.hover_nothing());
        // nothing is selected by hovering
        assert_eq!(state.selected, None);
    }

    #[test]
    fn release_selects_what_is_under() {
        let mut state = state();
        assert_eq!(tap(&mut state, 1, 0.5), Release::Selected);
        assert_eq!(selected_name(&state).as_deref(), Some("big"));
        assert_eq!(tap(&mut state, 2, 0.7), Release::Selected);
        assert_eq!(selected_name(&state).as_deref(), Some("b"));
        assert_eq!(state.hovered, None);
        // out of the chart: the selection stays
        assert_eq!(state.release(2.0, 2.0), Release::Nothing);
        assert_eq!(selected_name(&state).as_deref(), Some("b"));
        // a gap clears it
        assert_eq!(tap(&mut state, 3, 0.3), Release::Cleared);
        assert_eq!(state.selected, None);
        // the centre asks for the parent folder, and keeps the selection
        tap(&mut state, 1, 0.5);
        assert_eq!(state.release(0.0, 0.0), Release::Parent);
        assert_eq!(selected_name(&state).as_deref(), Some("big"));
    }

    #[test]
    fn details_describe_the_selection() {
        let mut state = state();
        tap(&mut state, 1, 0.5);
        let details = state.details().unwrap();
        assert_eq!(details.kind, "folder");
        assert_eq!(details.path, "/data/big");
        assert_eq!(details.size, 910);
        assert_eq!(details.count, 2);
        assert!(details.enterable);
        assert_eq!(details.refusal, "");
        assert_eq!(details.permissions, "---------");

        tap(&mut state, 2, 0.3);
        let details = state.details().unwrap();
        assert_eq!(details.kind, "file");
        assert_eq!(details.name, "a");
        assert_eq!(details.size, 600);
        assert!(!details.enterable);
    }

    #[test]
    fn selecting_by_index_is_checked() {
        let mut state = state();
        state.select(Some(0));
        assert_eq!(selected_name(&state).as_deref(), Some("big"));
        state.select(Some(99));
        assert_eq!(state.selected, None);
        state.select(Some(1));
        state.select(None);
        assert_eq!(state.selected, None);
    }

    #[test]
    fn depth_changes_the_rings_and_keeps_the_selection() {
        let mut state = state();
        tap(&mut state, 1, 0.94);
        assert_eq!(selected_name(&state).as_deref(), Some("small"));
        assert!(state.set_depth(1));
        assert_eq!(state.sectors.len(), 3);
        assert_eq!(selected_name(&state).as_deref(), Some("small"));
        assert!(!state.set_depth(1));
        assert!(!state.set_depth(0));
        assert!(state.set_depth(99));
        assert_eq!(state.depth, MAX_DEPTH);

        // a selection that is not drawn anymore is dropped
        tap(&mut state, 2, 0.3);
        assert_eq!(selected_name(&state).as_deref(), Some("a"));
        state.set_depth(1);
        assert_eq!(state.selected, None);
    }

    #[test]
    fn a_folder_of_the_tree_is_entered_without_a_scan() {
        let mut state = state();
        tap(&mut state, 1, 0.5);
        assert_eq!(state.set_folder(Path::new("/data/big")), Open::Shown);
        assert_eq!(state.folder, Path::new("/data/big"));
        assert_eq!(state.total(), 910);
        assert_eq!(state.sectors.len(), 2);
        assert_eq!(state.selected, None);
        assert_eq!(state.parent(), Some(PathBuf::from("/data")));

        // its parent is not known anymore
        assert_eq!(state.set_folder(Path::new("/data")), Open::Scan);
        assert!(state.tree.is_none());
        assert!(state.sectors.is_empty());
        assert!(state.set_tree(sample()));
        assert_eq!(state.sectors.len(), 6);
    }

    #[test]
    fn files_and_unknown_folders_need_a_scan() {
        let mut state = state();
        assert_eq!(state.set_folder(Path::new("/data/d")), Open::Scan);
        let mut state = self::state();
        assert_eq!(state.set_folder(Path::new("/other")), Open::Scan);
        assert_eq!(State::new("/h", "/", 3).parent(), None);
    }

    #[test]
    fn mount_points_are_scanned_when_entered() {
        let mut tree = sample();
        let mut card = Node::new("card", Kind::Folder, 0);
        card.mount = true;
        tree.add(ROOT, card);
        tree.compute();
        let mut state = State::new("/home/user", "/data", 3);
        state.set_tree(tree);
        assert_eq!(state.set_folder(Path::new("/data/card")), Open::Scan);
    }

    #[test]
    fn markers_list_what_they_are_about() {
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let mut locked = Node::new("locked", Kind::Folder, 0);
        locked.unreadable = true;
        tree.add(big, locked);
        let mut card = Node::new("card", Kind::Folder, 0);
        card.mount = true;
        tree.add(ROOT, card);
        tree.compute();
        let mut state = State::new("/home/user", "/data", 3);
        state.set_tree(tree);

        // the marker of the current folder is at the top of the first ring
        assert_eq!(tap(&mut state, 1, 0.001), Release::Selected);
        let details = state.details().unwrap();
        assert_eq!(details.kind, "unreadable");
        assert_eq!(details.name, "data");
        assert_eq!(details.blocked, ["/data/big/locked"]);
        assert_eq!(details.mounts, ["/data/card"]);
        assert!(state.selected_item().is_none());
        assert!(state.refusal().is_none());
    }

    #[test]
    fn what_must_not_be_removed() {
        let state = state();
        assert_eq!(state.removable(Path::new("/data/big")), Ok(()));
        assert_eq!(state.removable(Path::new("/data/big/a")), Ok(()));
        // the current folder itself, and what the chart does not know
        assert!(state.removable(Path::new("/data")).is_err());
        assert!(state.removable(Path::new("/data/missing")).is_err());
        assert!(state.removable(Path::new("/etc/passwd")).is_err());
        assert!(State::new("/home/user", "/data", 3).removable(Path::new("/data/big")).is_err());

        // the home folder and what it is in
        let mut tree = Tree::new("/home", Node::new("home", Kind::Folder, 0));
        let user = tree.add(ROOT, Node::new("user", Kind::Folder, 0));
        tree.add(user, Node::new("file", Kind::File, 100));
        tree.compute();
        let mut state = State::new("/home/user", "/home", 3);
        state.set_tree(tree);
        tap(&mut state, 1, 0.5);
        assert_eq!(state.refusal(), Some(Refusal::Home));
        assert!(!state.details().unwrap().refusal.is_empty());
        assert!(state.removable(Path::new("/home/user")).is_err());
        tap(&mut state, 2, 0.5);
        assert_eq!(state.refusal(), None);
        assert_eq!(state.removable(Path::new("/home/user/file")), Ok(()));

        // a folder containing a mount point
        let mut tree = sample();
        let small = tree.find(Path::new("/data/small")).unwrap();
        let mut card = Node::new("card", Kind::Folder, 0);
        card.mount = true;
        tree.add(small, card);
        tree.compute();
        let mut state = State::new("/home/user", "/data", 3);
        state.set_tree(tree);
        tap(&mut state, 1, 0.94);
        assert_eq!(selected_name(&state).as_deref(), Some("small"));
        assert_eq!(state.refusal(), Some(Refusal::Mount));
        assert!(state.removable(Path::new("/data/small")).is_err());
        tap(&mut state, 2, 0.93);
        assert_eq!(selected_name(&state).as_deref(), Some("c"));
        assert_eq!(state.refusal(), None);
    }

    #[test]
    fn a_removed_item_leaves_the_chart() {
        let mut state = state();
        tap(&mut state, 2, 0.3);
        assert_eq!(selected_name(&state).as_deref(), Some("a"));
        state.removed(Path::new("/data/big/a"));
        assert_eq!(state.total(), 400);
        assert_eq!(state.selected, None);
        assert!(state.sectors.iter().all(|sector| state.tree.as_ref().unwrap().attached(sector.node)));
        // big is now 310 of 400
        tap(&mut state, 1, 0.5);
        assert_eq!(state.details().unwrap().size, 310);
        // removing what is not in the tree changes nothing
        state.removed(Path::new("/data/missing"));
        assert_eq!(state.total(), 400);
    }

    #[test]
    fn a_folder_touched_by_the_app_takes_its_new_date() {
        let mut state = state();
        let mut now = Node::new("big", Kind::Folder, 30);
        now.modified = 12345;
        state.touched(Path::new("/data/big"), &now);
        state.removed(Path::new("/data/big/a"));
        let tree = state.tree.as_ref().unwrap();
        let big = tree.node(tree.find(Path::new("/data/big")).unwrap());
        assert_eq!(big.modified, 12345);
        assert_eq!(big.total, 330);
        // an unknown folder is ignored
        state.touched(Path::new("/elsewhere"), &now);
    }

    #[test]
    fn a_compressed_item_becomes_its_archive() {
        let mut state = state();
        tap(&mut state, 1, 0.5);
        state.replaced(Path::new("/data/big"), Node::new("big.zip", Kind::File, 200));
        assert_eq!(state.total(), 290);
        let tree = state.tree.as_ref().unwrap();
        assert!(tree.find(Path::new("/data/big")).is_none());
        assert!(tree.find(Path::new("/data/big.zip")).is_some());
        assert_eq!(selected_name(&state).as_deref(), Some("big.zip"));
        assert_eq!(state.details().unwrap().kind, "file");
    }

    #[test]
    fn the_selection_survives_new_sizes() {
        let mut state = state();
        tap(&mut state, 2, 0.3);
        assert_eq!(selected_name(&state).as_deref(), Some("a"));
        // the same folder scanned again, with something new in it
        let mut tree = sample();
        tree.add(ROOT, Node::new("new", Kind::File, 400));
        tree.compute();
        assert!(state.set_tree(tree));
        assert_eq!(selected_name(&state).as_deref(), Some("a"));
        // what is gone is not selected anymore
        let mut tree = sample();
        let a = tree.find(Path::new("/data/big/a")).unwrap();
        tree.remove(a);
        assert!(state.set_tree(tree));
        assert_eq!(state.selected, None);
    }

    #[test]
    fn names_are_those_of_the_selection() {
        let mut state = state();
        let area = Area { landscape: false, radius: 300.0, line: 40.0, width: 340.0, height: 0.0, rows: 5 };
        state.set_area(area);
        let names = |state: &State| {
            let tree = state.tree.as_ref().unwrap();
            let mut names: Vec<String> = state
                .placement
                .labels
                .iter()
                .map(|label| tree.node(state.sectors[label.sector].node).name.to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(&state), ["big", "d", "small"]);
        // a folder: what is inside it
        tap(&mut state, 1, 0.3);
        assert_eq!(selected_name(&state).as_deref(), Some("big"));
        assert_eq!(names(&state), ["a", "b"]);
        // a file has nothing inside: the current folder again
        tap(&mut state, 2, 0.3);
        assert_eq!(selected_name(&state).as_deref(), Some("a"));
        assert_eq!(names(&state), ["big", "d", "small"]);
        // by a tap on a name, then nothing
        let big = state.sectors.iter().position(|sector| sector.ring == 1).unwrap();
        state.select(Some(big));
        assert_eq!(names(&state), ["a", "b"]);
        state.select(None);
        assert_eq!(names(&state), ["big", "d", "small"]);
        // a folder whose content is not drawn
        state.select(Some(big));
        state.set_depth(1);
        assert_eq!(selected_name(&state).as_deref(), Some("big"));
        assert_eq!(names(&state), ["big", "d", "small"]);
    }

    #[test]
    fn labels_follow_the_area() {
        let mut state = state();
        assert!(state.placement.labels.is_empty());
        let area = Area { landscape: false, radius: 300.0, line: 40.0, width: 340.0, height: 0.0, rows: 5 };
        assert!(state.set_area(area.clone()));
        assert_eq!(state.placement.labels.len(), 3);
        assert!(!state.set_area(area));
        state.set_folder(Path::new("/data/big"));
        assert_eq!(state.placement.labels.len(), 2);
        // a folder nothing is known about
        state.set_folder(Path::new("/elsewhere"));
        assert!(state.placement.labels.is_empty());
    }
}
