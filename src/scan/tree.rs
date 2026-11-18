//! the size tree: one node per file or folder with its size, what could not be read,
//! and the removal of a node after a deletion or compression

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// index of a node in its tree. the root is always 0, and a child always comes after its parent
pub type NodeId = usize;

pub const ROOT: NodeId = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
    /// symbolic link, never followed
    Link,
    /// socket, pipe, device
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: OsString,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: Kind,
    /// bytes used on disk by the item itself, 0 for a file already counted through another link
    pub own: u64,
    /// modification date, nanoseconds since the epoch
    pub modified: i64,
    /// permission bits and file type, as given by the system
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// folder whose content could not be listed
    pub unreadable: bool,
    /// folder that is another filesystem: not entered
    pub mount: bool,
    /// names in this folder whose properties could not be read
    pub failed: Vec<OsString>,
    /// `own` plus everything inside. computed by `Tree::compute`
    pub total: u64,
    /// number of unreadable folders and failed names in this node and inside. computed
    pub blocked: u32,
    /// number of mount points in this node and inside. computed
    pub mounts: u32,
}

impl Node {
    pub fn new(name: impl Into<OsString>, kind: Kind, own: u64) -> Self {
        Node {
            name: name.into(),
            parent: None,
            children: Vec::new(),
            kind,
            own,
            modified: 0,
            mode: 0,
            uid: 0,
            gid: 0,
            unreadable: false,
            mount: false,
            failed: Vec::new(),
            total: own,
            blocked: 0,
            mounts: 0,
        }
    }

    pub fn is_folder(&self) -> bool {
        self.kind == Kind::Folder
    }
}

/// sizes of everything under a folder
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    /// path of the root node. the name of the root node is not used
    pub path: PathBuf,
    nodes: Vec<Node>,
}

impl Tree {
    pub fn new(path: impl Into<PathBuf>, mut root: Node) -> Self {
        root.parent = None;
        root.children.clear();
        Tree { path: path.into(), nodes: vec![root] }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }

    /// number of nodes ever added, detached ones included
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// add a leaf under `parent`. totals are not updated: see `compute`
    pub fn add(&mut self, parent: NodeId, mut node: Node) -> NodeId {
        let id = self.nodes.len();
        node.parent = Some(parent);
        node.children.clear();
        self.nodes.push(node);
        self.nodes[parent].children.push(id);
        id
    }

    /// recompute `total`, `blocked` and `mounts` of every node
    pub fn compute(&mut self) {
        for node in &mut self.nodes {
            node.total = node.own;
            node.blocked = node.failed.len() as u32 + node.unreadable as u32;
            node.mounts = node.mount as u32;
        }
        // children come after their parent, so going backward meets them first
        for id in (1..self.nodes.len()).rev() {
            if let Some(parent) = self.nodes[id].parent {
                let (total, blocked, mounts) = {
                    let node = &self.nodes[id];
                    (node.total, node.blocked, node.mounts)
                };
                let parent = &mut self.nodes[parent];
                parent.total += total;
                parent.blocked += blocked;
                parent.mounts += mounts;
            }
        }
    }

    /// whether the node is still attached to the root
    pub fn attached(&self, id: NodeId) -> bool {
        let mut current = id;
        loop {
            if current == ROOT {
                return true;
            }
            match self.nodes[current].parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    /// path of a node on the filesystem
    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        let mut current = id;
        while current != ROOT {
            names.push(&self.nodes[current].name);
            match self.nodes[current].parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        let mut path = self.path.clone();
        for name in names.into_iter().rev() {
            path.push(name);
        }
        path
    }

    /// node of a path of the filesystem, when it is in the tree
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let inside = path.strip_prefix(&self.path).ok()?;
        let mut current = ROOT;
        for name in inside.iter() {
            current = *self.nodes[current]
                .children
                .iter()
                .find(|&&child| self.nodes[child].name == name)?;
        }
        Some(current)
    }

    /// the tree of a folder of this tree, as if it had been scanned alone
    pub fn subtree(&self, id: NodeId) -> Tree {
        let mut tree = Tree::new(self.path_of(id), self.nodes[id].clone());
        // (node here, node in the new tree)
        let mut pending = vec![(id, ROOT)];
        while let Some((from, to)) = pending.pop() {
            for &child in &self.nodes[from].children {
                let added = tree.add(to, self.nodes[child].clone());
                pending.push((child, added));
            }
        }
        tree.compute();
        tree
    }

    /// detach a node and what is inside, after it was deleted. the root cannot be removed
    pub fn remove(&mut self, id: NodeId) {
        if id == ROOT {
            return;
        }
        if let Some(parent) = self.nodes[id].parent.take() {
            self.nodes[parent].children.retain(|&child| child != id);
        }
        self.compute();
    }

    /// put a leaf in place of a node and what is inside, after it was compressed
    pub fn replace(&mut self, id: NodeId, mut node: Node) {
        if id == ROOT {
            return;
        }
        for child in std::mem::take(&mut self.nodes[id].children) {
            self.nodes[child].parent = None;
        }
        node.parent = self.nodes[id].parent;
        node.children.clear();
        self.nodes[id] = node;
        self.compute();
    }

    /// paths of what could not be read in a node and inside
    pub fn blocked_paths(&self, id: NodeId) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut pending = vec![id];
        while let Some(current) = pending.pop() {
            let node = &self.nodes[current];
            if node.blocked == 0 {
                continue;
            }
            if node.unreadable {
                paths.push(self.path_of(current));
            }
            if !node.failed.is_empty() {
                let folder = self.path_of(current);
                paths.extend(node.failed.iter().map(|name| folder.join(name)));
            }
            pending.extend(node.children.iter().copied());
        }
        paths.sort();
        paths
    }

