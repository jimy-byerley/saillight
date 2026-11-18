//! recursive walk filling a size tree: links and mount points are not followed,
//! unreadable folders are recorded, progress is reported and the walk can be cancelled

use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::tree::{Kind, Node, NodeId, Tree, ROOT};
use crate::worker::Cancel;

/// what was counted so far
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Progress {
    pub items: u64,
    pub bytes: u64,
}

/// time between two progress reports
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// node of an item from its properties. links are described, not what they point to
pub fn node_of(name: impl Into<std::ffi::OsString>, metadata: &Metadata) -> Node {
    let kind = if metadata.file_type().is_symlink() {
        Kind::Link
    } else if metadata.is_dir() {
        Kind::Folder
    } else if metadata.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    // blocks are counted by 512 bytes whatever the block size of the filesystem
    let mut node = Node::new(name, kind, metadata.blocks() * 512);
    node.modified = metadata
        .mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(metadata.mtime_nsec());
    node.mode = metadata.mode();
    node.uid = metadata.uid();
    node.gid = metadata.gid();
    node
}

/// sizes of everything under `root`. `None` when cancelled
pub fn walk(root: &Path, progress: impl FnMut(Progress), cancel: &Cancel) -> Option<Tree> {
    walk_with(root, &|_, metadata| metadata.dev(), progress, cancel)
}

/// same as `walk`, with the filesystem an item is on given by `device`
pub fn walk_with(
    root: &Path,
    device: &dyn Fn(&Path, &Metadata) -> u64,
    mut progress: impl FnMut(Progress),
    cancel: &Cancel,
) -> Option<Tree> {
    let mut counted = Progress::default();
    let mut reported = Instant::now();

    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(_) => {
            // nothing can be said about the root: an empty folder that could not be read
            let mut node = Node::new(root_name(root), Kind::Folder, 0);
            node.unreadable = true;
            let mut tree = Tree::new(root, node);
            tree.compute();
            return Some(tree);
        }
    };
    let root_device = device(root, &metadata);
    let root_node = node_of(root_name(root), &metadata);
    let is_folder = root_node.is_folder();
    counted.items += 1;
    counted.bytes += root_node.own;
    let mut tree = Tree::new(root, root_node);

    // files with several names, already counted through one of them
    let mut linked: HashSet<(u64, u64)> = HashSet::new();
    let mut pending: Vec<(NodeId, PathBuf)> = Vec::new();
    if is_folder {
        pending.push((ROOT, root.to_path_buf()));
    }

    while let Some((folder, path)) = pending.pop() {
        if cancel.is_cancelled() {
            return None;
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(_) => {
                tree.node_mut(folder).unreadable = true;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    // the listing stopped in the middle
                    tree.node_mut(folder).unreadable = true;
                    break;
                }
            };
            let name = entry.file_name();
            let child_path = path.join(&name);
            let metadata = match fs::symlink_metadata(&child_path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    tree.node_mut(folder).failed.push(name);
                    continue;
                }
            };
            let mut node = node_of(name, &metadata);
            let mut enter = false;
            match node.kind {
                Kind::Folder => {
                    if device(&child_path, &metadata) != root_device {
                        node.mount = true;
                        // its size belongs to the other filesystem
                        node.own = 0;
                    } else {
                        enter = true;
                    }
                }
                Kind::File => {
                    if metadata.nlink() > 1 && !linked.insert((metadata.dev(), metadata.ino())) {
                        node.own = 0;
                    }
                }
                Kind::Link | Kind::Other => {}
            }
            counted.items += 1;
            counted.bytes += node.own;
            let id = tree.add(folder, node);
            if enter {
                pending.push((id, child_path));
            }
        }
        if reported.elapsed() >= REPORT_EVERY {
            reported = Instant::now();
            progress(counted);
        }
    }
    progress(counted);
    tree.compute();
    Some(tree)
}

