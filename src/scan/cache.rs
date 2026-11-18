//! saves and loads a size tree in the app storage. always disposable: the filesystem is the truth
//!
//! one file, for one folder. lines of fields separated by spaces, the name last:
//!
//! ```text
//! saillight tree 1
//! <path of the folder>
//! N <parent> <kind> <own> <modified> <mode> <uid> <gid> <flags> <name>
//! F <folder> <name that could not be examined>
//! end <number of N lines>
//! ```
//!
//! nodes are numbered in the order of their lines, the parent of the first one is `-`.
//! in names and in the path, `\` is written `\\` and a line break `\n`

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::tree::{Kind, Node, NodeId, Tree, ROOT};

const HEADER: &[u8] = b"saillight tree 1";

/// folder holding the cache of the app
pub fn default_folder() -> PathBuf {
    let folder = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    folder.join("saillight")
}

/// where the cached tree of the user is
pub fn default_path() -> PathBuf {
    default_folder().join("tree")
}

fn escape(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'\\' => escaped.extend_from_slice(b"\\\\"),
            b'\n' => escaped.extend_from_slice(b"\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn unescape(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(bytes.len());
    let mut bytes = bytes.iter();
    while let Some(&byte) = bytes.next() {
        if byte == b'\\' {
            match bytes.next()? {
                b'n' => result.push(b'\n'),
                b'\\' => result.push(b'\\'),
                _ => return None,
            }
        } else {
            result.push(byte);
        }
    }
    Some(result)
}

fn kind_letter(kind: Kind) -> char {
    match kind {
        Kind::File => 'f',
        Kind::Folder => 'd',
        Kind::Link => 'l',
        Kind::Other => 'o',
    }
}

fn letter_kind(letter: &[u8]) -> Option<Kind> {
    match letter {
        b"f" => Some(Kind::File),
        b"d" => Some(Kind::Folder),
        b"l" => Some(Kind::Link),
        b"o" => Some(Kind::Other),
        _ => None,
    }
}

/// write the tree to `file`, replacing what was there
pub fn save(tree: &Tree, file: &Path) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    // written aside then renamed, so a crash never leaves half a cache
    let aside = file.with_extension("new");
    let result = write(tree, &aside).and_then(|()| fs::rename(&aside, file));
    if result.is_err() {
        let _ = fs::remove_file(&aside);
    }
    result
}

fn write(tree: &Tree, file: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(fs::File::create(file)?);
    out.write_all(HEADER)?;
    out.write_all(b"\n")?;
    out.write_all(&escape(tree.path.as_os_str().as_bytes()))?;
    out.write_all(b"\n")?;

    // (node, number of its parent in the file)
    let mut pending: Vec<(NodeId, Option<usize>)> = vec![(ROOT, None)];
    let mut count = 0usize;
    while let Some((id, parent)) = pending.pop() {
        let node = tree.node(id);
        let number = count;
        count += 1;
        match parent {
            Some(parent) => write!(out, "N {parent}")?,
            None => write!(out, "N -")?,
        }
        let flags = node.unreadable as u8 | (node.mount as u8) << 1;
        write!(
            out,
            " {} {} {} {} {} {} {} ",
            kind_letter(node.kind),
            node.own,
            node.modified,
            node.mode,
            node.uid,
            node.gid,
            flags
        )?;
        out.write_all(&escape(node.name.as_bytes()))?;
        out.write_all(b"\n")?;
        for name in &node.failed {
            write!(out, "F {number} ")?;
            out.write_all(&escape(name.as_bytes()))?;
            out.write_all(b"\n")?;
        }
        for &child in node.children.iter().rev() {
            pending.push((child, Some(number)));
        }
    }
    writeln!(out, "end {count}")?;
    out.flush()
}

