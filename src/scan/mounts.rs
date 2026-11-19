//! the storages of the device and how full they are

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    /// where it is mounted
    pub path: PathBuf,
    /// bytes it can hold
    pub total: u64,
    /// bytes taken
    pub used: u64,
    /// bytes the user can still write
    pub available: u64,
}

/// `(total, free, available)` bytes of the filesystem holding `path`
pub fn usage(path: &Path) -> Option<(u64, u64, u64)> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs64 = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs64(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    Some((stat.f_blocks as u64 * block, stat.f_bfree as u64 * block, stat.f_bavail as u64 * block))
}

/// a field of the mount table: spaces and the like are written as `\040`
fn unescape(field: &str) -> PathBuf {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let code = (bytes[index] == b'\\')
            .then(|| field.get(index + 1..index + 4))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 8).ok());
        match code {
            Some(byte) => {
                out.push(byte);
                index += 4;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    PathBuf::from(std::ffi::OsStr::from_bytes(&out))
}

/// the storages listed by a mount table (`/proc/self/mounts`), sorted by path.
/// only writable ones on a real device are kept, each device once, at its shortest path
pub fn list_from(table: &str, usage: impl Fn(&Path) -> Option<(u64, u64, u64)>) -> Vec<Mount> {
    let mut places: Vec<(&str, PathBuf)> = Vec::new();
    for line in table.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(path), Some(_), Some(options)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !device.starts_with("/dev/") || options.split(',').any(|option| option == "ro") {
            continue;
        }
        let path = unescape(path);
        match places.iter_mut().find(|(other, _)| *other == device) {
            Some((_, known)) => {
                if path.as_os_str().len() < known.as_os_str().len() {
                    *known = path;
                }
            }
            None => places.push((device, path)),
        }
    }
    let mut mounts: Vec<Mount> = places
        .into_iter()
        .filter_map(|(_, path)| {
            let (total, free, available) = usage(&path)?;
            (total > 0).then(|| Mount { path, total, used: total.saturating_sub(free), available })
        })
        .collect();
    mounts.sort_by(|a, b| a.path.cmp(&b.path));
    mounts
}

/// the storages of the device, empty when they cannot be known
pub fn list() -> Vec<Mount> {
    match std::fs::read_to_string("/proc/self/mounts") {
        Ok(table) => list_from(&table, usage),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
sysfs /sys sysfs rw,nosuid 0 0
tmpfs /run tmpfs rw,nosuid 0 0
/dev/mapper/sailfish-root / ext4 rw,relatime 0 0
/dev/mapper/sailfish-home /home ext4 rw,relatime 0 0
/dev/mapper/sailfish-home /opt/alien/data ext4 rw,relatime 0 0
/dev/mmcblk0p12 /firmware vfat ro,relatime 0 0
/dev/mmcblk1p1 /run/media/user/my\\040card vfat rw,relatime 0 0
/dev/mmcblk0p9 /gone ext4 rw 0 0
/dev/mmcblk0p8 /empty ext4 rw 0 0
broken line
";

    fn fake(path: &Path) -> Option<(u64, u64, u64)> {
        match path.to_str()? {
            "/" => Some((1000, 400, 300)),
            "/home" => Some((5000, 1000, 900)),
            "/run/media/user/my card" => Some((800, 800, 800)),
            "/empty" => Some((0, 0, 0)),
            _ => None,
        }
    }

    #[test]
    fn writable_devices_once_each() {
        let mounts = list_from(TABLE, fake);
        let paths: Vec<&str> = mounts.iter().map(|mount| mount.path.to_str().unwrap()).collect();
        assert_eq!(paths, ["/", "/home", "/run/media/user/my card"]);
        assert_eq!(mounts[0], Mount { path: "/".into(), total: 1000, used: 600, available: 300 });
        assert_eq!(mounts[1].used, 4000);
        assert_eq!(mounts[2].used, 0);
    }

    #[test]
    fn a_device_is_kept_at_its_shortest_path() {
        let table = "/dev/a /opt/alien/home ext4 rw 0 0\n/dev/a /home ext4 rw 0 0\n";
        let mounts = list_from(table, |_| Some((10, 5, 5)));
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].path, PathBuf::from("/home"));
    }

    #[test]
    fn escapes() {
        assert_eq!(unescape("/a\\040b\\011c"), PathBuf::from("/a b\tc"));
        assert_eq!(unescape("/plain"), PathBuf::from("/plain"));
        // not an escape: kept as it is
        assert_eq!(unescape("/a\\b\\9"), PathBuf::from("/a\\b\\9"));
    }

    #[test]
    fn nothing_known() {
        assert!(list_from("", fake).is_empty());
        assert!(list_from(TABLE, |_| None).is_empty());
    }

    #[test]
    fn the_real_table_is_read() {
        // whatever the machine: no failure, and sensible figures
        for mount in list() {
            assert!(mount.path.is_absolute());
            assert!(mount.used <= mount.total);
        }
        assert!(usage(Path::new("/")).is_some());
        assert!(usage(Path::new("/does/not/exist")).is_none());
    }
}
