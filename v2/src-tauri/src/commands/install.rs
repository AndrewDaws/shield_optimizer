//! ADB install / status commands — let the UI auto-download Google's
//! platform-tools when no adb is present, matching v1's Check-Adb behavior.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::adb::driver::discover_adb_binary;
use crate::adb::{install_platform_tools, parse_device_list, AdbDriver, SubprocessAdb};
use crate::engine::types::ConnectionType;

use super::AppState;

#[derive(Serialize)]
pub struct AdbStatus {
    /// Is an adb binary currently configured?
    pub available: bool,
    /// Path to the binary if discovery resolves one — useful in the UI for
    /// "Using adb at /opt/homebrew/bin/adb" diagnostics.
    pub path: Option<String>,
    /// What the last `adb devices` call returned, as a quick connectivity probe.
    /// `None` when adb isn't available.
    pub last_probe: Option<String>,
}

/// `adb_status` — fast diagnostic the UI shows when devices fail to list.
#[tauri::command]
pub async fn adb_status(state: State<'_, AppState>) -> Result<AdbStatus, String> {
    let adb = state.adb_snapshot().await;
    let path = discover_adb_binary().map(|p| p.display().to_string());
    let probe = adb.raw(&["devices"]).await;
    match probe {
        Ok(out) => Ok(AdbStatus {
            available: true,
            path,
            last_probe: Some(out.stdout),
        }),
        Err(e) => Ok(AdbStatus {
            available: false,
            path,
            last_probe: Some(e.to_string()),
        }),
    }
}

#[derive(Serialize)]
pub struct InstallResult {
    pub ok: bool,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Serialize)]
pub struct RestartResult {
    pub ok: bool,
    pub message: String,
}

/// `restart_adb` — `adb kill-server` then `adb start-server`. Matches v1's
/// Restart-AdbServer (main-menu shortcut A). Useful when the daemon wedges
/// after the device sleeps, when multiple adb versions collided, or after a
/// USB-cable swap. Does not redownload — for that use `install_adb`.
#[tauri::command]
pub async fn restart_adb(state: State<'_, AppState>) -> Result<RestartResult, String> {
    let adb = state.adb_snapshot().await;

    // kill-server drops every TCP connection. USB devices re-enumerate on
    // their own after the daemon restarts; network devices (ip:port) do not —
    // so capture them first and reconnect afterward. Without this, "Restart
    // ADB" silently disconnects the user's network device and the list comes
    // back empty even though the daemon restarted fine.
    let network_serials: Vec<String> = match adb.raw(&["devices"]).await {
        Ok(out) => parse_device_list(&out.stdout)
            .into_iter()
            .filter(|e| e.connection == ConnectionType::Network)
            .map(|e| e.serial)
            .collect(),
        Err(_) => Vec::new(),
    };

    // kill-server can fail with no daemon running — that's fine, we still
    // care about the start-server result.
    let _ = adb.raw(&["kill-server"]).await;
    let start_output = match adb.raw(&["start-server"]).await {
        Ok(out) => format!("{}{}", out.stdout, out.stderr),
        Err(e) => {
            return Ok(RestartResult {
                ok: false,
                message: e.to_string(),
            })
        }
    };

    // start-server prints "* daemon not running…" on first start; that's
    // success. Real failures contain "error" / "cannot".
    let lower = start_output.to_lowercase();
    let mut ok = !lower.contains("error") && !lower.contains("cannot");

    // Reconnect the network devices we saw before the restart.
    let mut reconnected = Vec::new();
    let mut failed = Vec::new();
    for serial in &network_serials {
        let connected = match adb.raw(&["connect", serial]).await {
            // "connected to X" and "already connected to X" both pass;
            // "failed to connect" / "cannot connect" do not.
            Ok(out) => format!("{}{}", out.stdout, out.stderr)
                .to_lowercase()
                .contains("connected to"),
            Err(_) => false,
        };
        if connected {
            reconnected.push(serial.clone());
        } else {
            failed.push(serial.clone());
        }
    }
    if !failed.is_empty() {
        ok = false;
    }

    let mut message = if start_output.trim().is_empty() {
        "ADB server restarted.".to_string()
    } else {
        start_output.trim().to_string()
    };
    if !reconnected.is_empty() {
        message.push_str(&format!("\nReconnected: {}", reconnected.join(", ")));
    }
    if !failed.is_empty() {
        message.push_str(&format!(
            "\nCould not reconnect: {} — try Scan Network or Connect IP.",
            failed.join(", ")
        ));
    }

    Ok(RestartResult { ok, message })
}

/// `install_adb` — download Google's platform-tools archive, extract into the
/// OS app-data dir, and swap the live driver so the next `list_devices` call
/// uses the freshly-installed binary. Matches v1's auto-download flow.
#[tauri::command]
pub async fn install_adb(state: State<'_, AppState>) -> Result<InstallResult, String> {
    match install_platform_tools().await {
        Ok(path) => {
            let new_driver: Arc<dyn AdbDriver> = Arc::new(SubprocessAdb::new(path.clone()));
            state.replace_adb(new_driver).await;
            Ok(InstallResult {
                ok: true,
                path: Some(path.display().to_string()),
                message: format!("Installed platform-tools to {}", path.display()),
            })
        }
        Err(e) => Ok(InstallResult {
            ok: false,
            path: None,
            message: e.to_string(),
        }),
    }
}