fn root_name(root: &Path) -> std::ffi::OsString {
    root.file_name().map(|name| name.to_os_string()).unwrap_or_else(|| "/".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// a file of `size` bytes really written, so it uses blocks
    fn write(path: &Path, size: usize) {
        let mut file = File::create(path).unwrap();
        file.write_all(&vec![7u8; size]).unwrap();
        file.sync_all().unwrap();
    }

    fn used(path: &Path) -> u64 {
        fs::symlink_metadata(path).unwrap().blocks() * 512
    }

    fn scan(path: &Path) -> Tree {
        walk(path, |_| {}, &Cancel::default()).unwrap()
    }

    fn is_root() -> bool {
        // root reads everything, permission tests mean nothing then
        fs::metadata("/proc/self").map(|metadata| metadata.uid() == 0).unwrap_or(false)
    }

    #[test]
    fn totals_are_the_blocks_used_by_everything_inside() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::create_dir(root.join("sub/deep")).unwrap();
        fs::create_dir(root.join("empty")).unwrap();
        write(&root.join("one"), 10_000);
        write(&root.join("sub/two"), 50_000);
        write(&root.join("sub/deep/three"), 200_000);

        let tree = scan(root);
        assert_eq!(tree.path, root);
        assert_eq!(tree.len(), 7);
        let expected: u64 = ["", "sub", "sub/deep", "empty", "one", "sub/two", "sub/deep/three"]
            .iter()
            .map(|name| used(&root.join(name)))
            .sum();
        assert_eq!(tree.node(ROOT).total, expected);
        assert!(tree.node(ROOT).total >= 260_000);

        let sub = tree.find(&root.join("sub")).unwrap();
        assert_eq!(tree.node(sub).kind, Kind::Folder);
        assert_eq!(
            tree.node(sub).total,
            used(&root.join("sub"))
                + used(&root.join("sub/two"))
                + used(&root.join("sub/deep"))
                + used(&root.join("sub/deep/three"))
        );
        let three = tree.find(&root.join("sub/deep/three")).unwrap();
        assert_eq!(tree.node(three).kind, Kind::File);
        assert_eq!(tree.node(three).total, used(&root.join("sub/deep/three")));
        let empty = tree.find(&root.join("empty")).unwrap();
        assert!(tree.node(empty).children.is_empty());
        assert_eq!(tree.node(ROOT).blocked, 0);
        assert_eq!(tree.node(ROOT).mounts, 0);
    }

    #[test]
    fn properties_are_recorded() {
        let temp = TempDir::new();
        let file = temp.path().join("file");
        write(&file, 100);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let metadata = fs::metadata(&file).unwrap();

        let tree = scan(temp.path());
        let node = tree.node(tree.find(&file).unwrap());
        assert_eq!(node.mode & 0o777, 0o640);
        assert_eq!(node.uid, metadata.uid());
        assert_eq!(node.gid, metadata.gid());
        assert_eq!(node.modified / 1_000_000_000, metadata.mtime());
    }

    #[test]
    fn links_are_not_followed() {
        let temp = TempDir::new();
        let root = temp.path();
        let outside = TempDir::new();
        write(&outside.path().join("huge"), 300_000);
        fs::create_dir(root.join("inside")).unwrap();
        write(&root.join("inside/file"), 20_000);
        std::os::unix::fs::symlink(outside.path(), root.join("to-folder")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("huge"), root.join("to-file")).unwrap();
        std::os::unix::fs::symlink(root.join("inside"), root.join("to-inside")).unwrap();
        std::os::unix::fs::symlink("nowhere", root.join("broken")).unwrap();

        let tree = scan(root);
        assert_eq!(tree.len(), 7);
        for name in ["to-folder", "to-file", "to-inside", "broken"] {
            let node = tree.node(tree.find(&root.join(name)).unwrap());
            assert_eq!(node.kind, Kind::Link, "{name}");
            assert!(node.children.is_empty(), "{name}");
            assert_eq!(node.total, used(&root.join(name)), "{name}");
        }
        // nothing of the outside folder is counted, and the inside one only once
        assert!(tree.node(ROOT).total < 100_000);
    }

    #[test]
    fn a_file_with_several_names_is_counted_once() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("a")).unwrap();
        fs::create_dir(root.join("b")).unwrap();
        write(&root.join("a/file"), 100_000);
        fs::hard_link(root.join("a/file"), root.join("b/again")).unwrap();
        fs::hard_link(root.join("a/file"), root.join("third")).unwrap();
        write(&root.join("alone"), 30_000);

        let tree = scan(root);
        let size = used(&root.join("a/file"));
        let names = ["a/file", "b/again", "third"];
        let sizes: Vec<u64> = names
            .iter()
            .map(|name| tree.node(tree.find(&root.join(name)).unwrap()).own)
            .collect();
        assert_eq!(sizes.iter().sum::<u64>(), size);
        assert_eq!(sizes.iter().filter(|&&own| own == 0).count(), 2);
        let expected = used(root) + used(&root.join("a")) + used(&root.join("b")) + size
            + used(&root.join("alone"));
        assert_eq!(tree.node(ROOT).total, expected);
    }

    #[test]
    fn unreadable_folders_are_recorded() {
        if is_root() {
            return;
        }
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("locked")).unwrap();
        write(&root.join("locked/secret"), 50_000);
        write(&root.join("visible"), 10_000);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

        let tree = scan(root);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();

        let locked = tree.find(&root.join("locked")).unwrap();
        assert!(tree.node(locked).unreadable);
        assert!(tree.node(locked).children.is_empty());
        assert_eq!(tree.node(ROOT).blocked, 1);
        assert_eq!(tree.blocked_paths(ROOT), vec![root.join("locked")]);
        assert!(tree.find(&root.join("visible")).is_some());
    }

    #[test]
    fn names_that_cannot_be_examined_are_recorded() {
        if is_root() {
            return;
        }
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("listed")).unwrap();
        write(&root.join("listed/file"), 1000);
        // listing allowed, examining the content not
        fs::set_permissions(root.join("listed"), fs::Permissions::from_mode(0o444)).unwrap();

        let tree = scan(root);
        fs::set_permissions(root.join("listed"), fs::Permissions::from_mode(0o755)).unwrap();

        let listed = tree.find(&root.join("listed")).unwrap();
        assert_eq!(tree.node(listed).failed, vec![std::ffi::OsString::from("file")]);
        assert_eq!(tree.blocked_paths(ROOT), vec![root.join("listed/file")]);
    }

    #[test]
    fn a_missing_root_is_an_unreadable_folder() {
        let temp = TempDir::new();
        let tree = scan(&temp.path().join("missing"));
        assert_eq!(tree.len(), 1);
        assert!(tree.node(ROOT).unreadable);
        assert_eq!(tree.node(ROOT).total, 0);
        assert_eq!(tree.node(ROOT).blocked, 1);
    }

    #[test]
    fn a_file_can_be_the_root() {
        let temp = TempDir::new();
        let file = temp.path().join("file");
        write(&file, 5000);
        let tree = scan(&file);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.node(ROOT).kind, Kind::File);
        assert_eq!(tree.node(ROOT).total, used(&file));
    }

    #[test]
    fn mount_points_are_not_entered() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("card")).unwrap();
        write(&root.join("card/movie"), 200_000);
        fs::create_dir(root.join("normal")).unwrap();
        write(&root.join("normal/file"), 10_000);

        let card = root.join("card");
        let device = move |path: &Path, _: &Metadata| if path.starts_with(&card) { 2 } else { 1 };
        let tree = walk_with(root, &device, |_| {}, &Cancel::default()).unwrap();

        let card = tree.find(&root.join("card")).unwrap();
        assert!(tree.node(card).mount);
        assert!(tree.node(card).children.is_empty());
        assert_eq!(tree.node(card).total, 0);
        assert_eq!(tree.node(ROOT).mounts, 1);
        assert_eq!(tree.mount_paths(ROOT), vec![root.join("card")]);
        assert!(tree.find(&root.join("normal/file")).is_some());
        assert!(tree.node(ROOT).total < 100_000);
    }

    #[test]
    fn progress_ends_with_everything_counted() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("sub")).unwrap();
        write(&root.join("sub/file"), 10_000);
        write(&root.join("other"), 10_000);

        let mut reports = Vec::new();
        let tree = walk(root, |progress| reports.push(progress), &Cancel::default()).unwrap();
        let last = reports.last().unwrap();
        assert_eq!(last.items, 4);
        assert_eq!(last.bytes, tree.node(ROOT).total);
    }

    #[test]
    fn a_cancelled_walk_gives_nothing() {
        let temp = TempDir::new();
        write(&temp.path().join("file"), 1000);
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(walk(temp.path(), |_| {}, &cancel).is_none());
    }
}
