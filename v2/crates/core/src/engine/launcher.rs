//! Custom launcher catalog + plan helpers.
//!
//! The catalog itself is data: `crates/core/data/app-lists/launchers.json`,
//! parsed by the loader and handed to these pure functions as an argument.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One supported launcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherEntry {
    pub name: String,
    pub package: String,
    /// Where to get it when it isn't installed — the launcher's own official
    /// page. `None` for stock launchers, which ship with the device, and for
    /// HOME handlers discovered on the device rather than read from the file.
    #[serde(default)]
    pub source_url: Option<String>,
}

/// The launcher catalog, as loaded from `launchers.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LauncherCatalog {
    /// Installable third-party launchers — listed even when missing, so there
    /// is always a path to install one.
    pub custom: Vec<LauncherEntry>,
    /// Preinstalled launchers. Disabled when a custom launcher takes over, and
    /// listed (when present) so "back to stock" is one click away.
    pub stock: Vec<LauncherEntry>,
    /// Friendly names for known HOME-capable apps that aren't launchers.
    #[serde(default)]
    pub home_handler_names: BTreeMap<String, String>,
    /// HOME handlers that hold Home only in passing, while Android settles on
    /// a new default (Google TV's Setup Wraith, #122). Seeing one while
    /// verifying a switch means "not decided yet", never "the switch failed".
    #[serde(default)]
    pub transient_home_holders: Vec<String>,
}

impl LauncherCatalog {
    /// Friendly name for a known HOME-capable app that isn't a launcher.
    pub fn home_handler_name(&self, pkg: &str) -> Option<&str> {
        self.home_handler_names.get(pkg).map(String::as_str)
    }

    /// True when `pkg` is a preinstalled launcher we know by name.
    pub fn is_stock(&self, pkg: &str) -> bool {
        self.stock.iter().any(|e| e.package == pkg)
    }

    /// True when `pkg` only ever holds Home in passing.
    pub fn is_transient_home_holder(&self, pkg: &str) -> bool {
        self.transient_home_holders.iter().any(|p| p == pkg)
    }

    /// True when `pkg` appears in either catalog list.
    pub fn contains(&self, pkg: &str) -> bool {
        self.is_stock(pkg) || self.custom.iter().any(|e| e.package == pkg)
    }
}

/// One row of the Launchers list: a catalog entry plus its on-device state.
#[derive(Debug, Clone, Serialize)]
pub struct LauncherStatus {
    pub entry: LauncherEntry,
    pub installed: bool,
    pub enabled: bool,
    /// True for the device's preinstalled launcher(s) — rendered with a STOCK
    /// badge and no Install button, since they aren't on the Play Store.
    pub stock: bool,
    /// True for HOME-capable apps outside both catalogs (e.g. Setup Wraith,
    /// a sideloaded HOME app) — rendered with a HOME APP badge.
    pub other: bool,
}

/// Build the Launchers list: stock launchers actually present on the device
/// first (so "back to stock" is always one click away), then the full custom
/// catalog, then any other HOME-capable app. Custom launchers are listed even
/// when missing (they get an Install button); stock ones only when installed —
/// a Shield shouldn't show a "missing" Amazon launcher row.
///
/// `home_handler_pkgs` is the device's enabled HOME handlers (disabled
/// packages don't answer the HOME intent query, which is why callers pass
/// `tracked_disabled_pkgs` — handlers we disabled ourselves and remembered).
/// Safe fallbacks (Settings) are deliberately absent: they must never be
/// disabled, so we don't render them at all.
pub fn launcher_rows(
    catalog: &LauncherCatalog,
    installed_pkgs: &[String],
    disabled_pkgs: &[String],
    home_handler_pkgs: &[String],
    tracked_disabled_pkgs: &[String],
) -> Vec<LauncherStatus> {
    let is_disabled = |pkg: &str| disabled_pkgs.iter().any(|d| d == pkg);
    let stock_catalog = &catalog.stock;
    let custom_catalog = &catalog.custom;

    let stock = stock_catalog
        .iter()
        .filter(|e| installed_pkgs.iter().any(|p| p == &e.package))
        .map(|entry| LauncherStatus {
            enabled: !is_disabled(&entry.package),
            installed: true,
            stock: true,
            other: false,
            entry: entry.clone(),
        });

    let custom = custom_catalog.iter().map(|entry| {
        let installed = installed_pkgs.iter().any(|p| p == &entry.package);
        LauncherStatus {
            installed,
            enabled: installed && !is_disabled(&entry.package),
            stock: false,
            other: false,
            entry: entry.clone(),
        }
    });

    let mut seen_other = std::collections::HashSet::new();
    let other = home_handler_pkgs
        .iter()
        .chain(tracked_disabled_pkgs.iter())
        .filter(|pkg| {
            !catalog.contains(pkg)
                && !safe_home_handlers().contains(&pkg.as_str())
                && seen_other.insert(pkg.to_string())
        })
        .map(|pkg| LauncherStatus {
            entry: LauncherEntry {
                name: catalog.home_handler_name(pkg).unwrap_or(pkg).to_string(),
                package: pkg.clone(),
                source_url: None,
            },
            installed: true,
            enabled: !is_disabled(pkg),
            stock: false,
            other: true,
        });

    stock.chain(custom).chain(other).collect()
}

