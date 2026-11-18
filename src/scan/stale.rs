//! tells whether a folder changed since its sizes were computed, by comparing
//! modification dates against the cached tree

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use super::tree::{Kind, NodeId, Tree, ROOT};
use super::walk::node_of;
use crate::worker::Cancel;

/// whether something under the folder of `tree` is not as the tree says. `None` when cancelled.
///
/// folders are compared by their modification date, which moves when something is added to,
/// removed from or renamed in them; the other items by their date, kind and size.
/// what is under one of the `ignored` paths is not looked at: the app's own files
pub fn changed(tree: &Tree, ignored: &[PathBuf], cancel: &Cancel) -> Option<bool> {
    let mut pending: Vec<(NodeId, PathBuf)> = vec![(ROOT, tree.path.clone())];
    let mut examined = 0u32;
    while let Some((id, path)) = pending.pop() {
        examined += 1;
        if examined % 256 == 0 && cancel.is_cancelled() {
            return None;
        }
        if ignored.iter().any(|ignored| path.starts_with(ignored)) {
            continue;
        }
        let node = tree.node(id);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            // it was there when the tree was made
            return Some(true);
        };
        let now = node_of("", &metadata);
        if now.kind != node.kind || now.modified != node.modified {
            return Some(true);
        }
        match node.kind {
            Kind::Folder => {
                // a mount point is not entered, an unreadable folder has nothing to compare
                if !node.mount {
                    for &child in &node.children {
                        pending.push((child, path.join(&tree.node(child).name)));
                    }
                }
            }
            // a file counted through another of its names has no size in the tree
            Kind::File if metadata.nlink() > 1 => {}
            Kind::File | Kind::Link | Kind::Other => {
                if now.own != node.own {
                    return Some(true);
                }
            }
        }
    }
    Some(false)
}

/// the files the app writes itself, which must not make a folder look changed
pub fn own_folders() -> Vec<PathBuf> {
    vec![crate::scan::cache::default_folder(), crate::config::default_folder()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::walk::walk;
    use crate::testing::TempDir;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    fn scan(path: &Path) -> Tree {
        walk(path, |_| {}, &Cancel::default()).unwrap()
    }

    fn is_changed(tree: &Tree) -> bool {
        changed(tree, &[], &Cancel::default()).unwrap()
    }

    /// a folder with dates in the past, so that any change made by a test shows
    fn fixture() -> TempDir {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("top"), vec![1u8; 5_000]).unwrap();
        fs::write(root.join("sub/file"), vec![1u8; 5_000]).unwrap();
        fs::write(root.join("sub/deep/file"), vec![1u8; 5_000]).unwrap();
        std::os::unix::fs::symlink("top", root.join("link")).unwrap();
        let past = SystemTime::now() - Duration::from_secs(3600);
        for name in ["sub/deep/file", "sub/deep", "sub/file", "sub", "top", ""] {
            let file = fs::File::open(root.join(name)).unwrap();
            file.set_modified(past).unwrap();
        }
        temp
    }

    #[test]
    fn an_untouched_folder_has_not_changed() {
        let temp = fixture();
        let tree = scan(temp.path());
        assert!(!is_changed(&tree));
        // reading does not count
        let _ = fs::read(temp.path().join("sub/deep/file")).unwrap();
        let _ = fs::read_dir(temp.path().join("sub")).unwrap().count();
        assert!(!is_changed(&tree));
    }

    #[test]
    fn additions_are_seen() {
        let temp = fixture();
        let tree = scan(temp.path());
        fs::write(temp.path().join("sub/deep/new"), b"new").unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn removals_are_seen() {
        let temp = fixture();
        let tree = scan(temp.path());
        fs::remove_file(temp.path().join("sub/deep/file")).unwrap();
        assert!(is_changed(&tree));

        let temp = fixture();
        let tree = scan(temp.path());
        fs::remove_dir_all(temp.path().join("sub")).unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn renames_are_seen() {
        let temp = fixture();
        let tree = scan(temp.path());
        fs::rename(temp.path().join("sub/file"), temp.path().join("sub/renamed")).unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn modified_files_are_seen() {
        let temp = fixture();
        let tree = scan(temp.path());
        fs::write(temp.path().join("sub/deep/file"), vec![2u8; 5_000]).unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn a_file_becoming_a_folder_is_seen() {
        let temp = fixture();
        let tree = scan(temp.path());
        let path = temp.path().join("top");
        let date = fs::metadata(&path).unwrap().modified().unwrap();
        let parent_date = fs::metadata(temp.path()).unwrap().modified().unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        // even with the dates put back
        fs::File::open(&path).unwrap().set_modified(date).unwrap();
        fs::File::open(temp.path()).unwrap().set_modified(parent_date).unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn ignored_paths_are_not_looked_at() {
        let temp = fixture();
        let tree = scan(temp.path());
        fs::write(temp.path().join("sub/deep/new"), b"new").unwrap();
        fs::write(temp.path().join("sub/deep/file"), b"other").unwrap();
        let ignored = vec![temp.path().join("sub/deep")];
        assert_eq!(changed(&tree, &ignored, &Cancel::default()), Some(false));
        // outside of it changes still show
        fs::write(temp.path().join("sub/new"), b"new").unwrap();
        assert_eq!(changed(&tree, &ignored, &Cancel::default()), Some(true));
    }

    #[test]
    fn a_file_with_several_names_is_compared_by_date_only() {
        let temp = fixture();
        fs::hard_link(temp.path().join("top"), temp.path().join("sub/again")).unwrap();
        let past = SystemTime::now() - Duration::from_secs(3600);
        fs::File::open(temp.path().join("sub")).unwrap().set_modified(past).unwrap();
        let tree = scan(temp.path());
        assert!(!is_changed(&tree));
    }

    #[test]
    fn a_vanished_root_has_changed() {
        let temp = fixture();
        let tree = scan(&temp.path().join("sub"));
        fs::remove_dir_all(temp.path().join("sub")).unwrap();
        assert!(is_changed(&tree));
    }

    #[test]
    fn cancellation_gives_no_answer() {
        let temp = TempDir::new();
        for index in 0..600 {
            fs::write(temp.path().join(format!("file{index}")), b"x").unwrap();
        }
        let tree = scan(temp.path());
        let cancel = Cancel::default();
        cancel.cancel();
        assert_eq!(changed(&tree, &[], &cancel), None);
    }
}
