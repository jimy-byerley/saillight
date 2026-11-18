//! deletion of a file or of a folder with its content. a link is removed, never its target

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// why something must not be deleted, whatever its permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// the root of the filesystem
    Root,
    /// the home folder, or a folder it is in
    Home,
    /// a mount point, or a folder containing one: its content is another storage
    Mount,
    /// the folder the chart is about
    Current,
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Refusal::Root => "The root folder cannot be removed",
            Refusal::Home => "The home folder cannot be removed",
            Refusal::Mount => "This contains another storage, which is not removed from here",
            Refusal::Current => "The current folder cannot be removed from itself",
        })
    }
}

/// whether deleting or compressing `path` must be refused because of where it is
pub fn refusal(path: &Path, home: &Path) -> Option<Refusal> {
    if path.parent().is_none() || !path.is_absolute() {
        return Some(Refusal::Root);
    }
    if home.starts_with(path) {
        return Some(Refusal::Home);
    }
    None
}

/// delete a file, or a folder with everything inside
pub fn remove(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::os::unix::fs::symlink;

    #[test]
    fn files_and_full_folders_are_removed() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir_all(root.join("folder/sub")).unwrap();
        fs::write(root.join("folder/sub/file"), b"content").unwrap();
        fs::write(root.join("file"), b"content").unwrap();
        fs::write(root.join("kept"), b"content").unwrap();

        remove(&root.join("file")).unwrap();
        assert!(!root.join("file").exists());
        remove(&root.join("folder")).unwrap();
        assert!(!root.join("folder").exists());
        assert!(root.join("kept").exists());
    }

    #[test]
    fn a_link_is_removed_without_its_target() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target/file"), b"content").unwrap();
        symlink(root.join("target"), root.join("to-folder")).unwrap();
        symlink(root.join("target/file"), root.join("to-file")).unwrap();
        symlink("nowhere", root.join("broken")).unwrap();

        remove(&root.join("to-folder")).unwrap();
        remove(&root.join("to-file")).unwrap();
        remove(&root.join("broken")).unwrap();
        assert!(fs::symlink_metadata(root.join("to-folder")).is_err());
        assert!(fs::symlink_metadata(root.join("to-file")).is_err());
        assert!(fs::symlink_metadata(root.join("broken")).is_err());
        assert_eq!(fs::read(root.join("target/file")).unwrap(), b"content");
    }

    #[test]
    fn links_inside_a_removed_folder_keep_their_targets() {
        let temp = TempDir::new();
        let root = temp.path();
        fs::create_dir(root.join("precious")).unwrap();
        fs::write(root.join("precious/file"), b"content").unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        symlink(root.join("precious"), root.join("folder/link")).unwrap();

        remove(&root.join("folder")).unwrap();
        assert!(!root.join("folder").exists());
        assert_eq!(fs::read(root.join("precious/file")).unwrap(), b"content");
    }

    #[test]
    fn a_missing_item_is_an_error() {
        let temp = TempDir::new();
        assert!(remove(&temp.path().join("missing")).is_err());
    }

    #[test]
    fn places_that_are_never_removed() {
        let home = Path::new("/home/user");
        assert_eq!(refusal(Path::new("/"), home), Some(Refusal::Root));
        assert_eq!(refusal(Path::new(""), home), Some(Refusal::Root));
        assert_eq!(refusal(Path::new("relative/path"), home), Some(Refusal::Root));
        assert_eq!(refusal(Path::new("/home/user"), home), Some(Refusal::Home));
        assert_eq!(refusal(Path::new("/home/user/"), home), Some(Refusal::Home));
        assert_eq!(refusal(Path::new("/home"), home), Some(Refusal::Home));
        assert_eq!(refusal(Path::new("/home/user/Documents"), home), None);
        assert_eq!(refusal(Path::new("/home/other"), home), None);
        assert_eq!(refusal(Path::new("/home/username"), home), None);
        assert_eq!(refusal(Path::new("/tmp/file"), home), None);
    }
}
