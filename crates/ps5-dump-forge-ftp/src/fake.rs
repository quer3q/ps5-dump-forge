//! A small in-process FTP server for the tests: PASV only (EPSV answered 502), no MLST, one
//! thread per control connection, serving a temporary folder.

use std::fs;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Fake {
    pub port: u16,
    pub root: PathBuf,
    conns: Arc<Mutex<Vec<TcpStream>>>,
}

impl Fake {
    /// Closes every control connection, as a server does to idle sessions.
    pub fn kick(&self) {
        for c in self.conns.lock().unwrap().drain(..) {
            let _ = c.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Starts a server on 127.0.0.1 over a fresh empty folder.
pub fn start() -> Fake {
    static N: AtomicU32 = AtomicU32::new(0);
    let root = std::env::temp_dir().join(format!(
        "forge-ftp-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = root.clone();
    let conns = Arc::new(Mutex::new(Vec::new()));
    let kept = conns.clone();
    std::thread::spawn(move || {
        for ctl in listener.incoming().flatten() {
            if let Ok(c) = ctl.try_clone() {
                kept.lock().unwrap().push(c);
            }
            let root = served.clone();
            std::thread::spawn(move || {
                let _ = serve(ctl, &root);
            });
        }
    });
    Fake { port, root, conns }
}

struct State<'a> {
    root: &'a Path,
    cwd: String,
    pasv: Option<TcpListener>,
    rest: u64,
    rnfr: Option<PathBuf>,
}

fn send(w: &mut TcpStream, line: &str) -> io::Result<()> {
    w.write_all(format!("{line}\r\n").as_bytes())
}

fn serve(ctl: TcpStream, root: &Path) -> io::Result<()> {
    ctl.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut r = BufReader::new(ctl.try_clone()?);
    let mut w = ctl;
    let mut s = State {
        root,
        cwd: "/".into(),
        pasv: None,
        rest: 0,
        rnfr: None,
    };
    send(&mut w, "220 fake FTP ready")?;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let verb = verb.to_ascii_uppercase();
        if verb == "QUIT" {
            return send(&mut w, "221 bye");
        }
        command(&mut s, &mut w, &verb, arg)?;
    }
}

impl State<'_> {
    /// A path (absolute, or relative to the current folder) under the root; `..` refused.
    fn resolve(&self, arg: &str) -> Option<(String, PathBuf)> {
        let joined = if arg.starts_with('/') {
            arg.to_string()
        } else {
            format!("{}/{arg}", self.cwd.trim_end_matches('/'))
        };
        let mut virt = String::new();
        let mut real = self.root.to_path_buf();
        for c in Path::new(&joined).components() {
            match c {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(n) => {
                    virt.push('/');
                    virt.push_str(n.to_str()?);
                    real.push(n);
                }
                _ => return None,
            }
        }
        Some((if virt.is_empty() { "/".into() } else { virt }, real))
    }

    /// Accepts the data connection PASV opened.
    fn accept(&mut self) -> io::Result<TcpStream> {
        let l = self
            .pasv
            .take()
            .ok_or_else(|| io::Error::other("no PASV"))?;
        let (data, _) = l.accept()?;
        data.set_read_timeout(Some(Duration::from_secs(30)))?;
        data.set_write_timeout(Some(Duration::from_secs(30)))?;
        Ok(data)
    }
}

const NOT_FOUND: &str = "550 No such file or directory.";

fn command(s: &mut State, w: &mut TcpStream, verb: &str, arg: &str) -> io::Result<()> {
    let found = s.resolve(arg);
    let real = found.as_ref().map(|(_, p)| p.clone());
    match verb {
        "USER" => send(w, "331 password please"),
        "PASS" => send(w, "230 logged in"),
        "FEAT" => send(
            w,
            "211-Features:\r\n SIZE\r\n REST STREAM\r\n UTF8\r\n211 End",
        ),
        "OPTS" | "TYPE" => send(w, "200 ok"),
        "PWD" => send(w, &format!("257 \"{}\" is the current directory", s.cwd)),
        "CWD" => match found {
            Some((virt, p)) if p.is_dir() => {
                s.cwd = virt;
                send(w, "250 ok")
            }
            _ => send(w, NOT_FOUND),
        },
        "EPSV" => send(w, "502 not here"),
        "PASV" => {
            let l = TcpListener::bind("127.0.0.1:0")?;
            let port = l.local_addr()?.port();
            s.pasv = Some(l);
            send(
                w,
                &format!(
                    "227 Entering Passive Mode (127,0,0,1,{},{})",
                    port >> 8,
                    port & 255
                ),
            )
        }
        "LIST" => match real.filter(|p| p.is_dir()) {
            Some(p) => {
                send(w, "150 here it comes")?;
                let mut data = s.accept()?;
                let mut out = String::from("total 1\r\n");
                for e in fs::read_dir(p)? {
                    let e = e?;
                    let m = e.metadata()?;
                    let kind = if m.is_dir() {
                        "drwxr-xr-x"
                    } else {
                        "-rw-r--r--"
                    };
                    let name = e.file_name().into_string().unwrap();
                    // As a server with a row format no parser reads: `nosize-odd*` names.
                    if name.starts_with("nosize-odd") {
                        out += &format!("odd-row {name}\r\n");
                        continue;
                    }
                    out += &format!("{kind} 1 ftp ftp {} Jan  1  2020 {name}\r\n", m.len());
                }
                data.write_all(out.as_bytes())?;
                drop(data);
                send(w, "226 done")
            }
            None => send(w, NOT_FOUND),
        },
        // As a server that denies SIZE for some files: a name starting `nosize` gets none.
        "SIZE"
            if arg
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with("nosize")) =>
        {
            send(w, "550 SIZE not allowed")
        }
        "SIZE" => match real.filter(|p| p.is_file()) {
            Some(p) => send(w, &format!("213 {}", fs::metadata(p)?.len())),
            None => send(w, NOT_FOUND),
        },
        "REST" => match arg.parse() {
            Ok(n) => {
                s.rest = n;
                send(w, "350 restarting")
            }
            Err(_) => send(w, "501 bad offset"),
        },
        "RETR" => {
            let rest = std::mem::take(&mut s.rest);
            let Some(mut f) = real.and_then(|p| fs::File::open(p).ok()) else {
                return send(w, NOT_FOUND);
            };
            f.seek(SeekFrom::Start(rest))?;
            send(w, "150 opening")?;
            let mut data = s.accept()?;
            // As a server that follows a cut transfer's 426 with a 226: `chatty*` names.
            if arg
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with("chatty"))
            {
                let _ = io::copy(&mut f, &mut data);
                drop(data);
                return send(w, "426 cut\r\n226 abort done");
            }
            match io::copy(&mut f, &mut data) {
                Ok(_) => send(w, "226 done"),
                Err(_) => send(w, "426 connection closed; transfer aborted"),
            }
        }
        // A restarted STOR overwrites from its offset and cuts nothing (as vsftpd does).
        "STOR" => {
            let rest = std::mem::take(&mut s.rest);
            let open = |p: &std::path::Path| match rest {
                0 => fs::File::create(p),
                n => fs::OpenOptions::new()
                    .write(true)
                    .open(p)
                    .and_then(|mut f| {
                        f.seek(SeekFrom::Start(n))?;
                        Ok(f)
                    }),
            };
            let Some(mut f) = real.and_then(|p| open(&p).ok()) else {
                return send(w, "553 can't create");
            };
            send(w, "150 send it")?;
            let mut data = s.accept()?;
            io::copy(&mut data, &mut f)?;
            send(w, "226 stored")
        }
        "MKD" => match found {
            Some((_, p)) if p.exists() => send(w, "550 File exists"),
            Some((virt, p)) if fs::create_dir(&p).is_ok() => {
                send(w, &format!("257 \"{virt}\" created"))
            }
            _ => send(w, "550 can't create"),
        },
        "RNFR" => match real.filter(|p| p.exists()) {
            Some(p) => {
                s.rnfr = Some(p);
                send(w, "350 ready for RNTO")
            }
            None => send(w, NOT_FOUND),
        },
        "RNTO" => match (s.rnfr.take(), real) {
            (Some(from), Some(to)) if fs::rename(&from, &to).is_ok() => send(w, "250 renamed"),
            _ => send(w, "553 rename failed"),
        },
        // As a server that lets a file be stored but not deleted: `nodele*` names.
        "DELE"
            if arg
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with("nodele")) =>
        {
            send(w, "550 Permission denied")
        }
        "DELE" => match real.filter(|p| p.is_file()) {
            Some(p) if fs::remove_file(&p).is_ok() => send(w, "250 deleted"),
            _ => send(w, NOT_FOUND),
        },
        // vsftpd's one answer to every failure, so the client finds out why itself.
        "RMD" => match real.filter(|p| p.is_dir()) {
            Some(p) if fs::remove_dir(&p).is_ok() => send(w, "250 removed"),
            _ => send(w, "550 Remove directory operation failed."),
        },
        "ABOR" => send(w, "225 no transfer to abort"),
        "NOOP" => send(w, "200 ok"),
        _ => send(w, "502 not implemented"),
    }
}
