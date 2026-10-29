//! The server's PS5 notifications, in the console's language (System Settings → Language) when
//! the web UI has it, else English. Off the PS5 they stay English (the CLI's stderr).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ps5_dump_forge_core::{ConvertRequest, Format, Lz4Mode};

/// A UI language (app/src/i18n.ts `LANGUAGES`), by the console's `SceSystemParamLang` code.
// Off the PS5 only English is reached.
#[cfg_attr(not(target_env = "ps5"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lang {
    En,
    Ru,
    Zh,
    De,
    Es,
    Fr,
    Ja,
    Pt,
    It,
    Ko,
    Ar,
}

#[cfg_attr(not(target_env = "ps5"), allow(dead_code))]
impl Lang {
    /// `SCE_SYSTEM_PARAM_LANG_*`: both Englishes, Spanishes, Frenches, Portugueses and Chineses
    /// map to the one the UI has; a language it lacks is English.
    fn from_console(code: i32) -> Self {
        match code {
            0 => Self::Ja,
            2 | 22 => Self::Fr,
            3 | 20 => Self::Es,
            4 => Self::De,
            5 => Self::It,
            7 | 17 => Self::Pt,
            8 => Self::Ru,
            9 => Self::Ko,
            10 | 11 => Self::Zh,
            21 => Self::Ar,
            _ => Self::En,
        }
    }
}

/// The console's language now; English when it can't be read (and off the PS5).
pub(crate) fn console_lang() -> Lang {
    #[cfg(target_env = "ps5")]
    {
        unsafe extern "C" {
            /// libSceSystemService: a system setting by id.
            fn sceSystemServiceParamGetInt(param_id: i32, value: *mut i32) -> i32;
        }
        /// `SCE_SYSTEM_SERVICE_PARAM_ID_LANG`.
        const LANG: i32 = 1;
        let mut code = -1;
        // SAFETY: one i32 out-parameter that outlives the call.
        if unsafe { sceSystemServiceParamGetInt(LANG, &mut code) } == 0 {
            return Lang::from_console(code);
        }
    }
    Lang::En
}

/// Serving on the PS5. Just that: the console's own browser opens the page from the app's icon.
pub(crate) fn started(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Dump Forge is launched",
        Lang::Ru => "Dump Forge запущен",
        Lang::Zh => "Dump Forge 已启动",
        Lang::De => "Dump Forge wurde gestartet",
        Lang::Es => "Dump Forge se ha iniciado",
        Lang::Fr => "Dump Forge est lancé",
        Lang::Ja => "Dump Forge を起動しました",
        Lang::Pt => "O Dump Forge foi iniciado",
        Lang::It => "Dump Forge è stato avviato",
        Lang::Ko => "Dump Forge가 실행되었습니다",
        Lang::Ar => "تم تشغيل Dump Forge",
    }
}

/// Something else holds the port.
pub(crate) fn port_in_use(lang: Lang, port: u16) -> String {
    match lang {
        Lang::En => format!("Dump Forge: port {port} is used by another program"),
        Lang::Ru => format!("Dump Forge: порт {port} занят другой программой"),
        Lang::Zh => format!("Dump Forge：端口 {port} 已被其他程序占用"),
        Lang::De => format!("Dump Forge: Port {port} wird von einem anderen Programm verwendet"),
        Lang::Es => format!("Dump Forge: otro programa está usando el puerto {port}"),
        Lang::Fr => {
            format!("Dump Forge\u{202f}: le port {port} est utilisé par un autre programme")
        }
        Lang::Ja => format!("Dump Forge：ポート {port} は別のプログラムが使用しています"),
        Lang::Pt => format!("Dump Forge: a porta {port} está sendo usada por outro programa"),
        Lang::It => format!("Dump Forge: la porta {port} è usata da un altro programma"),
        Lang::Ko => format!("Dump Forge: 다른 프로그램이 포트 {port}을(를) 사용하고 있습니다"),
        Lang::Ar => format!("Dump Forge: المنفذ {port} يستخدمه برنامج آخر"),
    }
}