/// the tree saved in `file`, when it is a complete one and about `folder`
pub fn load(file: &Path, folder: &Path) -> Option<Tree> {
    let content = fs::read(file).ok()?;
    let mut lines = content.split(|&byte| byte == b'\n');
    if lines.next()? != HEADER {
        return None;
    }
    let path = PathBuf::from(OsString::from_vec(unescape(lines.next()?)?));
    if path != folder {
        return None;
    }

    let mut tree: Option<Tree> = None;
    // node of each N line
    let mut ids: Vec<NodeId> = Vec::new();
    let mut complete = false;
    for line in lines {
        if complete {
            // nothing but the final line break may follow the end
            if !line.is_empty() {
                return None;
            }
            continue;
        }
        let (tag, rest) = split_field(line)?;
        match tag {
            b"N" => {
                let mut fields = rest.splitn(9, |&byte| byte == b' ');
                let parent = fields.next()?;
                let kind = letter_kind(fields.next()?)?;
                let own: u64 = number(fields.next()?)?;
                let modified: i64 = number(fields.next()?)?;
                let mode: u32 = number(fields.next()?)?;
                let uid: u32 = number(fields.next()?)?;
                let gid: u32 = number(fields.next()?)?;
                let flags: u8 = number(fields.next()?)?;
                let name = OsString::from_vec(unescape(fields.next()?)?);

                let mut node = Node::new(name, kind, own);
                node.modified = modified;
                node.mode = mode;
                node.uid = uid;
                node.gid = gid;
                node.unreadable = flags & 1 != 0;
                node.mount = flags & 2 != 0;
                match (&mut tree, parent) {
                    (None, b"-") => {
                        tree = Some(Tree::new(path.clone(), node));
                        ids.push(ROOT);
                    }
                    (Some(tree), parent) => {
                        let parent = *ids.get(number::<usize>(parent)?)?;
                        ids.push(tree.add(parent, node));
                    }
                    _ => return None,
                }
            }
            b"F" => {
                let (folder, name) = split_field(rest)?;
                let folder = *ids.get(number::<usize>(folder)?)?;
                let name = OsString::from_vec(unescape(name)?);
                tree.as_mut()?.node_mut(folder).failed.push(name);
            }
            b"end" => {
                if number::<usize>(rest)? != ids.len() {
                    return None;
                }
                complete = true;
            }
            _ => return None,
        }
    }
    if !complete {
        return None;
    }
    let mut tree = tree?;
    tree.compute();
    Some(tree)
}

/// forget the saved tree
pub fn clear(file: &Path) {
    let _ = fs::remove_file(file);
}

/// first field of a line and what follows its space
fn split_field(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let space = line.iter().position(|&byte| byte == b' ')?;
    Some((&line[..space], &line[space + 1..]))
}

