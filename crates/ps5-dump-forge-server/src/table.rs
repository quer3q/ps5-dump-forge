//! What `GET /api/jobs` shows: every unfinished job and the last finished ones, built from
//! core's events. Core can emit a job's events, `Done` included, before `Jobs::start`
//! returns its id, so an entry is made by its first event and the request merged in later.
//! A job's end notice (`notice::ended`) comes from whichever of the two is last.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ps5_dump_forge_core::{ConvertRequest, Edit, Event, JobId};
use serde_json::{Value, json};

use crate::notice::Ended;

const FINISHED: usize = 32;
const LOG_LINES: usize = 200;

#[derive(Default)]
pub(crate) struct Table {
    jobs: BTreeMap<JobId, Entry>,
    /// Finished ids, oldest first.
    finished: VecDeque<JobId>,
}

#[derive(Default)]
struct Entry {
    request: Option<ConvertRequest>,
    /// `Event::Stages`' list; empty until it comes.
    stages: Vec<String>,
    progress: Option<Value>,
    done: Option<Value>,
    /// When `Done` was recorded.
    finished_at: Option<Instant>,
    /// `Done`'s result: the report's output, or the error.
    outcome: Option<Result<PathBuf, String>>,
    log: VecDeque<String>,
    log_total: u64,
}

impl Table {
    /// Keeps `event`; a `Done` of an admitted job gives its end notice, if any.
    pub fn record(&mut self, event: &Event) -> Option<Ended> {
        let (Event::Stages { job, .. }
        | Event::Progress { job, .. }
        | Event::Log { job, .. }
        | Event::Done { job, .. }) = event;
        let entry = self.jobs.entry(*job).or_default();
        match event {
            Event::Stages { stages, .. } => entry.stages = stages.clone(),
            Event::Progress { .. } => entry.progress = serde_json::to_value(event).ok(),
            Event::Log { line, .. } => {
                if entry.log.len() == LOG_LINES {
                    entry.log.pop_front();
                }
                entry.log.push_back(line.clone());
                entry.log_total += 1;
            }
            Event::Done { result, .. } => {
                entry.done = serde_json::to_value(event).ok();
                entry.finished_at = Some(Instant::now());
                let outcome = result
                    .as_ref()
                    .map(|r| r.output.clone())
                    .map_err(Clone::clone);
                let ended =
                    (entry.request.as_ref()).and_then(|r| crate::notice::ended(r, &outcome));
                entry.outcome = Some(outcome);
                self.finished.push_back(*job);
                if self.finished.len() > FINISHED
                    && let Some(old) = self.finished.pop_front()
                {
                    self.jobs.remove(&old);
                }
                return ended;
            }
        }
        None
    }

    /// `Jobs::start` returned `job` for `request`.
    // ponytail: an entry already dropped from history would come back here, but that needs 32
    // jobs to finish while `start` runs, and admission (held across it) caps them at 8.
    /// A job whose `Done` came first gives its end notice here, if any.
    pub fn admit(&mut self, job: JobId, request: ConvertRequest) -> Option<Ended> {
        let entry = self.jobs.entry(job).or_default();
        let ended = (entry.outcome.as_ref()).and_then(|o| crate::notice::ended(&request, o));
        entry.request = Some(request);
        ended
    }

    /// The unfinished jobs' ids and outputs (an edit job's is its source, whose `output` is
    /// ignored; a rebuild writes its `.part` beside the source).
    pub fn unfinished(&self) -> Vec<(JobId, PathBuf)> {
        self.requests()
            .map(|(id, r)| {
                let in_place = !r.edits.is_empty();
                (id, if in_place { &r.source } else { &r.output }.clone())
            })
            .collect()
    }

    /// What the unfinished edit jobs add (`Edit::Add.from`): staged uploads among them.
    pub fn adds(&self) -> Vec<PathBuf> {
        (self.requests().flat_map(|(_, r)| &r.edits))
            .filter_map(|e| match e {
                Edit::Add { from, .. } => Some(from.clone()),
                _ => None,
            })
            .collect()
    }

    fn requests(&self) -> impl Iterator<Item = (JobId, &ConvertRequest)> {
        (self.jobs.iter().filter(|(_, e)| e.done.is_none()))
            .filter_map(|(id, e)| Some((*id, e.request.as_ref()?)))
    }

    #[cfg(test)]
    pub fn all_admitted(&self) -> bool {
        self.jobs.values().all(|e| e.request.is_some())
    }

    /// By id; a job shows once its request is known.
    pub fn snapshot(&self) -> Vec<Value> {
        (self.jobs.iter())
            .filter_map(|(id, e)| e.json(*id))
            .collect()
    }

    /// `snapshot` of the jobs unfinished or finished less than `keep` ago (Connect mirrors
    /// them to the peers they use).
    pub fn recent(&self, keep: Duration) -> Vec<Value> {
        (self.jobs.iter())
            .filter(|(_, e)| e.finished_at.is_none_or(|t| t.elapsed() < keep))
            .filter_map(|(id, e)| e.json(*id))
            .collect()
    }
}

impl Entry {
    fn json(&self, id: JobId) -> Option<Value> {
        Some(json!({
            "id": id,
            "request": self.request.as_ref()?,
            "stages": self.stages,
            "progress": self.progress,
            "done": self.done,
            "log": self.log,
            "log_total": self.log_total,
        }))
    }
}