/// The server has stopped (Stop in the page); the web UI's `app.stopped` words, except English
/// keeps its own (tests match it).
pub(crate) fn stopped(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Dump Forge stopped",
        Lang::Ru => "Dump Forge остановлен",
        Lang::Zh => "Dump Forge 已停止",
        Lang::De => "Dump Forge wurde beendet",
        Lang::Es => "Dump Forge se ha detenido",
        Lang::Fr => "Dump Forge est arrêté",
        Lang::Ja => "Dump Forge は停止しました",
        Lang::Pt => "O Dump Forge foi parado",
        Lang::It => "Dump Forge è stato arrestato",
        Lang::Ko => "Dump Forge가 중지되었습니다",
        Lang::Ar => "توقّف Dump Forge",
    }
}

/// A newer copy was started while an older one has a job: the older one
/// keeps serving, and the newer one's self copy is what the tile starts next time.
pub(crate) fn update_busy(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Can't close Dump Forge - some jobs are still running.",
        Lang::Ru => "Не удаётся закрыть Dump Forge - некоторые задачи ещё выполняются.",
        Lang::Zh => "无法关闭 Dump Forge - 仍有任务正在运行。",
        Lang::De => "Dump Forge kann nicht geschlossen werden - einige Aufträge laufen noch.",
        Lang::Es => "No se puede cerrar Dump Forge - algunas tareas siguen en curso.",
        Lang::Fr => "Impossible de fermer Dump Forge - des tâches sont encore en cours.",
        Lang::Ja => "Dump Forge を終了できません - 実行中のジョブがあります。",
        Lang::Pt => {
            "Não é possível fechar o Dump Forge - algumas tarefas ainda estão em andamento."
        }
        Lang::It => "Impossibile chiudere Dump Forge - alcuni processi sono ancora in corso.",
        Lang::Ko => "Dump Forge를 닫을 수 없습니다 - 아직 실행 중인 작업이 있습니다.",
        Lang::Ar => "تعذّر إغلاق Dump Forge - لا تزال بعض المهام قيد التشغيل.",
    }
}

/// The older copy agreed to quit but its port didn't come free in time (or it didn't answer).
pub(crate) fn not_taken_over(lang: Lang, old: &str, new: &str) -> String {
    match lang {
        Lang::En => format!("Dump Forge v{new} could not take over from v{old}"),
        Lang::Ru => format!("Dump Forge v{new} не смог сменить v{old}"),
        Lang::Zh => format!("Dump Forge v{new} 无法接替 v{old}"),
        Lang::De => format!("Dump Forge v{new} konnte v{old} nicht ablösen"),
        Lang::Es => format!("Dump Forge v{new} no pudo reemplazar a v{old}"),
        Lang::Fr => format!("Dump Forge v{new} n’a pas pu prendre le relais de v{old}"),
        Lang::Ja => format!("Dump Forge v{new} は v{old} から引き継げませんでした"),
        Lang::Pt => format!("O Dump Forge v{new} não conseguiu substituir a v{old}"),
        Lang::It => format!("Dump Forge v{new} non è riuscito a sostituire la v{old}"),
        Lang::Ko => format!("Dump Forge v{new}이(가) v{old}을(를) 대신하지 못했습니다"),
        Lang::Ar => format!("تعذّر على Dump Forge v{new} أن يحلّ محلّ v{old}"),
    }
}

/// The kind of long job a notice is sent for: the UI's `jobType` (app/src/jobs.ts) but edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    Convert,
    Lz4,
    /// A zstd pack or unpack job (`zstd_pack`, `zstd_unpack`).
    Zstd,
    Transfer,
    /// A package install (`install`).
    Install,
}

/// A job of this device that finished or failed, as the console is told ([`job_ended`]).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Ended {
    pub kind: JobKind,
    pub ok: bool,
    pub name: String,
}

