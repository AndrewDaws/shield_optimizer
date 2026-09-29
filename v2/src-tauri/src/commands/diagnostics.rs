//! Logging to disk, and the bug-report bundle.
//!
//! A packaged app has nowhere for stdout to go, so until now every `tracing`
//! line the app emitted was thrown away on the machines where it mattered.
//! [`init_logging`] adds a rolling daily file under the app data dir and keeps
//! its filter behind a `reload` handle, so "Debug logging" can be switched on
//! from the UI without a restart — which is the only way someone reproducing a
//! bug can capture the run that failed.
//!
//! The bundle itself is formatted by the pure
//! [`shield_optimizer_core::engine::diagnostics`] module; everything here is
//! the I/O that feeds it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, State};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter, Layer};

use shield_optimizer_core::commands::{devices, launcher, AppState};
use shield_optimizer_core::engine::diagnostics::{
    format_diagnostics, DeviceDiagnostics, DiagnosticsInput,
};

/// Subdirectory of the app data dir that holds the rolling log files.
const LOG_DIR_NAME: &str = "logs";
/// One file per day, a week back. Long enough to cover "it broke sometime last
/// week", short enough that nobody's disk notices.
const KEEP_LOG_FILES: usize = 7;
/// Log lines carried in a bug report. Enough to hold a failed device scan and
/// everything around it, not so many that the report stops being readable.
const LOG_TAIL_LINES: usize = 200;

/// Quiet by default, verbose for our own code when debug logging is on.
/// Turning *everything* to `debug` buries the interesting lines under hyper
/// and reqwest internals, which makes the bundle worse, not better.
const INFO_FILTER: &str = "info";
const DEBUG_FILTER: &str =
    "info,shield_optimizer_v2_lib=debug,shield_optimizer_core=debug,shield_optimizer_v2=debug";

/// Managed state for the log file: where it is, whether debug is on, and the
/// switch that changes the running filter.
pub struct LogControl {
    dir: PathBuf,
    debug: AtomicBool,
    /// `None` when the file layer could not be created — the app still runs
    /// and still logs to stdout, the toggle just has nothing to switch.
    apply: Option<Box<dyn Fn(bool) -> bool + Send + Sync>>,
    /// Dropping this stops the background writer flushing, so it has to live
    /// as long as the app does.
    _guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl LogControl {
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn base_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(INFO_FILTER))
}

/// Install the stdout + rolling-file subscriber. Returns the handle the
/// `set_debug_logging` command drives. Best-effort throughout: a read-only or
/// missing data dir costs the log file, never the app.
pub fn init_logging(data_dir: &Path) -> LogControl {
    let dir = data_dir.join(LOG_DIR_NAME);
    let appender = match std::fs::create_dir_all(&dir).ok().and_then(|()| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("shield-optimizer")
            .filename_suffix("log")
            .max_log_files(KEEP_LOG_FILES)
            .build(&dir)
            .ok()
    }) {
        Some(appender) => appender,
        None => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(base_filter())
                .try_init();
            return LogControl {
                dir,
                debug: AtomicBool::new(false),
                apply: None,
                _guard: None,
            };
        }
    };

    let (writer, guard) = tracing_appender::non_blocking(appender);
    let (file_filter, handle) = reload::Layer::new(base_filter());
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(writer)
        .with_filter(file_filter);
    let stdout_layer = fmt::layer().with_filter(base_filter());

    let _ = tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .try_init();

    LogControl {
        dir,
        debug: AtomicBool::new(false),
        apply: Some(Box::new(move |debug: bool| {
            let filter = EnvFilter::new(if debug { DEBUG_FILTER } else { INFO_FILTER });
            handle.reload(filter).is_ok()
        })),
        _guard: Some(guard),
    }
}

/// `set_debug_logging` — raise or lower the *file* layer's level at runtime.
/// Returns what the level actually is afterwards, so a failed reload cannot
/// leave the checkbox claiming something untrue.
#[tauri::command]
pub fn set_debug_logging(logs: State<'_, LogControl>, enabled: bool) -> Result<bool, String> {
    let Some(apply) = logs.apply.as_ref() else {
        return Err(
            "Logging to a file isn't available on this install, so there's nothing to turn up."
                .to_string(),
        );
    };
    if !apply(enabled) {
        return Err("The log filter could not be changed.".to_string());
    }
    logs.debug.store(enabled, Ordering::Relaxed);
    tracing::info!(debug_logging = enabled, "debug logging toggled");
    Ok(enabled)
}

/// `get_debug_logging` — is the file layer currently at debug?
#[tauri::command]
pub fn get_debug_logging(logs: State<'_, LogControl>) -> bool {
    logs.debug.load(Ordering::Relaxed)
}

/// `log_dir_path` — where the rolling log files live, for the UI to show.
#[tauri::command]
pub fn log_dir_path(logs: State<'_, LogControl>) -> String {
    logs.dir().display().to_string()
}

