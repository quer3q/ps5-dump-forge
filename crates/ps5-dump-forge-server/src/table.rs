//! `GET /api/jobs`: unfinished jobs and the last finished ones. Core can emit `Done` before
//! `Jobs::start` returns, so an entry is made by its first event and the request merged later.

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
    /// Oldest first.
    finished: VecDeque<JobId>,
}

#[derive(Default)]
struct Entry {
    request: Option<ConvertRequest>,
    stages: Vec<String>,
    progress: Option<Value>,
    done: Option<Value>,
    finished_at: Option<Instant>,
    outcome: Option<Result<PathBuf, String>>,
    log: VecDeque<String>,
    log_total: u64,
}

impl Table {
    /// A `Done` of an admitted job gives its end notice, if any.
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

    /// A job whose `Done` came first gives its end notice here, if any.
    // ponytail: an entry already dropped from history would come back here, but that needs 32
    // jobs to finish while `start` runs, and admission (held across it) caps them at 8.
    pub fn admit(&mut self, job: JobId, request: ConvertRequest) -> Option<Ended> {
        let entry = self.jobs.entry(job).or_default();
        let ended = (entry.outcome.as_ref()).and_then(|o| crate::notice::ended(&request, o));
        entry.request = Some(request);
        ended
    }

    /// An edit job's output is its source (a rebuild writes its `.part` beside it).
    pub fn unfinished(&self) -> Vec<(JobId, PathBuf)> {
        self.requests()
            .map(|(id, r)| {
                let in_place = !r.edits.is_empty() || r.recover_metadata.is_some();
                (id, if in_place { &r.source } else { &r.output }.clone())
            })
            .collect()
    }

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

    /// A job shows once its request is known.
    pub fn snapshot(&self) -> Vec<Value> {
        (self.jobs.iter())
            .filter_map(|(id, e)| e.json(*id))
            .collect()
    }

    /// Unfinished or finished less than `keep` ago.
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

/// Core's `part_path`, in the output's resolved folder (as `stale_parts` lists it).
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
            replace_ampr_emu: true,
            edits: Vec::new(),
            extract: Vec::new(),
            transfer: Vec::new(),
            replace: false,
            edit_rebuild: false,
            install: None,
            recover_metadata: None,
            restore_metadata: false,
            delete_source: false,
            pack_profile: None,
            archive_password: None,
        }
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
                    source_bytes: 0,
                    started_at_ms: 0,
                    ended_at_ms: 0,
                })
                .map_err(str::to_string),
        }
    }

    /// The end notice comes once, from whichever of `admit` and `Done` is last.
    #[test]
    fn end_notice_once_whichever_comes_last() {
        use crate::notice::{Ended, JobKind};
        let mut t = Table::default();
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