/// What a notice says about `request`'s job, given its result (`Ok`: the report's output):
/// a conversion, an LZ4 tab job, a zstd pack or a transfer (named by what it sent) that finished or failed. Nothing for an edit,
/// an extract or a cancel.
pub(crate) fn ended(request: &ConvertRequest, result: &Result<PathBuf, String>) -> Option<Ended> {
    if let Err(e) = result
        && (e == "cancelled" || e.starts_with("cancelled; "))
    {
        return None;
    }
    let lz4 = match request.lz4 {
        Some(Lz4Mode::Pack | Lz4Mode::Unpack) => true,
        None => request.format == Format::Lz4,
    };
    let zstd = request.zstd_pack || request.zstd_unpack;
    let kind = if request.install.is_some() {
        JobKind::Install
    } else if !request.transfer.is_empty() {
        JobKind::Transfer
    } else if !request.edits.is_empty() || !request.extract.is_empty() {
        return None;
    } else if lz4 {
        JobKind::Lz4
    } else if zstd {
        JobKind::Zstd
    } else {
        JobKind::Convert
    };
    let name = if let [first, rest @ ..] = request.transfer.as_slice() {
        // What was sent, not where: "game-1k", or "game-1k +2" for three items.
        match rest.len() {
            0 => file_name(first),
            n => format!("{} +{n}", file_name(first)),
        }
    } else if let Some(pkg) = &request.install {
        file_name(pkg)
    } else {
        file_name(match result {
            Ok(output) => output,
            Err(_) => &request.output,
        })
    };
    Some(Ended {
        kind,
        ok: result.is_ok(),
        name,
    })
}

fn file_name(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.display().to_string(),
    }
}

/// A conversion, LZ4 job, zstd job or transfer finished or failed: "Dump Forge: conversion
/// finished: <name>" and its seven siblings (the plain notice, when the system toast can't be sent).
pub(crate) fn job_ended(lang: Lang, e: &Ended) -> String {
    let (what, colon, name) = job_parts(lang, e);
    format!("Dump Forge{colon}{what}{colon}{name}")
}

