//! Bring-up routes, only with `FORGE_DEBUG_API` set: `POST /api/debug {path}` (the file calls'
//! raw and std results), `GET /probe` and `/api/debug_input` (the input probe).

use std::ffi::{CStr, CString};
use std::fs;
use std::io;

use serde_json::{Value, json};

pub(crate) const ENABLED: bool = option_env!("FORGE_DEBUG_API").is_some();

pub(crate) fn probe(path: &str) -> Value {
    let meta = |m: io::Result<fs::Metadata>| match m {
        Ok(m) => json!({ "dir": m.is_dir(), "file": m.is_file(), "symlink": m.is_symlink(),
            "len": m.len(), "mode": format!("{:o}", mode(&m)) }),
        Err(e) => json!({ "error": e.to_string(), "errno": e.raw_os_error() }),
    };
    let listed: Vec<Value> = match fs::read_dir(path) {
        Ok(dir) => dir
            .take(12)
            .map(|e| match e {
                Ok(e) => json!({
                    "name": e.file_name().to_string_lossy(),
                    "file_type": e.file_type().map(|t| format!("{t:?}")).map_err(|e| e.to_string()),
                    "metadata": meta(e.metadata()),
                }),
                Err(e) => json!({ "error": e.to_string() }),
            })
            .collect(),
        Err(e) => vec![json!({ "error": e.to_string() })],
    };
    json!({
        "path": path,
        "std_metadata": meta(fs::metadata(path)),
        "std_symlink_metadata": meta(fs::symlink_metadata(path)),
        "std_canonicalize": fs::canonicalize(path).map(|p| p.display().to_string())
            .map_err(|e| e.to_string()),
        "std_read_dir": listed,
        "raw": raw::probe(path),
    })
}

fn mode(m: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    m.mode()
}

mod raw {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// `stat`/`lstat` into a buffer of `0xaa`: the untouched tail shows how much the call wrote.
    #[allow(clippy::unnecessary_cast)] // field widths differ between targets
    fn stat(c: &CStr, link: bool) -> Value {
        let mut buf = [0xaau8; 512];
        let ptr = buf.as_mut_ptr().cast::<libc::stat>();
        // SAFETY: `buf` is 512 bytes, larger than any `struct stat`, and suitably aligned for
        // the reads below (`read_unaligned`).
        let rc = unsafe {
            if link {
                libc::lstat(c.as_ptr(), ptr)
            } else {
                libc::stat(c.as_ptr(), ptr)
            }
        };
        let errno = io::Error::last_os_error().raw_os_error();
        let written = buf.iter().rposition(|&b| b != 0xaa).map_or(0, |i| i + 1);
        // SAFETY: the buffer holds a whole `libc::stat`, initialized (0xaa where untouched).
        let st = unsafe { ptr.read_unaligned() };
        json!({ "rc": rc, "errno": errno, "bytes_written": written,
            "hex": hex(&buf[..written.clamp(64, 256)]),
            "decoded": { "st_mode": format!("{:o}", st.st_mode), "st_size": st.st_size,
                "st_ino": st.st_ino as u64, "st_dev": st.st_dev as u64 } })
    }

    fn realpath(c: &CStr) -> Value {
        // SAFETY: a null buffer asks realpath to allocate; freed below.
        let p = unsafe { libc::realpath(c.as_ptr(), std::ptr::null_mut()) };
        if p.is_null() {
            return json!({ "error": io::Error::last_os_error().to_string() });
        }
        // SAFETY: realpath returned a NUL-terminated malloc'd string.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { libc::free(p.cast()) };
        json!(s)
    }

