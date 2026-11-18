//! names of users and groups, permissions as text

use std::ffi::CStr;

/// name of a user, its number when it has none
pub fn user(uid: u32) -> String {
    user_name(uid).unwrap_or_else(|| uid.to_string())
}

/// name of a group, its number when it has none
pub fn group(gid: u32) -> String {
    group_name(gid).unwrap_or_else(|| gid.to_string())
}

fn user_name(uid: u32) -> Option<String> {
    let mut buffer = vec![0; 1024];
    loop {
        let mut record: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(uid, &mut record, buffer.as_mut_ptr(), buffer.len(), &mut found)
        };
        if status == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || found.is_null() {
            return None;
        }
        let name = unsafe { CStr::from_ptr(record.pw_name) };
        return Some(name.to_string_lossy().into_owned());
    }
}

fn group_name(gid: u32) -> Option<String> {
    let mut buffer = vec![0; 1024];
    loop {
        let mut record: libc::group = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        let status = unsafe {
            libc::getgrgid_r(gid, &mut record, buffer.as_mut_ptr(), buffer.len(), &mut found)
        };
        if status == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || found.is_null() {
            return None;
        }
        let name = unsafe { CStr::from_ptr(record.gr_name) };
        return Some(name.to_string_lossy().into_owned());
    }
}

/// permission bits as `rwxr-xr-x`. file type and special bits are ignored
pub fn mode_to_string(mode: u32) -> String {
    let mut text = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = mode >> shift;
        text.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        text.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        text.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_string_conversion() {
        assert_eq!(mode_to_string(0o755), "rwxr-xr-x");
        assert_eq!(mode_to_string(0o640), "rw-r-----");
        assert_eq!(mode_to_string(0o000), "---------");
        assert_eq!(mode_to_string(0o100644), "rw-r--r--");
    }

    #[test]
    fn unknown_ids_fall_back_to_numbers() {
        assert_eq!(user(3_999_999_999), "3999999999");
        assert_eq!(group(3_999_999_999), "3999999999");
        // the first user of any system
        assert_eq!(user(0), "root");
    }
}
