//! Jobs mirrored to the peers they use: a job on this device whose paths name a paired peer
//! (`flink://<key_id>`) is shown on that peer too, read only, where its user may only cancel
//! it. Every `PUSH_EVERY` this device sends each such peer a `jobs` RPC with its jobs (paths in
//! this device's share root spelled `flink://<this key_id>/...`); the peer keeps the list for
//! `KEEP_RECEIVED`, spells its own paths back as local ones, and answers with the ids its user
//! asked to cancel. ponytail: a fresh control conn per push, as the prober does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ps5_dump_forge_core::remote::{FlinkPath, Scheme, is_flink};
use ps5_dump_forge_core::{ConvertRequest, Event, JobId};
use serde_json::{Value, json};

use crate::table::Table;

/// How often this device's jobs are pushed to the peers they use.
pub(crate) const PUSH_EVERY: Duration = Duration::from_secs(2);
/// A finished job stays on the peer this long.
const KEEP_FINISHED: Duration = Duration::from_secs(30);
/// A peer's list is dropped when not refreshed this long.
const KEEP_RECEIVED: Duration = Duration::from_secs(10);
/// Jobs kept per peer.
const MAX_RECEIVED: usize = 32;
/// Log lines sent per job.
const LOG_LINES: usize = 50;

/// Cancels one of this device's jobs (core's `Jobs::cancel`).
pub type CancelHook = Arc<dyn Fn(JobId) + Send + Sync>;

/// Both sides: this device's jobs and what it pushed, and the peers' lists.
#[derive(Default)]
pub(crate) struct Mirror {
    table: Mutex<Table>,
    /// The peers sent a non-empty list last time, so one whose last job left gets one empty
    /// push.
    pushed: Mutex<HashSet<String>>,
    cancel: Mutex<Option<CancelHook>>,
    /// The peers a push to is still going (one at a time per peer).
    going: Mutex<HashSet<String>>,
    /// Peer key id → its jobs here.
    received: Mutex<HashMap<String, Received>>,
}

struct Received {
    at: Instant,
    jobs: Vec<Value>,
    /// The run of the peer that sent it ([`instance`]): its job ids start at 1 each run.
    instance: String,
    /// Ids this device's user asked to cancel; kept while the job is listed.
    cancel: HashSet<JobId>,
}

/// One push: the peer, the RPC body and the unfinished jobs in it (the only ones its reply
/// may cancel).
pub(crate) struct Push {
    pub key_id: String,
    pub body: Value,
    pub cancellable: Vec<JobId>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// This run of the process, in every push: job ids start at 1 each run, so the peer tells a
/// new job 1 from the last run's.
fn instance() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        format!(
            "{:x}-{}",
            now.map_or(0, |d| d.as_nanos()),
            std::process::id()
        )
    })
}

/// The path strings a job names, as `f` may rewrite them: `request`'s source, output,
/// transfer, added files (`edits[].from`, op `add`), and the done
/// report's output.
fn map_paths(job: &mut Value, f: &mut dyn FnMut(&str) -> Option<String>) {
    let mut each = |v: &mut Value| {
        if let Some(new) = v.as_str().and_then(&mut *f) {
            *v = Value::String(new);
        }
    };
    let r = &mut job["request"];
    if r.is_object() {
        for key in ["source", "output", "install"] {
            if let Some(v) = r.get_mut(key) {
                each(v);
            }
        }
        if let Some(list) = r.get_mut("transfer").and_then(Value::as_array_mut) {
            list.iter_mut().for_each(&mut each);
        }
        if let Some(edits) = r.get_mut("edits").and_then(Value::as_array_mut) {
            for e in edits.iter_mut().filter(|e| e["op"] == "add") {
                if let Some(v) = e.get_mut("from") {
                    each(v);
                }
            }
        }
    }
    if let Some(v) = job.pointer_mut("/done/result/Ok/output") {
        each(v);
    }
}

/// The paired peers' key ids among the `flink://` paths `request` names.
fn peers_of(request: &Value) -> HashSet<String> {
    let mut keys = HashSet::new();
    let mut job = json!({ "request": request });
    map_paths(&mut job, &mut |s| {
        let p = Path::new(s);
        if is_flink(p)
            && let Ok(f) = FlinkPath::parse(p)
            && f.scheme == Scheme::Flink
        {
            keys.insert(f.key_id);
        }
        None
    });
    keys
}