    /// The first records `readdir` returns: their header bytes and the name at `d_name`.
    fn readdir(c: &CStr) -> Value {
        // SAFETY: plain opendir/readdir/closedir on a C string; each record is read only within
        // its header and its NUL-terminated name.
        unsafe {
            let dir = libc::opendir(c.as_ptr());
            if dir.is_null() {
                return json!({ "error": io::Error::last_os_error().to_string() });
            }
            let mut out = Vec::new();
            for _ in 0..12 {
                let e = libc::readdir(dir);
                if e.is_null() {
                    break;
                }
                let head = std::slice::from_raw_parts(e.cast::<u8>(), 32);
                let name = CStr::from_ptr((*e).d_name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                out.push(json!({ "hex32": hex(head), "d_reclen": (*e).d_reclen,
                    "d_type": (*e).d_type, "name": name }));
            }
            libc::closedir(dir);
            json!(out)
        }
    }

    pub(super) fn probe(path: &str) -> Value {
        let Ok(c) = CString::new(path) else {
            return json!({ "error": "path holds a NUL" });
        };
        json!({
            "size_of_stat": std::mem::size_of::<libc::stat>(),
            "size_of_dirent": std::mem::size_of::<libc::dirent>(),
            "stat": stat(&c, false),
            "lstat": stat(&c, true),
            "realpath": realpath(&c),
            "readdir": readdir(&c),
        })
    }
}

/// Every input event the console's browser gives it, each `POST`ed to `/api/debug_input`.
pub(crate) const PROBE_PAGE: &[u8] = br##"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Forge input probe</title>
<style>body{background:#111;color:#eee;font:18px monospace;margin:16px}#head{color:#8f8}
#log{white-space:pre-wrap}input{font-size:18px;margin:8px 0}</style></head>
<body><div id="head">probe on</div><p><a href="/" style="color:#9cf;font-size:22px">Back to Dump Forge</a></p>
<input id="field" placeholder="focus me, then press buttons too">
<div id="log"></div>
<script>
var log = document.getElementById("log"), head = document.getElementById("head"), lines = [];
function add(s) {
  lines.unshift(new Date().toISOString().slice(11, 23) + " " + s);
  lines.length = Math.min(lines.length, 30);
  log.textContent = lines.join("\n");
  try { fetch("/api/debug_input", { method: "POST", body: s }); } catch (e) {}
}
// Something to go back to, so a "back" button shows as popstate instead of leaving.
for (var i = 0; i < 20; i++) history.pushState({ i: i }, "", "#" + i);
var mods = function (e) { return (e.ctrlKey ? " ctrl" : "") + (e.altKey ? " alt" : "") + (e.shiftKey ? " shift" : "") + (e.metaKey ? " meta" : ""); };
["keydown", "keyup", "keypress"].forEach(function (t) {
  window.addEventListener(t, function (e) {
    add(t + " key=" + JSON.stringify(e.key) + " code=" + JSON.stringify(e.code) + " keyCode=" + e.keyCode + " which=" + e.which + " loc=" + e.location + mods(e) + (e.repeat ? " repeat" : ""));
  }, true);
});
["mousedown", "mouseup", "click", "auxclick", "contextmenu", "pointerdown", "pointerup", "touchstart"].forEach(function (t) {
  window.addEventListener(t, function (e) { add(t + " button=" + e.button + " buttons=" + e.buttons + " type=" + (e.pointerType || "") + " at " + e.clientX + "," + e.clientY); }, true);
});
window.addEventListener("wheel", function (e) { add("wheel dx=" + e.deltaX + " dy=" + e.deltaY + " dz=" + e.deltaZ + " mode=" + e.deltaMode + mods(e)); }, true);
["focus", "blur", "popstate", "hashchange", "pageshow", "pagehide", "resize", "gamepadconnected", "gamepaddisconnected"].forEach(function (t) {
  window.addEventListener(t, function (e) { add(t + (e.gamepad ? " " + JSON.stringify(e.gamepad.id) + " mapping=" + e.gamepad.mapping : "") + (t === "hashchange" ? " " + location.hash : "")); }, true);
});
document.addEventListener("visibilitychange", function () { add("visibility " + document.visibilityState); });
var lastScroll = 0;
window.addEventListener("scroll", function () { var n = Date.now(); if (n - lastScroll > 500) { lastScroll = n; add("scroll y=" + scrollY); } }, true);
var getPads = navigator.getGamepads ? function () { return navigator.getGamepads(); } : navigator.webkitGetGamepads ? function () { return navigator.webkitGetGamepads(); } : null;
var seen = {};
function poll() {
  var pads = getPads ? getPads() : [], n = 0;
  for (var i = 0; i < pads.length; i++) {
    var p = pads[i]; if (!p) continue; n++;
    var before = seen[p.index] || { b: [], a: [] };
    var b = [], a = [];
    for (var j = 0; j < p.buttons.length; j++) {
      var v = typeof p.buttons[j] === "object" ? p.buttons[j].value : p.buttons[j];
      b.push(v);
      if ((v > 0.5) !== ((before.b[j] || 0) > 0.5)) add("pad" + p.index + " button " + j + (v > 0.5 ? " down " : " up ") + v.toFixed(2) + " id=" + JSON.stringify(p.id) + " mapping=" + p.mapping);
    }
    for (var k = 0; k < p.axes.length; k++) {
      var x = p.axes[k]; a.push(x);
      if (Math.abs(x - (before.a[k] || 0)) > 0.5) add("pad" + p.index + " axis " + k + " " + x.toFixed(2));
    }
    seen[p.index] = { b: b, a: a };
  }
  head.textContent = "probe on | gamepad API: " + (navigator.getGamepads ? "yes" : navigator.webkitGetGamepads ? "webkit" : "no") + " | pads: " + n + " | " + document.visibilityState + " | focus: " + document.hasFocus() + " | " + navigator.userAgent;
  requestAnimationFrame(poll);
}
add("loaded " + navigator.userAgent);
requestAnimationFrame(poll);
</script></body></html>
"##;

static INPUT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// At most 1 KiB, the last 500 kept.
pub(crate) fn input_push(body: &[u8]) {
    let line = String::from_utf8_lossy(&body[..body.len().min(1024)]).into_owned();
    let mut all = INPUT.lock().unwrap_or_else(|e| e.into_inner());
    if all.len() == 500 {
        all.remove(0);
    }
    all.push(line);
}

pub(crate) fn input_lines() -> Value {
    json!(*INPUT.lock().unwrap_or_else(|e| e.into_inner()))
}