/// True when disabling `target` would leave the device without a single
/// enabled HOME handler the user can actually land on. Safe fallbacks
/// (Settings) don't count — they're a recovery hatch, not a launcher.
pub fn is_last_enabled_home_handler(target: &str, enabled_handler_pkgs: &[String]) -> bool {
    let target_is_enabled = enabled_handler_pkgs.iter().any(|h| h == target);
    let remaining = enabled_handler_pkgs
        .iter()
        .filter(|h| h.as_str() != target && !safe_home_handlers().contains(&h.as_str()))
        .count();
    target_is_enabled && remaining == 0
}

/// HOME-capable packages that we must NEVER disable — fallback safety net.
pub fn safe_home_handlers() -> &'static [&'static str] {
    &["com.android.tv.settings", "com.android.settings"]
}

/// Validate a user-supplied custom launcher package name (Setup-Launcher's
/// "Custom..." option in v1). Matches the regex v1 uses.
pub fn is_valid_package_name(pkg: &str) -> bool {
    if pkg.is_empty() {
        return false;
    }
    // Must look like `name(.name)+` where each segment starts with a letter.
    let segments: Vec<&str> = pkg.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().all(|seg| {
        !seg.is_empty()
            && seg.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A stand-in for the shipped `launchers.json`. The engine is handed a
    /// catalog rather than owning one, so these tests exercise the row logic
    /// against a fixture; the shipped file's own contents (Monet, Dispatch's
    /// package, the entry count) are asserted in the loader's tests, against
    /// the real JSON.
    fn catalog() -> LauncherCatalog {
        let entry = |name: &str, package: &str, source_url: Option<&str>| LauncherEntry {
            name: name.to_string(),
            package: package.to_string(),
            source_url: source_url.map(str::to_string),
        };
        LauncherCatalog {
            custom: vec![
                entry(
                    "Projectivy Launcher",
                    "com.spocky.projengmenu",
                    Some("https://example.invalid/projectivy"),
                ),
                entry("FLauncher", "me.efesser.flauncher", None),
            ],
            stock: vec![
                entry(
                    "Android TV Launcher (Stock)",
                    "com.google.android.tvlauncher",
                    None,
                ),
                entry("Amazon TV Launcher (Stock)", "com.amazon.tv.launcher", None),
            ],
            home_handler_names: [(
                "com.google.android.tungsten.setupwraith".to_string(),
                "Setup Wraith (HOME)".to_string(),
            )]
            .into_iter()
            .collect(),
            transient_home_holders: vec!["com.google.android.tungsten.setupwraith".to_string()],
        }
    }

    fn pkgs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launcher_rows_put_installed_stock_first() {
        let cat = catalog();
        let rows = launcher_rows(
            &cat,
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &[],
            &[],
            &[],
        );
        assert_eq!(rows[0].entry.package, "com.google.android.tvlauncher");
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(rows[0].enabled);
        // The installed stock launcher + every custom catalog entry.
        assert_eq!(rows.len(), 1 + cat.custom.len());
        assert!(rows[1..].iter().all(|r| !r.stock));
    }

    #[test]
    fn launcher_rows_omit_stock_not_on_device() {
        // A Shield shouldn't get an Amazon (or any "missing stock") row.
        let cat = catalog();
        let rows = launcher_rows(&cat, &pkgs(&["com.spocky.projengmenu"]), &[], &[], &[]);
        assert!(rows.iter().all(|r| !r.stock));
        assert_eq!(rows.len(), cat.custom.len());
    }

    #[test]
    fn launcher_rows_carry_the_catalog_source_url() {
        // The "Get" link on a missing launcher is the only way to install one
        // that isn't on the device's Play Store.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.google.android.tvlauncher"]),
            &[],
            &[],
            &[],
        );
        let projectivy = rows
            .iter()
            .find(|r| r.entry.package == "com.spocky.projengmenu")
            .expect("custom row");
        assert!(!projectivy.installed);
        assert_eq!(
            projectivy.entry.source_url.as_deref(),
            Some("https://example.invalid/projectivy")
        );
    }

    #[test]
    fn launcher_rows_mark_disabled_stock() {
        // The post-cleanup state: stock disabled, custom active. The stock
        // row must still appear — it's the path back.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &pkgs(&["com.google.android.tvlauncher"]),
            &[],
            &[],
        );
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(!rows[0].enabled);
    }

    #[test]
    fn launcher_rows_include_other_home_handlers_but_never_safe_fallbacks() {
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.spocky.projengmenu"]),
            &[],
            &pkgs(&[
                "com.spocky.projengmenu",                  // catalog — already a row
                "com.android.tv.settings",                 // safe fallback — never shown
                "com.google.android.tungsten.setupwraith", // genuinely "other"
            ]),
            &[],
        );
        let others: Vec<_> = rows.iter().filter(|r| r.other).collect();
        assert_eq!(others.len(), 1);
        assert_eq!(
            others[0].entry.package,
            "com.google.android.tungsten.setupwraith"
        );
        assert_eq!(others[0].entry.name, "Setup Wraith (HOME)");
        assert!(others[0].enabled);
        assert!(!rows
            .iter()
            .any(|r| r.entry.package == "com.android.tv.settings"));
    }

    #[test]
    fn launcher_rows_keep_tracked_disabled_handlers_visible() {
        // A disabled handler doesn't answer the HOME query — the tracked list
        // is what keeps its row (and its Enable path) alive.
        let rows = launcher_rows(
            &catalog(),
            &pkgs(&["com.spocky.projengmenu"]),
            &pkgs(&["com.example.sideloaded.home"]),
            &[],
            &pkgs(&["com.example.sideloaded.home"]),
        );
        let row = rows
            .iter()
            .find(|r| r.entry.package == "com.example.sideloaded.home")
            .unwrap();
        assert!(row.other);
        assert!(!row.enabled);
    }

    #[test]
    fn last_enabled_home_handler_guard() {
        let enabled = pkgs(&["com.spocky.projengmenu", "com.android.tv.settings"]);
        // Projectivy is the only real launcher left — Settings doesn't count.
        assert!(is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &enabled
        ));

        let two = pkgs(&["com.spocky.projengmenu", "com.google.android.tvlauncher"]);
        assert!(!is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &two
        ));

        // Target already disabled (absent from the enabled list) — nothing to guard.
        assert!(!is_last_enabled_home_handler(
            "com.example.gone",
            &pkgs(&["com.spocky.projengmenu"])
        ));
    }

    #[test]
    fn package_name_validation_accepts_valid() {
        for valid in &[
            "com.example.launcher",
            "tv.projectivy.launcher",
            "com.google.android.tvlauncher",
            "org.example.app123",
            "com.a.b",
            "com.Example_App.test",
        ] {
            assert!(is_valid_package_name(valid), "should accept {}", valid);
        }
    }

    #[test]
    fn package_name_validation_rejects_invalid() {
        for invalid in &[
            "",
            "   ",
            "com",
            "com.",
            ".com.example",
            "123.example.app",
            "com..example",
            "com.example.",
            "com.123.app",
            "-com.example.app",
            "com.example.app with spaces",
        ] {
            assert!(
                !is_valid_package_name(invalid),
                "should reject {:?}",
                invalid
            );
        }
    }
}