/// The system toast's headline: "Conversion finished" and its seven siblings.
fn job_headline(lang: Lang, e: &Ended) -> String {
    let what = job_parts(lang, e).0;
    let mut chars = what.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// What happened, the language's colon and the name (kept whole in right-to-left text).
fn job_parts(lang: Lang, e: &Ended) -> (&'static str, &'static str, String) {
    // Finished, failed: a conversion, an LZ4 job, a zstd job, a transfer, a package install.
    let words: [&str; 10] = match lang {
        Lang::En => [
            "conversion finished",
            "conversion failed",
            "LZ4 job finished",
            "LZ4 job failed",
            "zstd job finished",
            "zstd job failed",
            "transfer finished",
            "transfer failed",
            "install finished",
            "install failed",
        ],
        Lang::Ru => [
            "конвертация завершена",
            "конвертация не удалась",
            "задача LZ4 завершена",
            "задача LZ4 не удалась",
            "задача zstd завершена",
            "задача zstd не удалась",
            "передача завершена",
            "передача не удалась",
            "установка завершена",
            "установка не удалась",
        ],
        Lang::Zh => [
            "转换完成",
            "转换失败",
            "LZ4 任务完成",
            "LZ4 任务失败",
            "zstd 任务完成",
            "zstd 任务失败",
            "传输完成",
            "传输失败",
            "安装完成",
            "安装失败",
        ],
        Lang::De => [
            "Konvertierung abgeschlossen",
            "Konvertierung fehlgeschlagen",
            "LZ4-Auftrag abgeschlossen",
            "LZ4-Auftrag fehlgeschlagen",
            "zstd-Auftrag abgeschlossen",
            "zstd-Auftrag fehlgeschlagen",
            "Übertragung abgeschlossen",
            "Übertragung fehlgeschlagen",
            "Installation abgeschlossen",
            "Installation fehlgeschlagen",
        ],
        Lang::Es => [
            "conversión terminada",
            "conversión fallida",
            "tarea LZ4 terminada",
            "tarea LZ4 fallida",
            "tarea zstd terminada",
            "tarea zstd fallida",
            "transferencia terminada",
            "transferencia fallida",
            "instalación terminada",
            "instalación fallida",
        ],
        Lang::Fr => [
            "conversion terminée",
            "échec de la conversion",
            "tâche LZ4 terminée",
            "échec de la tâche LZ4",
            "tâche zstd terminée",
            "échec de la tâche zstd",
            "transfert terminé",
            "échec du transfert",
            "installation terminée",
            "échec de l’installation",
        ],
        Lang::Ja => [
            "変換が完了しました",
            "変換に失敗しました",
            "LZ4 ジョブが完了しました",
            "LZ4 ジョブに失敗しました",
            "zstd ジョブが完了しました",
            "zstd ジョブに失敗しました",
            "転送が完了しました",
            "転送に失敗しました",
            "インストール完了",
            "インストール失敗",
        ],
        Lang::Pt => [
            "conversão concluída",
            "falha na conversão",
            "tarefa LZ4 concluída",
            "falha na tarefa LZ4",
            "tarefa zstd concluída",
            "falha na tarefa zstd",
            "transferência concluída",
            "falha na transferência",
            "instalação concluída",
            "falha na instalação",
        ],
        Lang::It => [
            "conversione completata",
            "conversione non riuscita",
            "processo LZ4 completato",
            "processo LZ4 non riuscito",
            "processo zstd completato",
            "processo zstd non riuscito",
            "trasferimento completato",
            "trasferimento non riuscito",
            "installazione completata",
            "installazione non riuscita",
        ],
        Lang::Ko => [
            "변환 완료",
            "변환 실패",
            "LZ4 작업 완료",
            "LZ4 작업 실패",
            "zstd 작업 완료",
            "zstd 작업 실패",
            "전송 완료",
            "전송 실패",
            "설치 완료",
            "설치 실패",
        ],
        Lang::Ar => [
            "اكتمل التحويل",
            "فشل التحويل",
            "اكتملت مهمة LZ4",
            "فشلت مهمة LZ4",
            "اكتملت مهمة zstd",
            "فشلت مهمة zstd",
            "اكتمل النقل",
            "فشل النقل",
            "اكتمل التثبيت",
            "فشل التثبيت",
        ],
    };
    let kind = match e.kind {
        JobKind::Convert => 0,
        JobKind::Lz4 => 2,
        JobKind::Zstd => 4,
        JobKind::Transfer => 6,
        JobKind::Install => 8,
    };
    let what = words[kind + usize::from(!e.ok)];
    let colon = match lang {
        Lang::Zh | Lang::Ja => "：",
        Lang::Fr => "\u{202f}: ",
        _ => ": ",
    };
    let name = match lang {
        // A Latin name kept whole in right-to-left text (LRI…PDI), as the web UI does.
        Lang::Ar => format!("\u{2066}{}\u{2069}", e.name),
        _ => e.name.clone(),
    };
    (what, colon, name)
}

/// The marker file whose presence turns job notices off.
const NOTICES_OFF: &str = "notifications.off";

/// Shows the user a line (`Options::notify`).
pub(crate) type Notify = Arc<dyn Fn(&str) + Send + Sync>;

/// Job-end notices ([`job_ended`]): sent on the PS5 while the setting is on (the default).
/// The setting is kept as [`NOTICES_OFF`] in the settings folder, else in memory only.
pub(crate) struct JobNotices {
    /// Held across a change of the marker file too, so two changes can't cross.
    on: Mutex<bool>,
    dir: Option<PathBuf>,
    /// `None` off the PS5: nothing is sent.
    send: Option<Notify>,
    /// The system toast ([`system_toast`]; tests pass their own), tried before `send`.
    toast: fn(&str) -> bool,
    /// Where the toast's icon is put ([`TOAST_ICON`]); `None` off the PS5: no toast.
    icon: Option<PathBuf>,
}

impl JobNotices {
    pub fn new(dir: Option<PathBuf>, send: Option<Notify>) -> Self {
        let off = dir.as_ref().is_some_and(|d| d.join(NOTICES_OFF).exists());
        Self {
            on: Mutex::new(!off),
            dir,
            send,
            toast: system_toast,
            icon: cfg!(target_env = "ps5").then(|| PathBuf::from(TOAST_ICON)),
        }
    }

    pub fn on(&self) -> bool {
        *self.on.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Turns them on or off, kept in the settings folder first (an error leaves it as it was).
    pub fn set(&self, on: bool) -> Result<(), String> {
        let mut now = self.on.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(dir) = &self.dir {
            let marker = dir.join(NOTICES_OFF);
            let kept = if on {
                match std::fs::remove_file(&marker) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
            } else {
                std::fs::create_dir_all(dir).and_then(|()| std::fs::File::create(&marker).map(drop))
            };
            kept.map_err(|e| format!("{}: {e}", marker.display()))?;
        }
        *now = on;
        Ok(())
    }

    /// Tells the console that a job ended, when it is one a notice is for and they are on: a
    /// system toast with Forge's icon, else (refused, or no icon) the plain notice.
    pub fn send(&self, ended: Option<Ended>) {
        let (Some(send), Some(ended)) = (&self.send, ended) else {
            return;
        };
        if !self.on() {
            return;
        }
        let lang = console_lang();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let toasted = self.icon.as_ref().is_some_and(|icon| {
            toast_icon(icon)
                && (self.toast)(&toast_json(
                    &job_headline(lang, &ended),
                    &job_parts(lang, &ended).2,
                    icon,
                    now,
                ))
        });
        if !toasted {
            send(&job_ended(lang, &ended));
        }
    }
}

/// Forge's icon for the system toast, where the console's notification overlay reads it
/// (ShadowMountPlus keeps its own under `/user/data` too).
const TOAST_ICON: &str = "/user/data/dumpforge/notify-icon.png";

/// Forge's icon: the tile's `icon0.png` and the toast's.
pub(crate) const ICON: &[u8] = include_bytes!("../../../ps5/icon0.png");

/// Sends a system toast from its JSON payload (libSceNotification, as the console's own
/// notices); false when the console refuses it.
#[cfg(target_env = "ps5")]
pub(crate) fn system_toast(payload: &str) -> bool {
    unsafe extern "C" {
        /// libSceNotification: a notification from its JSON payload.
        fn sceNotificationSend(
            user_id: i32,
            logged_in: bool,
            payload: *const std::ffi::c_char,
        ) -> i32;
    }
    /// 254: every logged-in user (ShadowMountPlus's `SCE_NOTIFICATION_LOCAL_USER_ID_SYSTEM`).
    const SYSTEM_USER: i32 = 0xFE;
    let Ok(payload) = std::ffi::CString::new(payload) else {
        return false;
    };
    // SAFETY: a NUL-terminated string that outlives the call.
    unsafe { sceNotificationSend(SYSTEM_USER, true, payload.as_ptr()) == 0 }
}

/// Off the PS5 there is no system toast.
#[cfg(not(target_env = "ps5"))]
pub(crate) fn system_toast(_: &str) -> bool {
    false
}

/// Puts Forge's icon (the tile's) at `icon` unless those bytes are there already; false if it
/// can't.
fn toast_icon(icon: &Path) -> bool {
    use std::sync::atomic::{AtomicU32, Ordering};
    // Two jobs ending at once write their own temporary files.
    static TMP: AtomicU32 = AtomicU32::new(0);
    if std::fs::read(icon).is_ok_and(|have| have == ICON) {
        return true;
    }
    let n = TMP.fetch_add(1, Ordering::Relaxed);
    let tmp = icon.with_extension(format!("png.{}-{n}.tmp", std::process::id()));
    let put = icon
        .parent()
        .is_some_and(|d| std::fs::create_dir_all(d).is_ok())
        && std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut f| std::io::Write::write_all(&mut f, ICON))
            .is_ok()
        && std::fs::rename(&tmp, icon).is_ok();
    if !put {
        let _ = std::fs::remove_file(&tmp);
    }
    put
}

/// A job-end toast: Forge's icon, the name as the upper line (`subMessage`), the headline as the
/// main one (`message`). The shape ShadowMountPlus
/// sends (its TOASTS.md): `InteractiveToastTemplateB`, here with no action.
fn toast_json(headline: &str, name: &str, icon: &Path, now: u64) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    // Two jobs ending within a second get their own ids.
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let id = format!("{now}{:03}", SEQ.fetch_add(1, Ordering::Relaxed) % 1000);
    serde_json::json!({
        "rawData": {
            "viewTemplateType": "InteractiveToastTemplateB",
            "channelType": "ServiceFeedback",
            "bundleName": "DumpForgeJobs",
            "useCaseId": "IDC",
            "soundEffect": "none",
            "toastOverwriteType": "No",
            "isImmediate": true,
            "priority": 100,
            "viewData": {
                "icon": { "type": "Url", "parameters": { "url": icon } },
                "message": { "body": headline },
                "subMessage": { "body": name },
            },
            "platformViews": {
                "previewDisabled": {
                    "viewData": {
                        "icon": { "type": "Predefined", "parameters": { "icon": "community" } },
                        "message": { "body": headline },
                    }
                }
            },
        },
        "createdDateTime": utc_time(now),
        "localNotificationId": id,
    })
    .to_string()
}

