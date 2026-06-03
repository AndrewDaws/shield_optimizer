//! Network-scan command — matches v1's `Scan-Network` UX.

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::adb::{local_subnet_prefix, scan_subnet, AdbDriver};

use super::AppState;

#[derive(Serialize)]
pub struct ScanResult {
    /// First three octets of the scanned /24 (e.g. "192.168.42"), or `null`
    /// if the gateway couldn't be detected.
    pub subnet: Option<String>,
    /// IPs that answered on the ADB port.
    pub found: Vec<String>,
    /// IPs that `adb connect` succeeded against.
    pub connected: Vec<String>,
    /// IPs that responded to the port probe but `adb connect` failed.
    pub failed: Vec<String>,
    /// Human-readable summary line — useful diagnostic for the UI.
    pub message: String,
}

/// `adb connect <target>` succeeded? Detection is text-based on the combined
/// streams ("connected to X" / "already connected to X") rather than the exit
/// code, since `adb connect`'s exit-code conventions vary across versions. A
/// nonzero exit surfaces as `Err` from the driver and counts as not-connected.
async fn adb_connect_ok(adb: &dyn AdbDriver, target: &str) -> bool {
    match adb.raw(&["connect", target]).await {
        Ok(out) => {
            let s = format!("{}{}", out.stdout, out.stderr).to_lowercase();
            s.contains("connected to") && !s.contains("failed") && !s.contains("cannot")
        }
        Err(_) => false,
    }
}

/// `scan_network` — sweep the local /24 for ADB-listening devices and try
/// `adb connect` against each responder. Returns a structured summary so the
/// UI can render counts and any per-IP failures.
#[tauri::command]
pub async fn scan_network(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let Some(prefix) = local_subnet_prefix().await else {
        return Ok(ScanResult {
            subnet: None,
            found: vec![],
            connected: vec![],
            failed: vec![],
            message: "Could not detect default gateway. Set SHIELD_OPTIMIZER_SUBNET=\"a.b.c\" \
                      to override, or use Connect IP."
                .to_string(),
        });
    };
    let subnet_label = format!("{}.{}.{}", prefix[0], prefix[1], prefix[2]);

    let hits = scan_subnet(prefix).await;
    let found: Vec<String> = hits.iter().map(|h| h.ip.clone()).collect();

    let adb = state.adb_snapshot().await;

    // Warm the adb daemon before connecting. The port sweep just opened and
    // dropped raw TCP sockets against each device's adbd; firing `adb connect`
    // immediately afterward — especially against a cold daemon — tends to get
    // a transient refusal, which is why a manual "Restart ADB" (which starts
    // the daemon) made the same devices connect. Starting the server here, plus
    // a single retry below, makes the scan connect on its own.
    let _ = adb.raw(&["start-server"]).await;

    let mut connected = Vec::new();
    let mut failed = Vec::new();
    for hit in &hits {
        let target = format!("{}:5555", hit.ip);
        let mut ok = adb_connect_ok(adb.as_ref(), &target).await;
        if !ok {
            tokio::time::sleep(Duration::from_millis(400)).await;
            ok = adb_connect_ok(adb.as_ref(), &target).await;
        }
        if ok {
            connected.push(hit.ip.clone());
        } else {
            failed.push(hit.ip.clone());
        }
    }

    let message = if hits.is_empty() {
        format!(
            "No devices on {subnet_label}.x answered on the ADB port. Make sure Network \
             Debugging is enabled on your TV, or use Connect IP for newer Google TVs that \
             need PIN pairing first."
        )
    } else {
        format!(
            "Scanned {subnet_label}.x — found {} device{}, connected {}.",
            hits.len(),
            if hits.len() == 1 { "" } else { "s" },
            connected.len()
        )
    };

    Ok(ScanResult {
        subnet: Some(subnet_label),
        found,
        connected,
        failed,
        message,
    })
}