/// Where unfinished jobs write: core's `part_path`, `<output name>.<job>-<pid>.part` beside
/// the output, in the output's resolved folder (as `stale_parts` lists it).
pub(crate) fn running_parts(running: &[(JobId, PathBuf)]) -> Vec<PathBuf> {
    let pid = std::process::id();
    running
        .iter()
        .filter_map(|(job, output)| {
            let parent = output.parent()?;
            let parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            let name = output.file_name()?.to_str()?;
            Some(parent.join(format!("{name}.{job}-{pid}.part")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5_dump_forge_core::Format;

    fn request(output: &str) -> ConvertRequest {
        ConvertRequest {
            source: "/s".into(),
            format: Format::Exfat,
            output: output.into(),
            compression_threads: None,
            inner: None,
            remove_backport: false,
            full_verify: false,
            kraken_level: ps5_dump_forge_core::KrakenLevel::Fast,
            ffpfsc_level: 6,
            lz4: None,
            zstd_pack: false,
            zstd_level: 5,
            zstd_unpack: false,
            lz4_level: 0,
            lz4_runtime: Default::default(),
            edits: Vec::new(),
            extract: Vec::new(),
            transfer: Vec::new(),
            replace: false,
            edit_rebuild: false,
            install: None,
            archive_password: None,
        }
    }

    #[test]
    fn events_before_admission_and_history() {
        let mut t = Table::default();
        let line = |job, n: usize| Event::Log {
            job,
            line: format!("l{n}"),
        };
        for n in 0..250 {
            t.record(&line(1, n));
        }
        assert!(t.snapshot().is_empty()); // no request yet
        t.admit(1, request("/o/a.exfat"));
        let snap = &t.snapshot()[0];
        assert_eq!(snap["log_total"], 250);
        assert_eq!(snap["log"].as_array().unwrap().len(), 200);
        assert_eq!(snap["log"][0], "l50");
        assert_eq!(snap["stages"], serde_json::json!([])); // until `Stages` comes
        t.record(&Event::Stages {
            job: 1,
            stages: vec!["scan".into(), "write".into()],
        });
        assert_eq!(
            t.snapshot()[0]["stages"],
            serde_json::json!(["scan", "write"])
        );
        assert_eq!(t.unfinished(), [(1, PathBuf::from("/o/a.exfat"))]);
        t.admit(2, request("/o/b.exfat"));
        assert_eq!(t.unfinished()[1], (2, PathBuf::from("/o/b.exfat")));
        // An edit job's `output` is ignored: it edits the source (or rebuilds beside it).
        t.admit(
            3,
            ConvertRequest {
                source: "/i/edit.exfat".into(),
                edits: vec![
                    Edit::Delete { path: "a".into() },
                    Edit::Add {
                        from: "/s/x.bin".into(),
                        to: "x.bin".into(),
                    },
                ],
                ..request("/o/ignored")
            },
        );
        assert_eq!(t.unfinished()[2], (3, PathBuf::from("/i/edit.exfat")));
        assert_eq!(t.adds(), [PathBuf::from("/s/x.bin")]);
        for job in [2, 3] {
            t.record(&Event::Done {
                job,
                result: Err("cancelled".into()),
            });
        }
        assert!(t.adds().is_empty());
        for job in 1..=40 {
            t.admit(job, request("/o/x"));
            t.record(&Event::Done {
                job,
                result: Err("cancelled".into()),
            });
        }
        let snap = t.snapshot();
        assert_eq!(snap.len(), 32);
        assert_eq!(snap[0]["id"], 9);
        assert_eq!(snap[0]["done"]["result"]["Err"], "cancelled");
        assert!(t.unfinished().is_empty());
        // The job list never shows an archive's password.
        t.admit(
            41,
            ConvertRequest {
                edits: Vec::new(),
                archive_password: Some(ps5_dump_forge_core::ArchivePassword::new("Secret1")),
                ..request("/o/y")
            },
        );
        let snap = serde_json::to_string(&t.snapshot()).unwrap();
        assert!(snap.contains("/o/y") && !snap.contains("Secret1"), "{snap}");
    }

    fn done(job: JobId, result: Result<&str, &str>) -> Event {
        Event::Done {
            job,
            result: result
                .map(|output| ps5_dump_forge_core::JobReport {
                    output: output.into(),
                    bytes: 0,
                    files: 0,
                    checks: Vec::new(),
                    verify: ps5_dump_forge_core::VerifySummary {
                        mode: ps5_dump_forge_core::VerifyMode::Fast,
                        checked_bytes: 0,
                        total_bytes: 0,
                        samples: 0,
                        seed: 0,
                    },
                })
                .map_err(str::to_string),
        }
    }

    /// The end notice comes once, from whichever of `admit` and `Done` is last; none for an
    /// edit, an extract or a cancel.
    #[test]
    fn end_notice_once_whichever_comes_last() {
        use crate::notice::{Ended, JobKind};
        let mut t = Table::default();
        // Done before admission (core can emit it before `Jobs::start` returns).
        assert_eq!(t.record(&done(1, Ok("/o/b.exfat"))), None);
        assert_eq!(
            t.admit(1, request("/o/a.exfat")),
            Some(Ended {
                kind: JobKind::Convert,
                ok: true,
                name: "b.exfat".into(),
            })
        );
        // Admitted first: the Done gives it, its name from the request when it failed.
        assert_eq!(t.admit(2, request("/o/c.ffpkg")), None);
        let failed = t.record(&done(2, Err("no room")));
        assert_eq!(
            failed.map(|e| (e.ok, e.name)),
            Some((false, "c.ffpkg".into()))
        );
        // Cancelled, or an extract: nothing.
        t.admit(3, request("/o/d"));
        assert_eq!(t.record(&done(3, Err("cancelled"))), None);
        t.record(&done(4, Ok("/o")));
        let extract = ConvertRequest {
            extract: vec!["sce_sys".into()],
            ..request("/o")
        };
        assert_eq!(t.admit(4, extract), None);
    }
}