/// A Unix time as `2026-10-27T12:00:00.000Z`.
fn utc_time(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.000Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(output: &str) -> ConvertRequest {
        serde_json::from_value(serde_json::json!({
            "source": "/s/game",
            "format": "exfat",
            "output": output,
        }))
        .unwrap()
    }

    fn kind(request: &ConvertRequest) -> Option<JobKind> {
        ended(request, &Ok(PathBuf::from("/o/x"))).map(|e| e.kind)
    }

    /// As the UI's `jobType`: a transfer, then edits and extracts (no notice), then an LZ4
    /// tab job, then a zstd pack, else a conversion.
    #[test]
    fn which_jobs_get_a_notice() {
        let plain = request("/o/a.exfat");
        assert_eq!(kind(&plain), Some(JobKind::Convert));
        let with = |f: &dyn Fn(&mut ConvertRequest)| {
            let mut r = request("/o/a.exfat");
            f(&mut r);
            kind(&r)
        };
        assert_eq!(with(&|r| r.format = Format::Lz4), Some(JobKind::Lz4));
        for mode in [Lz4Mode::Pack, Lz4Mode::Unpack] {
            assert_eq!(with(&|r| r.lz4 = Some(mode)), Some(JobKind::Lz4));
        }
        let zstd = |r: &mut ConvertRequest| {
            r.format = Format::Folder;
            r.zstd_pack = true;
        };
        assert_eq!(with(&zstd), Some(JobKind::Zstd));
        assert_eq!(
            with(&|r| r.transfer = vec!["/s/a".into()]),
            Some(JobKind::Transfer)
        );
        assert_eq!(
            with(&|r| r.install = Some("/data/a.pkg".into())),
            Some(JobKind::Install)
        );
        assert_eq!(with(&|r| r.extract = vec!["a".into()]), None);
        let edit = |r: &mut ConvertRequest| {
            r.edits = vec![ps5_dump_forge_core::Edit::Delete { path: "a".into() }];
        };
        assert_eq!(with(&edit), None);
        // A cancel says nothing; a failure names the output.
        for cancel in ["cancelled", "cancelled; the image was restored"] {
            assert_eq!(ended(&plain, &Err(cancel.into())), None);
        }
        let failed = ended(&plain, &Err("no room".into())).unwrap();
        assert_eq!((failed.ok, failed.name.as_str()), (false, "a.exfat"));
        // A transfer names what it sent: the first item, "+N" for the rest.
        let mut t = request("/data/games");
        t.transfer = vec!["/s/game-1k".into(), "/s/b".into(), "/s/c".into()];
        assert_eq!(
            ended(&t, &Ok("/data/games".into())).unwrap().name,
            "game-1k +2"
        );
        t.transfer.truncate(1);
        assert_eq!(ended(&t, &Err("x".into())).unwrap().name, "game-1k");
    }

    #[test]
    fn texts() {
        let e = |kind, ok| Ended {
            kind,
            ok,
            name: "Game-PPSA01234.ffpkg".into(),
        };
        assert_eq!(
            job_ended(Lang::En, &e(JobKind::Convert, true)),
            "Dump Forge: conversion finished: Game-PPSA01234.ffpkg"
        );
        assert_eq!(
            job_ended(Lang::En, &e(JobKind::Lz4, false)),
            "Dump Forge: LZ4 job failed: Game-PPSA01234.ffpkg"
        );
        assert_eq!(
            job_ended(Lang::Ru, &e(JobKind::Transfer, true)),
            "Dump Forge: передача завершена: Game-PPSA01234.ffpkg"
        );
        assert_eq!(
            job_ended(Lang::Ja, &e(JobKind::Convert, false)),
            "Dump Forge：変換に失敗しました：Game-PPSA01234.ffpkg"
        );
        assert_eq!(
            job_ended(Lang::En, &e(JobKind::Zstd, true)),
            "Dump Forge: zstd job finished: Game-PPSA01234.ffpkg"
        );
        assert_eq!(
            job_ended(Lang::De, &e(JobKind::Zstd, false)),
            "Dump Forge: zstd-Auftrag fehlgeschlagen: Game-PPSA01234.ffpkg"
        );
        assert!(job_ended(Lang::Ar, &e(JobKind::Convert, true)).ends_with("\u{2069}"));
    }

    /// Sent only with a sender (the PS5) and while on.
    #[test]
    fn sent_while_on() {
        let sent = Arc::new(Mutex::new(Vec::<String>::new()));
        let to = sent.clone();
        let send: Notify = Arc::new(move |line: &str| to.lock().unwrap().push(line.into()));
        let notices = JobNotices::new(None, Some(send));
        let one = || ended(&request("/o/a.exfat"), &Ok("/o/a.exfat".into()));
        notices.send(one());
        notices.send(None);
        notices.set(false).unwrap();
        notices.send(one());
        assert_eq!(
            *sent.lock().unwrap(),
            ["Dump Forge: conversion finished: a.exfat"]
        );
        // Off the PS5: no sender, nothing sent.
        JobNotices::new(None, None).send(one());
    }

    /// The system toast first, Forge's icon put beside it; the plain notice when the toast is
    /// refused.
    #[test]
    fn a_system_toast_first_else_the_plain_notice() {
        let dir = std::env::temp_dir().join(format!("forge-toast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sent = Arc::new(Mutex::new(Vec::<String>::new()));
        let to = sent.clone();
        let send: Notify = Arc::new(move |line: &str| to.lock().unwrap().push(line.into()));
        let one = || ended(&request("/o/a.exfat"), &Ok("/o/a.exfat".into()));
        let accepted = JobNotices {
            toast: |payload| payload.contains("\"Conversion finished\""),
            icon: Some(dir.join("icons/notify-icon.png")),
            ..JobNotices::new(None, Some(send.clone()))
        };
        accepted.send(one());
        assert!(sent.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read(dir.join("icons/notify-icon.png")).unwrap(),
            ICON
        );
        let refused = JobNotices {
            toast: |_| false,
            ..accepted
        };
        refused.send(one());
        assert_eq!(
            *sent.lock().unwrap(),
            ["Dump Forge: conversion finished: a.exfat"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The fields the console's overlay requires, the text, the icon and the time.
    #[test]
    fn toast_payload() {
        let e = Ended {
            kind: JobKind::Transfer,
            ok: false,
            name: "Game \"1\"".into(),
        };
        let json = toast_json(
            &job_headline(Lang::En, &e),
            &e.name,
            Path::new("/user/data/dumpforge/notify-icon.png"),
            1_793_094_260,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let raw = &v["rawData"];
        for key in ["viewTemplateType", "useCaseId", "channelType", "viewData"] {
            assert!(!raw[key].is_null(), "{key}");
        }
        assert_eq!(raw["viewData"]["message"]["body"], "Transfer failed");
        assert_eq!(raw["viewData"]["subMessage"]["body"], "Game \"1\"");
        assert_eq!(
            raw["viewData"]["icon"]["parameters"]["url"],
            "/user/data/dumpforge/notify-icon.png"
        );
        assert_eq!(v["createdDateTime"], "2026-10-27T09:44:20.000Z");
        assert!(
            v["localNotificationId"]
                .as_str()
                .unwrap()
                .starts_with("1793094260")
        );
        assert_eq!(utc_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(utc_time(951_782_400), "2000-02-29T00:00:00.000Z");
        assert_eq!(job_headline(Lang::Ru, &e), "Передача не удалась");
    }
}