/// A local absolute path inside `root` as this device's `flink://<me>/...`; `None` otherwise.
// ponytail: a path outside the share root (Windows: another drive) goes as it is, shown on the
// peer as a plain path of this device's (display only: the peer never opens it).
fn to_flink(me: &str, root: &Path, s: &str) -> Option<String> {
    let rel = relative(root, s)?;
    Some(match rel.is_empty() {
        true => format!("flink://{me}"),
        false => format!("flink://{me}/{rel}"),
    })
}

/// A local absolute path inside `root` as `/`-joined names relative to it (`""` for `root`
/// itself); `None` outside it or for a name that isn't UTF-8.
pub(crate) fn relative(root: &Path, s: &str) -> Option<String> {
    let p = Path::new(s);
    if !p.is_absolute() {
        return None;
    }
    let rel = match p.strip_prefix(root) {
        Ok(rel) => rel.to_path_buf(),
        Err(_) => resolved(p)?.strip_prefix(root).ok()?.to_path_buf(),
    };
    let parts = (rel.components())
        .map(|c| match c {
            std::path::Component::Normal(n) => n.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// `p` resolved, or its folder resolved when `p` isn't there (yet).
fn resolved(p: &Path) -> Option<PathBuf> {
    dunce::canonicalize(p)
        .ok()
        .or_else(|| Some(dunce::canonicalize(p.parent()?).ok()?.join(p.file_name()?)))
}

/// This device's `flink://<me>/...` (from a peer) as the local path in `root`.
fn from_flink(me: &str, root: &Path, s: &str) -> Option<String> {
    let p = Path::new(s);
    if !is_flink(p) {
        return None;
    }
    let f = FlinkPath::parse(p).ok()?;
    if f.scheme != Scheme::Flink || f.key_id != me {
        return None;
    }
    let mut out = root.to_path_buf();
    out.extend(f.path.split('/').filter(|part| !part.is_empty()));
    Some(out.to_string_lossy().into_owned())
}

impl Mirror {
    /// `Jobs::start` returned `id` for `request` (its password never kept).
    pub fn started(&self, id: JobId, request: &ConvertRequest) {
        let mut request = request.clone();
        request.archive_password = None;
        lock(&self.table).admit(id, request);
    }

    pub fn event(&self, event: &Event) {
        lock(&self.table).record(event);
    }

    pub fn set_cancel(&self, cancel: CancelHook) {
        *lock(&self.cancel) = Some(cancel);
    }

    /// The pushes due now: each peer `paired` says is paired gets the jobs that use it
    /// (unfinished or finished less than `KEEP_FINISHED` ago), paths in `root` spelled as
    /// `flink://<me>/...`; a peer sent some last time and none now gets an empty list.
    pub fn due(&self, me: &str, root: Option<&Path>, paired: &dyn Fn(&str) -> bool) -> Vec<Push> {
        let jobs = lock(&self.table).recent(KEEP_FINISHED);
        let mut by_peer: HashMap<String, Push> = HashMap::new();
        for mut job in jobs {
            let keys = peers_of(&job["request"]);
            let Some(id) = job["id"].as_u64() else {
                continue;
            };
            if keys.is_empty() {
                continue;
            }
            let unfinished = job["done"].is_null();
            if let Some(o) = job.as_object_mut() {
                o.remove("log_total");
            }
            if let Some(log) = job["log"].as_array_mut() {
                log.drain(..log.len().saturating_sub(LOG_LINES));
            }
            if let Some(root) = root {
                map_paths(&mut job, &mut |s| to_flink(me, root, s));
            }
            for key in keys.into_iter().filter(|k| paired(k)) {
                let push = by_peer.entry(key.clone()).or_insert_with(|| Push {
                    key_id: key,
                    body: json!({ "instance": instance(), "jobs": [] }),
                    cancellable: Vec::new(),
                });
                if let Some(list) = push.body["jobs"].as_array_mut() {
                    list.push(job.clone());
                }
                if unfinished {
                    push.cancellable.push(id);
                }
            }
        }
        let mut pushed = lock(&self.pushed);
        let mut pushes: Vec<Push> = (pushed.iter())
            .filter(|k| !by_peer.contains_key(*k) && paired(k))
            .map(|k| Push {
                key_id: k.clone(),
                body: json!({ "instance": instance(), "jobs": [] }),
                cancellable: Vec::new(),
            })
            .collect();
        *pushed = by_peer.keys().cloned().collect();
        pushes.extend(by_peer.into_values());
        pushes
    }

    /// Whether a push to `key_id` may start now (none to it is still going); then
    /// [`Self::end_push`] when it ends.
    pub fn begin_push(&self, key_id: &str) -> bool {
        lock(&self.going).insert(key_id.to_string())
    }

    pub fn end_push(&self, key_id: &str) {
        lock(&self.going).remove(key_id);
    }

    /// A peer's `reply` to `push`: cancels each job it names that the push could cancel.
    pub fn replied(&self, push: &Push, reply: &Value) {
        let ids: Vec<JobId> = (reply["cancel"].as_array().into_iter().flatten())
            .filter_map(Value::as_u64)
            .filter(|id| push.cancellable.contains(id))
            .collect();
        // Not under a lock: cancelling may emit an event, which `event` records.
        let hook = lock(&self.cancel).clone();
        if let Some(cancel) = hook {
            ids.into_iter().for_each(|id| cancel(id));
        }
    }

    /// A peer's `jobs` RPC: its list replaces the last one (at most `MAX_RECEIVED`), this
    /// device's `flink://<me>/...` paths spelled as paths in `root`. The ids to cancel.
    pub fn received(
        &self,
        key_id: &str,
        body: &Value,
        me: &str,
        root: Option<&Path>,
    ) -> Result<Value, String> {
        let list = body["jobs"].as_array().ok_or("jobs: not a list")?;
        // Unfinished jobs first, so finished ones never push a running one past the cap.
        let mut jobs: Vec<Value> = list.iter().filter(|j| j["id"].is_u64()).cloned().collect();
        jobs.sort_by_key(|j| !j["done"].is_null());
        jobs.truncate(MAX_RECEIVED);
        jobs.sort_by_key(|j| j["id"].as_u64());
        if let Some(root) = root {
            for job in &mut jobs {
                map_paths(job, &mut |s| from_flink(me, root, s));
            }
        }
        let ids: HashSet<JobId> = jobs.iter().filter_map(|j| j["id"].as_u64()).collect();
        let mut all = lock(&self.received);
        // Job ids start again at 1 when the peer restarts: marks of another run, or of an
        // expired list, could name a new job.
        let instance = body["instance"].as_str().unwrap_or_default().to_string();
        let mut cancel = (all.remove(key_id))
            .filter(|r| r.at.elapsed() < KEEP_RECEIVED && r.instance == instance)
            .map(|r| r.cancel)
            .unwrap_or_default();
        cancel.retain(|id| ids.contains(id));
        let mut asked: Vec<JobId> = cancel.iter().copied().collect();
        asked.sort_unstable();
        if !jobs.is_empty() {
            let at = Instant::now();
            let r = Received {
                at,
                instance,
                jobs,
                cancel,
            };
            all.insert(key_id.to_string(), r);
        }
        Ok(json!({ "cancel": asked }))
    }

    /// Asks `key_id` to cancel its listed `job` (sent with the reply to its next push).
    pub fn cancel_remote(&self, key_id: &str, job: JobId) -> bool {
        let mut all = lock(&self.received);
        match all.get_mut(key_id) {
            Some(r) if r.at.elapsed() < KEEP_RECEIVED && r.jobs.iter().any(|j| j["id"] == job) => {
                r.cancel.insert(job);
                true
            }
            _ => false,
        }
    }

    /// `key_id` was forgotten: its jobs go, and it gets no more pushes.
    pub fn forget(&self, key_id: &str) {
        lock(&self.received).remove(key_id);
        lock(&self.pushed).remove(key_id);
    }

    /// The peers' jobs here, by peer key id, each with its saved `name` (a peer without one
    /// is left out).
    pub fn remote_jobs(&self, name_of: &dyn Fn(&str) -> Option<String>) -> Vec<Value> {
        let mut all = lock(&self.received);
        all.retain(|_, r| r.at.elapsed() < KEEP_RECEIVED);
        let mut keys: Vec<&String> = all.keys().collect();
        keys.sort();
        let mut out = Vec::new();
        for key in keys {
            let (r, Some(name)) = (&all[key], name_of(key)) else {
                continue;
            };
            for job in &r.jobs {
                let id = job["id"].as_u64().unwrap_or_default();
                out.push(json!({
                    "key_id": key,
                    "name": name,
                    "instance": r.instance,
                    "cancelling": r.cancel.contains(&id),
                    "job": job,
                }));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::{Connect, pair_for_test};
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-jobs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn request(source: &Path, output: &str) -> ConvertRequest {
        serde_json::from_value(json!({
            "source": source, "format": "folder", "output": output,
        }))
        .unwrap()
    }

    #[test]
    fn an_install_of_a_peer_package_is_shown_on_that_peer() {
        let key = "ab".repeat(16);
        let request = json!({ "install": format!("flink://{key}/Users/me/game.pkg") });
        assert_eq!(peers_of(&request), HashSet::from([key]));
    }

    #[test]
    fn a_job_is_shown_on_the_peer_it_uses_and_cancelled_from_there() {
        let dirs = ["a", "b", "share-a", "share-b"].map(temp_dir);
        let a = Connect::start_rooted(&dirs[0], 0, Some(dirs[2].clone())).unwrap();
        let b = Connect::start_rooted(&dirs[1], 0, Some(dirs[3].clone())).unwrap();
        let addr_a = format!("127.0.0.1:{}", a.me().port);
        assert_eq!(pair_for_test(&b, &a, &addr_a, true).state, "paired");
        let (ka, kb) = (a.me().key_id.clone(), b.me().key_id.clone());
        let cancelled = Arc::new(Mutex::new(Vec::new()));
        let seen = cancelled.clone();
        a.set_cancel(Arc::new(move |id| lock(&seen).push(id)));

        // Job 1 writes onto B; job 2 stays on A.
        a.job_started(
            1,
            &request(&dirs[2].join("game"), &format!("flink://{kb}/x")),
        );
        a.job_event(&Event::Progress {
            job: 1,
            stage: "write".into(),
            done: 1,
            total: 2,
        });
        let out = dirs[2].join("out").to_string_lossy().into_owned();
        a.job_started(2, &request(&dirs[2].join("other"), &out));
        a.push_jobs();

        let shown = b.snapshot()["remote_jobs"].clone();
        let shown = shown.as_array().unwrap();
        assert_eq!(shown.len(), 1, "{shown:?}");
        let (entry, job) = (&shown[0], &shown[0]["job"]);
        assert_eq!(entry["key_id"], ka.as_str());
        assert_eq!(entry["name"], a.me().name.as_str());
        assert_eq!(entry["cancelling"], false);
        assert_eq!(job["id"], 1);
        assert_eq!(job["progress"]["done"], 1);
        assert!(job.get("log_total").is_none());
        let share_b = dunce::canonicalize(&dirs[3]).unwrap();
        assert_eq!(
            job["request"]["output"],
            share_b.join("x").to_str().unwrap()
        );
        assert_eq!(job["request"]["source"], format!("flink://{ka}/game"));
        // A's own snapshot shows none of its jobs as a peer's.
        assert_eq!(a.snapshot()["remote_jobs"], json!([]));

        // B asks to cancel: only a listed job; A cancels it on its next push.
        assert!(!b.cancel_remote_job(&ka, 2));
        assert!(!b.cancel_remote_job(&kb, 1));
        assert!(b.cancel_remote_job(&ka, 1));
        assert_eq!(b.snapshot()["remote_jobs"][0]["cancelling"], true);
        assert!(lock(&cancelled).is_empty());
        a.push_jobs();
        assert_eq!(*lock(&cancelled), [1]);

        // Forgotten: B drops A's jobs at once.
        assert!(b.forget(&ka).unwrap());
        assert_eq!(b.snapshot()["remote_jobs"], json!([]));

        for s in [a, b] {
            s.shutdown();
        }
        for d in dirs {
            fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn pushes_go_to_paired_peers_and_end_with_an_empty_one() {
        let m = Mirror::default();
        let (k1, k2) = ("1".repeat(32), "2".repeat(32));
        let out = format!("flink://{k1}/o");
        let root = dunce::canonicalize(temp_dir("unit")).unwrap();
        m.started(7, &request(&root.join("g"), &out));
        m.started(8, &request(&root.join("g"), &format!("flink://{k2}/o")));
        let paired = |k: &str| k == k1 || k == k2;
        // A local path in the share root is spelled as this device's.
        let pushes = m.due("f".repeat(32).as_str(), Some(&root), &|k| k == k1);
        assert_eq!(pushes.len(), 1);
        assert_eq!(
            (pushes[0].key_id.as_str(), &pushes[0].cancellable[..]),
            (k1.as_str(), &[7][..])
        );
        let job = &pushes[0].body["jobs"][0];
        assert_eq!(
            job["request"]["source"],
            format!("flink://{}/g", "f".repeat(32))
        );
        assert_eq!(job["request"]["output"], out.as_str());
        // A peer sent jobs last time and none now gets one empty list, then nothing.
        lock(&m.table).record(&Event::Done {
            job: 7,
            result: Err("cancelled".into()),
        });
        let pushes = m.due("f", None, &paired);
        let cancellable = |k: &str| {
            let p = pushes.iter().find(|p| p.key_id == k).unwrap();
            (
                p.body["jobs"].as_array().unwrap().len(),
                p.cancellable.clone(),
            )
        };
        // A finished job is still shown, but not cancelled from there.
        assert_eq!(
            (cancellable(&k1), cancellable(&k2)),
            ((1, vec![]), (1, vec![8]))
        );
        *lock(&m.table) = Table::default();
        let pushes = m.due("f", None, &paired);
        assert_eq!(pushes.len(), 2);
        assert!(pushes.iter().all(|p| p.body["jobs"] == json!([])));
        assert!(m.due("f", None, &paired).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_received_list_is_checked_and_capped() {
        let m = Mirror::default();
        let me = "0".repeat(32);
        let root = Path::new("/r");
        assert!(
            m.received("k", &json!({ "jobs": 1 }), &me, Some(root))
                .is_err()
        );
        assert!(m.received("k", &json!([]), &me, Some(root)).is_err());
        let jobs: Vec<Value> = (0..40)
            .map(|id| json!({ "id": id, "request": { "output": format!("flink://{me}/o/{id}") } }))
            .collect();
        m.received("k", &json!({ "jobs": jobs }), &me, Some(root))
            .unwrap();
        let shown = m.remote_jobs(&|_| Some("A".into()));
        assert_eq!(shown.len(), MAX_RECEIVED);
        let want = Path::new("/r").join("o").join("3");
        assert_eq!(shown[3]["job"]["request"]["output"], want.to_str().unwrap());
        // A cancel mark is answered while the job is listed, then dropped with it.
        assert!(m.cancel_remote("k", 3));
        let reply = m
            .received("k", &json!({ "jobs": jobs }), &me, None)
            .unwrap();
        assert_eq!(reply, json!({ "cancel": [3] }));
        let reply = m.received("k", &json!({ "jobs": [] }), &me, None).unwrap();
        assert_eq!(reply, json!({ "cancel": [] }));
        assert!(m.remote_jobs(&|_| Some("A".into())).is_empty());
        assert!(!m.cancel_remote("k", 3));
        // A mark of the peer's last run never cancels its new run's job with that id.
        let run = |i: &str| json!({ "instance": i, "jobs": [{ "id": 3 }] });
        m.received("k", &run("one"), &me, None).unwrap();
        assert!(m.cancel_remote("k", 3));
        let reply = m.received("k", &run("two"), &me, None).unwrap();
        assert_eq!(reply, json!({ "cancel": [] }));
        assert_eq!(m.remote_jobs(&|_| Some("A".into()))[0]["instance"], "two");
        // Finished jobs never push a running one past the cap.
        let jobs: Vec<Value> = (0..40)
            .map(|id| json!({ "id": id, "done": if id < 39 { json!({}) } else { Value::Null } }))
            .collect();
        m.received("k", &json!({ "jobs": jobs }), &me, None)
            .unwrap();
        let shown = m.remote_jobs(&|_| Some("A".into()));
        assert_eq!(shown.len(), MAX_RECEIVED);
        assert_eq!(shown[MAX_RECEIVED - 1]["job"]["id"], 39);
    }
}
