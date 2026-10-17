//! What `GET /api/jobs` shows: every unfinished job and the last finished ones, built from
//! core's events. Core can emit a job's events, `Done` included, before `Jobs::start`
//! returns its id, so an entry is made by its first event and the request merged in later.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use ps5_dump_forge_core::{ConvertRequest, Edit, Event, JobId};
use serde_json::{Value, json};

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
    log: VecDeque<String>,
    log_total: u64,
}

impl Table {
    pub fn record(&mut self, event: &Event) {
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
            Event::Done { .. } => {
                entry.done = serde_json::to_value(event).ok();
                self.finished.push_back(*job);
                if self.finished.len() > FINISHED
                    && let Some(old) = self.finished.pop_front()
                {
                    self.jobs.remove(&old);
                }
            }
        }
    }

    /// `Jobs::start` returned `job` for `request`.
    // ponytail: an entry already dropped from history would come back here, but that needs 32
    // jobs to finish while `start` runs, and admission (held across it) caps them at 8.
    pub fn admit(&mut self, job: JobId, request: ConvertRequest) {
        self.jobs.entry(job).or_default().request = Some(request);
    }

    /// The unfinished jobs' ids and outputs (an in-place job's is its source, whose `.part`
    /// it writes beside it; so is an edit job's, whose `output` is ignored).
    pub fn unfinished(&self) -> Vec<(JobId, PathBuf)> {
        self.requests()
            .map(|(id, r)| {
                let in_place = r.lz4_in_place || !r.edits.is_empty();
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
        self.jobs
            .iter()
            .filter_map(|(id, e)| {
                Some(json!({
                    "id": id,
                    "request": e.request.as_ref()?,
                    "stages": e.stages,
                    "progress": e.progress,
                    "done": e.done,
                    "log": e.log,
                    "log_total": e.log_total,
                }))
            })
            .collect()
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
            lz4_profile: None,
            lz4_traces: None,
            lz4_trace_space_mib: 256,
            lz4_in_place: false,
            edits: Vec::new(),
            extract: Vec::new(),
            edit_rebuild: false,
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
        // In place, the `.part` goes beside the source.
        t.admit(
            2,
            ConvertRequest {
                source: "/i/game.ffpkg".into(),
                lz4_in_place: true,
                ..request("")
            },
        );
        assert_eq!(t.unfinished()[1], (2, PathBuf::from("/i/game.ffpkg")));
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
}