fn number<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::tests::sample;
    use crate::scan::walk::walk;
    use crate::testing::TempDir;
    use crate::worker::Cancel;

    #[test]
    fn a_tree_comes_back_the_same() {
        let temp = TempDir::new();
        let file = temp.path().join("cache/tree");
        let mut tree = sample();
        let big = tree.find(Path::new("/data/big")).unwrap();
        let mut locked = Node::new("locked", Kind::Folder, 4096);
        locked.unreadable = true;
        locked.modified = -5;
        locked.mode = 0o40700;
        locked.uid = 100000;
        locked.gid = 1001;
        tree.add(big, locked);
        let mut card = Node::new("card", Kind::Folder, 0);
        card.mount = true;
        tree.add(ROOT, card);
        tree.add(ROOT, Node::new("link", Kind::Link, 0));
        tree.add(ROOT, Node::new("socket", Kind::Other, 0));
        tree.node_mut(big).failed.push("ghost".into());
        tree.node_mut(big).failed.push("other ghost".into());
        tree.compute();

        save(&tree, &file).unwrap();
        let loaded = load(&file, Path::new("/data")).unwrap();
        assert_eq!(loaded.len(), tree.len());
        assert_eq!(loaded.node(ROOT).total, tree.node(ROOT).total);
        assert_eq!(loaded.node(ROOT).blocked, 3);
        assert_eq!(loaded.node(ROOT).mounts, 1);
        for id in 0..tree.len() {
            let path = tree.path_of(id);
            let original = tree.node(id);
            let copy = loaded.node(loaded.find(&path).unwrap());
            assert_eq!(copy.name, original.name);
            assert_eq!(copy.kind, original.kind);
            assert_eq!(copy.own, original.own);
            assert_eq!(copy.total, original.total);
            assert_eq!(copy.modified, original.modified);
            assert_eq!(copy.mode, original.mode);
            assert_eq!((copy.uid, copy.gid), (original.uid, original.gid));
            assert_eq!(copy.unreadable, original.unreadable);
            assert_eq!(copy.mount, original.mount);
            assert_eq!(copy.failed, original.failed);
            assert_eq!(copy.children.len(), original.children.len());
        }
        // nothing is left aside
        assert_eq!(fs::read_dir(temp.path().join("cache")).unwrap().count(), 1);
    }

    #[test]
    fn odd_names_survive() {
        let temp = TempDir::new();
        let file = temp.path().join("tree");
        let names: Vec<OsString> = vec![
            "with space".into(),
            " leading and trailing ".into(),
            "line\nbreak".into(),
            "back\\slash".into(),
            "ends with backslash\\".into(),
            "\\n".into(),
            "".into(),
            OsString::from_vec(vec![b'b', 0xff, 0xfe, b'd']),
            "ünïcödé 文件".into(),
        ];
        let mut tree = Tree::new("/odd path\\with\nthings", Node::new("x", Kind::Folder, 1));
        for name in &names {
            tree.add(ROOT, Node::new(name.clone(), Kind::File, 10));
        }
        tree.node_mut(ROOT).failed.push("fai\nled".into());
        tree.compute();

        save(&tree, &file).unwrap();
        let loaded = load(&file, Path::new("/odd path\\with\nthings")).unwrap();
        let loaded_names: Vec<OsString> =
            loaded.node(ROOT).children.iter().map(|&child| loaded.node(child).name.clone()).collect();
        assert_eq!(loaded_names, names);
        assert_eq!(loaded.node(ROOT).failed, vec![OsString::from("fai\nled")]);
    }

    #[test]
    fn a_scanned_folder_comes_back_the_same() {
        let temp = TempDir::new();
        let root = temp.path().join("data");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/file"), vec![1u8; 30_000]).unwrap();
        fs::write(root.join("top"), vec![1u8; 5_000]).unwrap();
        std::os::unix::fs::symlink("top", root.join("link")).unwrap();
        let tree = walk(&root, |_| {}, &Cancel::default()).unwrap();

        let file = temp.path().join("tree");
        save(&tree, &file).unwrap();
        let loaded = load(&file, &root).unwrap();
        assert_eq!(loaded.len(), tree.len());
        assert_eq!(loaded.node(ROOT).total, tree.node(ROOT).total);
        let link = loaded.find(&root.join("link")).unwrap();
        assert_eq!(loaded.node(link).kind, Kind::Link);
        let copy = loaded.node(loaded.find(&root.join("a/b/file")).unwrap());
        let original = tree.node(tree.find(&root.join("a/b/file")).unwrap());
        assert_eq!(copy.own, original.own);
        assert_eq!(copy.modified, original.modified);
        assert_eq!(copy.mode, original.mode);
        assert!(copy.own >= 30_000);
    }

    #[test]
    fn a_tree_of_another_folder_is_not_loaded() {
        let temp = TempDir::new();
        let file = temp.path().join("tree");
        save(&sample(), &file).unwrap();
        assert!(load(&file, Path::new("/data")).is_some());
        assert!(load(&file, Path::new("/data/big")).is_none());
        assert!(load(&file, Path::new("/")).is_none());
    }

    #[test]
    fn missing_and_damaged_files_are_no_cache() {
        let temp = TempDir::new();
        let file = temp.path().join("tree");
        assert!(load(&file, Path::new("/data")).is_none());

        save(&sample(), &file).unwrap();
        let good = fs::read(&file).unwrap();

        // cut anywhere: never a tree
        for length in 0..good.len() - 1 {
            fs::write(&file, &good[..length]).unwrap();
            assert!(load(&file, Path::new("/data")).is_none(), "cut at {length}");
        }
        // the final line break alone may be missing
        fs::write(&file, &good[..good.len() - 1]).unwrap();
        assert!(load(&file, Path::new("/data")).is_some());

        let text = String::from_utf8(good.clone()).unwrap();
        for (from, to) in [
            ("saillight tree 1", "saillight tree 2"),
            ("N 0 d", "N 99 d"),
            ("N 0 d", "N 0 z"),
            ("N 0 d 10", "N 0 d ten"),
            ("N - d", "X - d"),
            ("end 7", "end 6"),
            ("end 7", "end 7\nN 0 f 1 0 0 0 0 0 late"),
        ] {
            assert!(text.contains(from), "{from}");
            fs::write(&file, text.replacen(from, to, 1)).unwrap();
            assert!(load(&file, Path::new("/data")).is_none(), "{from} -> {to}");
        }

        fs::write(&file, b"\xff\xfe\x00garbage").unwrap();
        assert!(load(&file, Path::new("/data")).is_none());
        fs::write(&file, b"").unwrap();
        assert!(load(&file, Path::new("/data")).is_none());
    }

    #[test]
    fn clearing_forgets_the_tree() {
        let temp = TempDir::new();
        let file = temp.path().join("tree");
        save(&sample(), &file).unwrap();
        clear(&file);
        assert!(load(&file, Path::new("/data")).is_none());
        // clearing nothing is fine
        clear(&file);
    }
}
