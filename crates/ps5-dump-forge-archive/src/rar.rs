use super::*;
use std::{
    ffi::{c_char, c_int, c_uint, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};
// UnRAR's structs and wide callbacks are declared here from dll.hpp (`pragma pack(1)`, hence
// `packed`): unrar_sys types wchar_t as libc's, but the PS5 SDK's is 2 bytes. build.rs checks the
// asserts below in C++.
use unrar_sys as _;

#[cfg(any(windows, target_env = "ps5"))]
type WChar = u16;
#[cfg(not(any(windows, target_env = "ps5")))]
type WChar = u32;
#[cfg(target_env = "ps5")]
const _: () = assert!(size_of::<WChar>() == 2);
// LPARAM: long on Unix (LP64), LONG_PTR on Windows.
type LParam = isize;
type Callback = extern "C" fn(c_uint, LParam, LParam, LParam) -> c_int;

const ERAR_END_ARCHIVE: c_int = 10;
const ERAR_BAD_DATA: c_int = 12;
const ERAR_UNKNOWN_FORMAT: c_int = 14;
const ERAR_MISSING_PASSWORD: c_int = 22;
const ERAR_BAD_PASSWORD: c_int = 24;
const ERAR_LARGE_DICT: c_int = 25;
const RAR_OM_LIST: c_uint = 0;
const RAR_OM_EXTRACT: c_uint = 1;
const RAR_SKIP: c_int = 0;
const RAR_TEST: c_int = 1;
const RAR_VOL_ASK: LParam = 0;
const RHDF_ENCRYPTED: c_uint = 0x04;
const RHDF_SOLID: c_uint = 0x10;
const RHDF_DIRECTORY: c_uint = 0x20;
const ROADF_SOLID: c_uint = 0x0008;
const ROADF_ENCHEADERS: c_uint = 0x0080;
const UCM_CHANGEVOLUME: c_uint = 0;
const UCM_PROCESSDATA: c_uint = 1;
const UCM_NEEDPASSWORD: c_uint = 2;
const UCM_CHANGEVOLUMEW: c_uint = 3;
const UCM_NEEDPASSWORDW: c_uint = 4;
const UCM_LARGEDICT: c_uint = 5;

// struct RARHeaderDataEx
#[repr(C, packed)]
#[allow(dead_code)]
struct HeaderDataEx {
    arc_name: [c_char; 1024],
    arc_name_w: [WChar; 1024],
    file_name: [c_char; 1024],
    file_name_w: [WChar; 1024],
    flags: c_uint,
    pack_size: c_uint,
    pack_size_high: c_uint,
    unp_size: c_uint,
    unp_size_high: c_uint,
    host_os: c_uint,
    file_crc: c_uint,
    file_time: c_uint,
    unp_ver: c_uint,
    method: c_uint,
    file_attr: c_uint,
    cmt_buf: *mut c_char,
    cmt_buf_size: c_uint,
    cmt_size: c_uint,
    cmt_state: c_uint,
    dict_size: c_uint,
    hash_type: c_uint,
    hash: [c_char; 32],
    redir_type: c_uint,
    redir_name: *mut WChar,
    redir_name_size: c_uint,
    dir_target: c_uint,
    mtime_low: c_uint,
    mtime_high: c_uint,
    ctime_low: c_uint,
    ctime_high: c_uint,
    atime_low: c_uint,
    atime_high: c_uint,
    arc_name_ex: *mut WChar,
    arc_name_ex_size: c_uint,
    file_name_ex: *mut WChar,
    file_name_ex_size: c_uint,
    reserved: [c_uint; 982],
}
// struct RAROpenArchiveDataEx
#[repr(C, packed)]
#[allow(dead_code)]
struct OpenArchiveDataEx {
    arc_name: *const c_char,
    arc_name_w: *const WChar,
    open_mode: c_uint,
    open_result: c_uint,
    cmt_buf: *mut c_char,
    cmt_buf_size: c_uint,
    cmt_size: c_uint,
    cmt_state: c_uint,
    flags: c_uint,
    callback: Option<Callback>,
    user_data: LParam,
    op_flags: c_uint,
    cmt_buf_w: *mut WChar,
    reserved: [c_uint; 25],
}
// Packed layouts on our 64-bit targets; build.rs asserts the same numbers (keep them in step).
const H: usize = 2048 + 2048 * size_of::<WChar>();
const _: () = {
    use std::mem::offset_of;
    assert!(size_of::<*const c_void>() == 8 && size_of::<LParam>() == 8);
    assert!(offset_of!(HeaderDataEx, file_name_w) == 2048 + 1024 * size_of::<WChar>());
    assert!(offset_of!(HeaderDataEx, flags) == H);
    assert!(offset_of!(HeaderDataEx, unp_size) == H + 12);
    assert!(offset_of!(HeaderDataEx, unp_size_high) == H + 16);
    assert!(offset_of!(HeaderDataEx, host_os) == H + 20);
    assert!(offset_of!(HeaderDataEx, file_attr) == H + 40);
    assert!(offset_of!(HeaderDataEx, redir_type) == H + 104);
    assert!(offset_of!(HeaderDataEx, redir_name) == H + 108);
    assert!(offset_of!(HeaderDataEx, file_name_ex) == H + 160);
    assert!(size_of::<HeaderDataEx>() == H + 4100);
    assert!(offset_of!(OpenArchiveDataEx, arc_name) == 0);
    assert!(offset_of!(OpenArchiveDataEx, arc_name_w) == 8);
    assert!(offset_of!(OpenArchiveDataEx, open_mode) == 16);
    assert!(offset_of!(OpenArchiveDataEx, open_result) == 20);
    assert!(offset_of!(OpenArchiveDataEx, flags) == 44);
    assert!(offset_of!(OpenArchiveDataEx, callback) == 48);
    assert!(offset_of!(OpenArchiveDataEx, user_data) == 56);
    assert!(offset_of!(OpenArchiveDataEx, op_flags) == 64);
    assert!(offset_of!(OpenArchiveDataEx, cmt_buf_w) == 68);
    assert!(size_of::<OpenArchiveDataEx>() == 176);
};
unsafe extern "C" {
    fn RAROpenArchiveEx(data: *mut OpenArchiveDataEx) -> *mut c_void;
    fn RARCloseArchive(handle: *mut c_void) -> c_int;
    fn RARReadHeaderEx(handle: *mut c_void, data: *mut HeaderDataEx) -> c_int;
    fn RARProcessFileW(
        handle: *mut c_void,
        operation: c_int,
        dest_path: *mut WChar,
        dest_name: *mut WChar,
    ) -> c_int;
    fn RARSetCallback(handle: *mut c_void, callback: Option<Callback>, user_data: LParam);
    // UnRAR's global `ErrorHandler ErrHandler`: its first member is the int-sized `RAR_EXIT
    // ExitCode`, the only field read.
    // ponytail: the layout is taken from errhnd.hpp by hand; an unrar_sys bump must recheck it.
    #[cfg_attr(target_env = "msvc", link_name = "?ErrHandler@@3VErrorHandler@@A")]
    static mut ErrHandler: c_int;
}

// Passwords have no derived Debug, and decoder error messages are all replaced.
struct Secret(Vec<WChar>);
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
// UTF-16 where wchar_t is 2 bytes (Windows, the PS5), UTF-32 elsewhere; NUL-terminated.
fn wide(s: &str) -> Vec<WChar> {
    #[cfg(any(windows, target_env = "ps5"))]
    let mut out: Vec<WChar> = s.encode_utf16().collect();
    #[cfg(not(any(windows, target_env = "ps5")))]
    let mut out: Vec<WChar> = s.chars().map(u32::from).collect();
    out.push(0);
    out
}
fn decode(w: &[WChar]) -> Result<String, ArchiveError> {
    #[cfg(any(windows, target_env = "ps5"))]
    {
        String::from_utf16(w).map_err(|_| corrupt("Invalid RAR filename."))
    }
    #[cfg(not(any(windows, target_env = "ps5")))]
    {
        w.iter()
            .map(|&c| char::from_u32(c).ok_or_else(|| corrupt("Invalid RAR filename.")))
            .collect()
    }
}
struct State<'a> {
    password: Option<Secret>,
    failure: Option<ArchiveError>,
    volumes: Vec<PathBuf>,
    cancel: &'a AtomicBool,
    sink: Option<&'a mut dyn Sink>,
    active: bool,
}
impl fmt::Debug for State<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RarState")
            .field("password", &self.password)
            .finish_non_exhaustive()
    }
}
extern "C" fn callback(msg: c_uint, user: LParam, p1: LParam, p2: LParam) -> c_int {
    // SAFETY: State is boxed at a stable address until the handle has been closed.
    let state = unsafe { &mut *(user as *mut State<'_>) };
    if state.failure.is_some() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| handle_callback(state, msg, p1, p2)));
    match result {
        Ok(Ok(())) => 1,
        Ok(Err(e)) => {
            state.failure = Some(e);
            -1
        }
        Err(_) => {
            state.failure = Some(corrupt("RAR callback failed."));
            -1
        }
    }
}
fn handle_callback(
    state: &mut State<'_>,
    msg: c_uint,
    p1: LParam,
    p2: LParam,
) -> Result<(), ArchiveError> {
    check(state.cancel)?;
    match msg {
        UCM_PROCESSDATA if state.active => {
            if p1 == 0 || p2 < 0 {
                return Err(corrupt("Invalid RAR data callback."));
            }
            // UnRAR's buffer, valid for the callback: written in ≤ 64 KiB pieces, cancel checked
            // between.
            let data = unsafe { std::slice::from_raw_parts(p1 as *const u8, p2 as usize) };
            for chunk in data.chunks(64 * 1024) {
                check(state.cancel)?;
                if let Some(s) = &mut state.sink {
                    s.chunk(chunk)?;
                }
            }
        }
        UCM_NEEDPASSWORDW => {
            let pw = state
                .password
                .as_ref()
                .ok_or(ArchiveError::PasswordRequired)?;
            if pw.0.len() == 1 {
                return Err(ArchiveError::PasswordWrong);
            }
            if p1 == 0 || p2 <= 0 || pw.0.len() > p2 as usize {
                return Err(ArchiveError::Unsupported(
                    "Archive password is too long.".into(),
                ));
            }
            // Buffer capacity comes from UnRAR, including the terminating NUL.
            unsafe {
                ptr::copy_nonoverlapping(pw.0.as_ptr(), p1 as *mut WChar, pw.0.len());
            }
        }
        // Each volume comes twice, wide then narrow; UnRAR opens the narrow one on Unix (bytes
        // round-trip with no C locale), wide on Windows.
        #[cfg(windows)]
        UCM_CHANGEVOLUMEW => {
            if p1 == 0 {
                return Err(corrupt("Invalid RAR volume callback."));
            }
            // This is a NUL-terminated string, NOT a fixed 2048-unit array.
            let pointer = p1 as *const WChar;
            let mut len = 0;
            unsafe {
                while *pointer.add(len) != 0 {
                    len += 1;
                }
            }
            let name = decode(unsafe { std::slice::from_raw_parts(pointer, len) })?;
            volume(state, PathBuf::from(name), p2)?;
        }
        #[cfg(unix)]
        UCM_CHANGEVOLUME => {
            use std::os::unix::ffi::OsStrExt;
            if p1 == 0 {
                return Err(corrupt("Invalid RAR volume callback."));
            }
            let name = unsafe { std::ffi::CStr::from_ptr(p1 as *const c_char) };
            volume(
                state,
                Path::new(std::ffi::OsStr::from_bytes(name.to_bytes())).into(),
                p2,
            )?;
        }
        // The other spelling of the same volume: answered by the arm above.
        #[cfg(windows)]
        UCM_CHANGEVOLUME => (),
        #[cfg(unix)]
        UCM_CHANGEVOLUMEW => (),
        // Data of a skipped solid member: decoding goes on.
        UCM_PROCESSDATA => (),
        UCM_NEEDPASSWORD => return Err(ArchiveError::PasswordRequired),
        // UnRAR asks to go past its 4 GiB default dictionary; only 1 says yes, so this refuses.
        UCM_LARGEDICT => return Err(large_dict()),
        // Anything newer than this wrapper: no (-1), never a silent yes.
        _ => {
            return Err(ArchiveError::Unsupported(
                "Unsupported RAR archive feature.".into(),
            ));
        }
    }
    Ok(())
}
fn large_dict() -> ArchiveError {
    ArchiveError::Unsupported(
        "The archive needs a decompression dictionary over 4 GiB; re-pack it with a smaller one."
            .into(),
    )
}
fn volume(state: &mut State<'_>, path: PathBuf, mode: LParam) -> Result<(), ArchiveError> {
    if mode == RAR_VOL_ASK && !path.is_file() {
        return Err(ArchiveError::MissingPart(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ));
    }
    if !state.volumes.contains(&path) {
        state.volumes.push(path);
    }
    Ok(())
}
static RAR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            RARCloseArchive(self.0);
        }
    }
}
fn error(code: c_int, state: &mut State<'_>, encrypted: bool) -> ArchiveError {
    if let Some(e) = state.failure.take() {
        return e;
    }
    match code {
        ERAR_MISSING_PASSWORD => {
            if state.password.is_some() {
                ArchiveError::PasswordWrong
            } else {
                ArchiveError::PasswordRequired
            }
        }
        ERAR_BAD_PASSWORD => {
            if state.password.is_some() {
                ArchiveError::PasswordWrong
            } else {
                ArchiveError::PasswordRequired
            }
        }
        ERAR_BAD_DATA if encrypted => {
            if state.password.is_some() {
                ArchiveError::PasswordWrong
            } else {
                ArchiveError::PasswordRequired
            }
        }
        ERAR_LARGE_DICT => large_dict(),
        ERAR_UNKNOWN_FORMAT => ArchiveError::Unsupported("Unsupported RAR archive format.".into()),
        _ => corrupt("RAR archive is damaged or could not be read."),
    }
}
fn open(path: &Path, state: &mut State<'_>) -> Result<(Handle, u32), ArchiveError> {
    let mut data: OpenArchiveDataEx = unsafe { std::mem::zeroed() };
    // Unix: the path's own bytes, narrow only (see the volume callback). Windows: wide only.
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| corrupt("Invalid RAR archive path."))?
    };
    #[cfg(unix)]
    {
        data.arc_name = name.as_ptr();
    }
    #[cfg(windows)]
    let name: Vec<WChar> = {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain([0]).collect()
    };
    #[cfg(windows)]
    {
        data.arc_name_w = name.as_ptr();
    }
    data.open_mode = if state.sink.is_some() {
        RAR_OM_EXTRACT
    } else {
        RAR_OM_LIST
    };
    data.callback = Some(callback);
    data.user_data = state as *mut _ as LParam;
    let handle = unsafe { RAROpenArchiveEx(&raw mut data) };
    if handle.is_null() {
        return Err(error(data.open_result as c_int, state, true));
    }
    let handle = Handle(handle);
    if data.open_result != 0 {
        return Err(error(data.open_result as c_int, state, true));
    }
    unsafe {
        RARSetCallback(handle.0, Some(callback), data.user_data);
    }
    Ok((handle, data.flags))
}
fn entry(h: &HeaderDataEx) -> Result<Entry, ArchiveError> {
    // Packed: copy fields out, never borrow them.
    let name_w = h.file_name_w;
    let (flags, redir_type, host_os, file_attr) = (h.flags, h.redir_type, h.host_os, h.file_attr);
    // UnRAR cuts a longer name to the buffer and still terminates it: a full buffer may be cut.
    let end = name_w
        .iter()
        .position(|&c| c == 0)
        .filter(|&n| n + 1 < name_w.len())
        .ok_or_else(|| corrupt("RAR filename is too long."))?;
    // UnRAR gives separators as the system's: `\` on Windows becomes `/`; elsewhere `\` is part of
    // a name, which `safe_path` refuses.
    let name = decode(&name_w[..end])?;
    #[cfg(windows)]
    let name = name.replace('\\', "/");
    let dir = flags & RHDF_DIRECTORY != 0;
    if redir_type != 0 {
        return Err(ArchiveError::Unsupported(
            "Archive links and special files are not supported.".into(),
        ));
    }
    if host_os == 3 {
        special(file_attr, dir)?;
    }
    Ok(Entry {
        path: safe_path(&name, dir)?,
        size: (h.unp_size_high as u64) << 32 | h.unp_size as u64,
        dir,
        encrypted: flags & RHDF_ENCRYPTED != 0,
    })
}
fn run<'a>(
    path: &Path,
    password: Option<&str>,
    cancel: &'a AtomicBool,
    sink: Option<&'a mut dyn Sink>,
) -> Result<Listing, ArchiveError> {
    // UnRAR's one global, unsynchronized ErrHandler: one session at a time, open to close (`handle`
    // drops before the guard).
    // ponytail: a long extraction blocks every other .rar listing or read until it ends.
    let _session = RAR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = Box::new(State {
        password: password.map(|p| Secret(wide(p))),
        failure: None,
        volumes: vec![path.to_owned()],
        cancel,
        sink,
        active: false,
    });
    let (handle, flags) = open(path, &mut state)?;
    let mut listing = Listing {
        format: ArchiveFormat::Rar,
        entries: Vec::new(),
        volumes: Vec::new(),
        archive_bytes: 0,
        solid: flags & ROADF_SOLID != 0,
        encrypted: flags & ROADF_ENCHEADERS != 0,
    };
    loop {
        check(cancel)?;
        if state.sink.as_ref().is_some_and(|s| s.done()) {
            break;
        }
        // All integers and pointers: zero is a valid value (null Ex name buffers).
        let mut h: HeaderDataEx = unsafe { std::mem::zeroed() };
        let code = unsafe { RARReadHeaderEx(handle.0, &raw mut h) };
        if code == ERAR_END_ARCHIVE {
            // A header cut short also ends the walk with only RARX_WARNING: the files after it
            // would be missing.
            if unsafe { ptr::read_volatile(&raw const ErrHandler) } != 0 {
                return Err(corrupt("The archive is truncated or damaged."));
            }
            break;
        }
        if code != 0 {
            return Err(error(code, &mut state, listing.encrypted));
        }
        let e = entry(&h)?;
        let encrypted = h.flags & RHDF_ENCRYPTED != 0;
        listing.encrypted |= encrypted;
        listing.solid |= h.flags & RHDF_SOLID != 0;
        if encrypted && password.is_none() {
            return Err(ArchiveError::PasswordRequired);
        }
        state.active = if let Some(s) = &mut state.sink {
            s.begin(&e)?
        } else {
            false
        };
        let operation = if state.active { RAR_TEST } else { RAR_SKIP };
        let code =
            unsafe { RARProcessFileW(handle.0, operation, ptr::null_mut(), ptr::null_mut()) };
        if code != 0 {
            return Err(error(code, &mut state, encrypted));
        }
        if state.active
            && let Some(s) = &mut state.sink
        {
            s.end()?;
        }
        listing.entries.push(e);
    }
    // Close while the callback context is still alive.
    drop(handle);
    listing.volumes = std::mem::take(&mut state.volumes);
    Ok(listing)
}
pub(super) fn list(path: &Path, password: Option<&str>) -> Result<Listing, ArchiveError> {
    run(path, password, &AtomicBool::new(false), None)
}
pub(super) fn walk(
    path: &Path,
    password: Option<&str>,
    sink: &mut dyn Sink,
    cancel: &AtomicBool,
) -> Result<(), ArchiveError> {
    run(path, password, cancel, Some(sink)).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_callback() {
        let s = "Игра ü 游戏 🎮";
        let w = wide(s);
        assert_eq!(decode(&w[..w.len() - 1]).unwrap(), s);
        let cancel = AtomicBool::new(false);
        let call = |msg, p1, p2| {
            let mut state = State {
                password: None,
                failure: None,
                volumes: Vec::new(),
                cancel: &cancel,
                sink: None,
                active: false,
            };
            (
                callback(msg, &raw mut state as LParam, p1, p2),
                state.failure,
            )
        };
        // An 8 GiB dictionary (KiB) and an unknown message are refused (anything but 1).
        assert!(matches!(call(UCM_LARGEDICT, 8 << 20, 4 << 20), (r, Some(_)) if r != 1));
        assert!(matches!(call(99, 0, 0), (-1, Some(_))));
        assert!(matches!(call(UCM_PROCESSDATA, 0, 0), (1, None)));
    }
}
