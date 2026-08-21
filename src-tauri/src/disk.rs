//! How much room is left where her films go, and how big that disk is. The one thing in the app
//! that has to ask the operating system directly, so it lives here rather than in `core`.

use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct Space {
    /// Bytes available to this user, which is not the same as bytes unused: filesystems keep a
    /// margin.
    pub free: u64,
    pub total: u64,
}

pub fn space(path: &Path) -> Option<Space> {
    // a folder that does not exist yet still has a filesystem above it
    let mut candidate = path;
    while !candidate.exists() {
        candidate = candidate.parent()?;
    }
    measure(candidate)
}

#[cfg(unix)]
fn measure(path: &Path) -> Option<Space> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let route = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the path is a valid C string and the struct is ours to fill
    if unsafe { libc::statvfs(route.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    // these fields are not the same width on every unix, so the conversion is not always a no-op
    #[allow(clippy::useless_conversion)]
    Some(Space {
        free: u64::from(stats.f_bavail) * u64::from(stats.f_frsize),
        total: u64::from(stats.f_blocks) * u64::from(stats.f_frsize),
    })
}

#[cfg(windows)]
fn measure(path: &Path) -> Option<Space> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    // SAFETY: the path is null terminated and the output pointers are ours
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(Space {
        free: free_to_caller,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_free_and_total_for_a_folder_that_exists() {
        let space = space(Path::new(".")).expect("a filesystem");
        assert!(space.free > 0);
        assert!(
            space.total >= space.free,
            "a disk is at least as big as its free part"
        );
    }

    #[test]
    fn looks_upwards_for_a_folder_that_does_not_exist_yet() {
        let missing = std::env::temp_dir().join("mama-cine-not-here/nor-here");
        assert!(space(&missing).is_some());
    }
}
