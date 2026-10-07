//! The kernel32 calls the core needs on Windows and std does not expose (stably): file
//! identity and link counts of an open handle, free space and the file system's name.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

/// `BY_HANDLE_FILE_INFORMATION`.
#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    attributes: u32,
    times: [u32; 6],
    volume_serial: u32,
    size: [u32; 2],
    links: u32,
    index_high: u32,
    index_low: u32,
}

/// `FILE_ID_INFO`.
#[repr(C)]
#[derive(Default)]
struct FileIdInfo {
    volume_serial: u64,
    id: [u8; 16],
}

/// `FileIdInfo` in `FILE_INFO_BY_HANDLE_CLASS`.
const FILE_ID_INFO: i32 = 18;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandle(file: *mut c_void, info: *mut ByHandleFileInformation) -> i32;
    fn GetFileInformationByHandleEx(
        file: *mut c_void,
        class: i32,
        info: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetDiskFreeSpaceExW(
        dir: *const u16,
        free_to_caller: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
    fn GetVolumePathNameW(path: *const u16, volume: *mut u16, len: u32) -> i32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_len: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        fs_name: *mut u16,
        fs_name_len: u32,
    ) -> i32;
}

fn by_handle(file: &File) -> io::Result<ByHandleFileInformation> {
    let mut info = ByHandleFileInformation::default();
    // SAFETY: an open handle and a properly sized out-parameter.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

/// (volume serial, file id) of an open handle. The 128-bit id where the file system has one
/// (ReFS's 64-bit index is not unique); else the 64-bit index. An id of 0 (some SMB servers
/// and drivers report it for every file) is an error, never a match.
pub(crate) fn file_id(file: &File) -> io::Result<(u64, u128)> {
    let (serial, id) = raw_file_id(file)?;
    if id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the volume reports no file ids (0), so this file cannot be told apart from others",
        ));
    }
    Ok((serial, id))
}

/// `file_id`, but an id of 0 is returned as is. Only for source stamps, which also compare
/// length and modification time; never for identities that decide what to publish or delete.
pub(crate) fn raw_file_id(file: &File) -> io::Result<(u64, u128)> {
    let mut info = FileIdInfo::default();
    // SAFETY: an open handle; `info` is the FILE_ID_INFO the class asks for, with its size.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FILE_ID_INFO,
            (&raw mut info).cast(),
            size_of::<FileIdInfo>() as u32,
        )
    };
    if ok != 0 {
        return Ok((info.volume_serial, u128::from_le_bytes(info.id)));
    }
    let info = by_handle(file)?;
    let index = (u64::from(info.index_high) << 32) | u64::from(info.index_low);
    Ok((u64::from(info.volume_serial), u128::from(index)))
}

/// How many names the open file has.
pub(crate) fn link_count(file: &File) -> io::Result<u64> {
    by_handle(file).map(|info| u64::from(info.links))
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// Bytes free to this user (quotas included) in `dir`.
pub(crate) fn free_bytes(dir: &Path) -> io::Result<u64> {
    // A UNC directory needs its trailing backslash; any other takes one too.
    let mut dir = wide(dir);
    dir.pop();
    if !matches!(dir.last(), Some(&c) if c == u16::from(b'\\') || c == u16::from(b'/')) {
        dir.push(u16::from(b'\\'));
    }
    dir.push(0);
    let mut free = 0u64;
    // SAFETY: a NUL-terminated wide string; the totals are optional (null).
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            dir.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}

/// The name of the file system holding `dir` (`NTFS`, `FAT32`, `exFAT`, ...), read from its
/// volume root (a drive, a mounted folder or a UNC share).
pub(crate) fn fs_name(dir: &Path) -> io::Result<String> {
    let dir = wide(dir);
    // The volume root is a prefix of the path, plus a trailing backslash.
    let mut root = vec![0u16; dir.len().max(260) + 1];
    // SAFETY: a NUL-terminated wide string and a buffer of the length passed.
    if unsafe { GetVolumePathNameW(dir.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0u16; 261];
    // SAFETY: `root` is NUL-terminated by the call above; `name` is a buffer of the length
    // passed; everything else is optional (null).
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Ok(String::from_utf16_lossy(&name[..len]))
}