    /// paths of the mount points in a node and inside
    pub fn mount_paths(&self, id: NodeId) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut pending = vec![id];
        while let Some(current) = pending.pop() {
            let node = &self.nodes[current];
            if node.mounts == 0 {
                continue;
            }
            if node.mount {
                paths.push(self.path_of(current));
            }
            pending.extend(node.children.iter().copied());
        }
        paths.sort();
        paths
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// a tree built by hand:
    ///
    /// ```text
    /// /data           own 10
    /// ├── big         own 10
    /// │   ├── a       600
    /// │   └── b       300
    /// ├── small       own 10
    /// │   └── c       50
    /// └── d           20
    /// ```
    pub fn sample() -> Tree {
        let mut tree = Tree::new("/data", Node::new("data", Kind::Folder, 10));
        let big = tree.add(ROOT, Node::new("big", Kind::Folder, 10));
        tree.add(big, Node::new("a", Kind::File, 600));
        tree.add(big, Node::new("b", Kind::File, 300));
        let small = tree.add(ROOT, Node::new("small", Kind::Folder, 10));
        tree.add(small, Node::new("c", Kind::File, 50));
        tree.add(ROOT, Node::new("d", Kind::File, 20));
        tree.compute();
        tree
    }

    #[test]
    fn totals_include_the_content() {
        let tree = sample();
        assert_eq!(tree.node(ROOT).total, 1000);
        assert_eq!(tree.node(tree.find(Path::new("/data/big")).unwrap()).total, 910);
        assert_eq!(tree.node(tree.find(Path::new("/data/small")).unwrap()).total, 60);
        assert_eq!(tree.node(tree.find(Path::new("/data/d")).unwrap()).total, 20);
    }

    #[test]
    fn paths_and_nodes_match() {
        let tree = sample();
        assert_eq!(tree.find(Path::new("/data")), Some(ROOT));
        assert_eq!(tree.path_of(ROOT), Path::new("/data"));
        for id in 0..tree.len() {
            assert_eq!(tree.find(&tree.path_of(id)), Some(id));
        }
        assert_eq!(tree.find(Path::new("/data/big/missing")), None);
        assert_eq!(tree.find(Path::new("/elsewhere")), None);
        assert_eq!(tree.find(Path::new("/dat")), None);
    }

    #[test]
    fn subtree_is_a_tree_of_its_own() {
        let tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let sub = tree.subtree(big);
        assert_eq!(sub.path, Path::new("/data/big"));
        assert_eq!(sub.len(), 3);
        assert_eq!(sub.node(ROOT).total, 910);
        assert_eq!(sub.node(ROOT).parent, None);
        let a = sub.find(Path::new("/data/big/a")).unwrap();
        assert_eq!(sub.node(a).total, 600);
        assert_eq!(sub.node(a).parent, Some(ROOT));
    }

    #[test]
    fn removal_updates_the_totals_above() {
        let mut tree = sample();
        let a = tree.find(Path::new("/data/big/a")).unwrap();
        tree.remove(a);
        assert_eq!(tree.node(ROOT).total, 400);
        assert_eq!(tree.node(tree.find(Path::new("/data/big")).unwrap()).total, 310);
        assert_eq!(tree.find(Path::new("/data/big/a")), None);
        assert!(!tree.attached(a));

        let big = tree.find(Path::new("/data/big")).unwrap();
        let b = tree.find(Path::new("/data/big/b")).unwrap();
        tree.remove(big);
        assert_eq!(tree.node(ROOT).total, 90);
        assert!(!tree.attached(big));
        assert!(!tree.attached(b));
        assert!(tree.attached(tree.find(Path::new("/data/small/c")).unwrap()));
    }

    #[test]
    fn the_root_is_never_removed() {
        let mut tree = sample();
        tree.remove(ROOT);
        assert_eq!(tree.node(ROOT).total, 1000);
        assert_eq!(tree.node(ROOT).children.len(), 3);
    }

    #[test]
    fn replacement_puts_a_leaf_in_place() {
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let a = tree.find(Path::new("/data/big/a")).unwrap();
        tree.replace(big, Node::new("big.zip", Kind::File, 100));
        assert_eq!(tree.node(ROOT).total, 190);
        assert_eq!(tree.find(Path::new("/data/big")), None);
        let archive = tree.find(Path::new("/data/big.zip")).unwrap();
        assert_eq!(archive, big);
        assert_eq!(tree.node(archive).kind, Kind::File);
        assert!(tree.node(archive).children.is_empty());
        assert!(!tree.attached(a));
    }

    #[test]
    fn unreadable_items_and_mount_points_are_counted_above() {
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let mut locked = Node::new("locked", Kind::Folder, 10);
        locked.unreadable = true;
        tree.add(big, locked);
        tree.node_mut(big).failed.push("ghost".into());
        let small = tree.find(Path::new("/data/small")).unwrap();
        let mut card = Node::new("card", Kind::Folder, 10);
        card.mount = true;
        tree.add(small, card);
        tree.compute();

        assert_eq!(tree.node(ROOT).blocked, 2);
        assert_eq!(tree.node(big).blocked, 2);
        assert_eq!(tree.node(small).blocked, 0);
        assert_eq!(tree.node(ROOT).mounts, 1);
        assert_eq!(tree.node(small).mounts, 1);
        assert_eq!(tree.node(big).mounts, 0);
        assert_eq!(
            tree.blocked_paths(ROOT),
            vec![PathBuf::from("/data/big/ghost"), PathBuf::from("/data/big/locked")]
        );
        assert!(tree.blocked_paths(small).is_empty());
        assert_eq!(tree.mount_paths(ROOT), vec![PathBuf::from("/data/small/card")]);
    }
}