/// `open_log_dir` — reveal the log folder in the system file manager.
#[tauri::command]
pub fn open_log_dir(app: AppHandle, logs: State<'_, LogControl>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = logs.dir().to_path_buf();
    // A folder that was never created cannot be opened; make it rather than
    // handing back an error the user can do nothing about.
    let _ = std::fs::create_dir_all(&dir);
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// The most recently written log file in `dir`.
///
/// Picked by modification time rather than by formatting today's date:
/// `tracing-appender` names files by UTC day, so a machine west of Greenwich
/// would look for a file that does not exist yet for part of every evening.
fn newest_log_file(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let is_newer = match newest.as_ref() {
            Some((best, _)) => modified > *best,
            None => true,
        };
        if is_newer {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

fn log_tail(dir: &Path, lines: usize) -> Vec<String> {
    let Some(path) = newest_log_file(dir) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| (*l).to_string())
        .collect()
}

/// Parse `HOME_HANDLER_QUERY`'s `cmd package query-activities` output down to
/// the packages it reported. Reuses the launcher tab's own parser rather than
/// a second one: Android's ResolveInfo dump exposes the class under `name=`
/// and the package separately under `packageName=` — `name=` has no slash, so
/// treating it as a flattened `pkg/activity` component silently dropped every
/// real handler.
fn home_handler_components(stdout: &str) -> Vec<String> {
    launcher::parse_home_handler_packages(stdout)
}

/// `collect_diagnostics` — the text a user pastes into a bug report.
///
/// Nothing is sent anywhere: this command returns a string and the UI puts it
/// on the clipboard. Deliberately narrow — host facts, one device's identity
/// and TV evidence, its HOME handlers, and the tail of the log. No package
/// inventory.
#[tauri::command]
pub async fn collect_diagnostics(
    state: State<'_, AppState>,
    logs: State<'_, LogControl>,
    serial: Option<String>,
) -> Result<String, String> {
    let adb_path = crate::adb::cached_adb_binary().map(|p| p.display().to_string());
    let adb = state.adb_snapshot().await;
    let adb_version = adb
        .raw(&["version"])
        .await
        .ok()
        .map(|out| out.stdout.trim().to_string());

    // Resolve the device first so a failure there is reported as "no device
    // section" rather than failing the whole bundle — the host half is still
    // worth having when the device is the thing that is broken.
    let mut unreadable: Option<String> = None;
    let device = match serial.as_deref() {
        None => None,
        Some(serial) => match devices::device_profile_impl(state.inner(), serial).await {
            Ok(device) => {
                let handlers = adb
                    .shell(serial, launcher::HOME_HANDLER_QUERY)
                    .await
                    .ok()
                    .map(|out| home_handler_components(&out.stdout))
                    .unwrap_or_default();
                Some((device, handlers))
            }
            Err(e) => {
                tracing::warn!(serial, error = %e, "diagnostics: device unavailable");
                unreadable = Some(e);
                None
            }
        },
    };

    let tail = log_tail(logs.dir(), LOG_TAIL_LINES);

    Ok(format_diagnostics(&DiagnosticsInput {
        app_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        adb_path: adb_path.as_deref(),
        adb_version: adb_version.as_deref(),
        device: device.as_ref().map(|(device, handlers)| DeviceDiagnostics {
            serial: &device.serial,
            connection: device.connection,
            properties: device.properties.as_ref(),
            tv_evidence: device.tv_evidence,
            device_type: device.device_type,
            home_handlers: handlers,
        }),
        unreadable_device: match (serial.as_deref(), unreadable.as_deref()) {
            (Some(serial), Some(error)) => Some((serial, error)),
            _ => None,
        },
        log_tail: &tail,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_handlers_are_read_from_the_real_resolveinfo_shape() {
        // `name=` is the bare class (no slash) and `packageName=` is the
        // separate field that actually names the package — the shape Android
        // emits, not a flattened `pkg/activity` component.
        let stdout = "Activity #0:\n  \
                      Priority=0 PreferredOrder=0 Match=0x108000 Specific=null\n  \
                      ActivityInfo:\n    \
                      name=com.spocky.projengmenu.ui.home.MainActivity\n    \
                      packageName=com.spocky.projengmenu\n    \
                      labelRes=0x7f0e0000\n\n\
                      Activity #1:\n  \
                      ActivityInfo:\n    \
                      name=com.google.android.tvlauncher.MainActivity\n    \
                      packageName=com.google.android.tvlauncher\n";

        assert_eq!(
            home_handler_components(stdout),
            vec![
                "com.spocky.projengmenu".to_string(),
                "com.google.android.tvlauncher".to_string(),
            ]
        );
    }

    #[test]
    fn the_tail_is_the_last_n_lines_and_an_absent_log_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(log_tail(dir.path(), 200).is_empty());

        let body: String = (1..=300).map(|n| format!("line {n}\n")).collect();
        std::fs::write(dir.path().join("shield-optimizer.2026-09-19.log"), body).unwrap();

        let tail = log_tail(dir.path(), 200);
        assert_eq!(tail.len(), 200);
        assert_eq!(tail.first().unwrap(), "line 101");
        assert_eq!(tail.last().unwrap(), "line 300");
    }
}
