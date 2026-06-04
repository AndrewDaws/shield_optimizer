# aTV Tools parity plan

Comparison of Shield Optimizer v2 against **aTV Tools** (`dev.vodik7.atvtools` by vodik7) — the most popular ADB toolbox for Android TV — and a prioritized plan for closing the gaps that fit our product.

Research notes: aTV Tools is a **phone/tablet companion app** (Android 8.1+/iOS 14+) controlling the TV over network ADB; freemium (~$2 lifetime Pro unlocks app management, remote, recording, gamepad). We're a free, open-source **desktop** app over the same transport — so most of its features are reachable for us, and some of its phone-centric ones (gamepad, mirroring) don't make sense here.

## Feature comparison

| Capability | aTV Tools | Shield Optimizer v2 |
|---|---|---|
| Curated debloat catalog + safety list | ❌ | ✅ (core strength) |
| Optimize/Restore wizard with per-app defaults | ❌ | ✅ |
| Snapshots (state backup → restore/clone across devices) | ❌ | ✅ |
| Launcher wizard (install/set/disable stock) | ❌ | ✅ |
| Dedicated tweaks UI (CEC, frame-rate, animations, scaling) | ❌ (raw shell only) | ✅ |
| Network scan / auto-discovery | partial (manual IP) | ✅ |
| Disable/enable/uninstall apps | ✅ (Pro) | ✅ |
| Sideload APK | ✅ | ✅ |
| **APK backup/extract (device → local file)** | ✅ | ✅ (PR #40) |
| **Install extracted APK to another device (app cloning)** | manual two-step | ✅ one-click (PR #40) |
| **File manager (browse / push / pull)** | ✅ | ❌ |
| **Screenshots of the TV** | ✅ | ✅ (PR #39) |
| Screen recording | ✅ (Pro, no DRM content) | ❌ |
| Remote control / D-pad / mouse | ✅ (Pro) | ❌ |
| **Send text to TV (type from keyboard)** | ✅ | ✅ (PR #41) |
| Permissions grant/revoke | ✅ | ❌ |
| Bulk cache clear | ✅ | ✅ (PR #41) |
| Running apps + force-stop | ✅ | ✅ (PR #39) |
| Resource monitor | CPU/RAM/net/storage | RAM/temp/storage/display (no CPU/net) |
| Shell runner with bookmarks | ✅ | ❌ |
| Screen mirroring / gamepad / media remote | ✅ (phone-centric) | — (out of scope for desktop) |
| Open source / free | ❌ | ✅ |

**Bottom line:** we beat aTV Tools on the *debloat/optimize/safety* core, they beat us on *general device utilities*. The gaps worth closing are the utilities that complement debloating; the phone-centric features aren't our product.

## Prioritized roadmap

> **Status (2026-06-04):** P1.1 + P1.2 shipped in PR #40, P1.4 in PR #39,
> P2.5 in PR #39, P2.6 + P2.8 in PR #41. Next up: **P1.3 file manager**
> (Bryan re-confirmed: browse + download + upload, the biggest remaining
> piece) and the **remote control panel** below (promoted from P3 on
> request).

### P1 — App backup / cloning + file management (the asked-for set)

**1. APK backup ("Save APK to computer")**
Shape: for an installed app, resolve its APK path(s) with `pm path <pkg>` (may return multiple lines for split APKs), `adb pull` each to a user-chosen folder, name them `<pkg>-<version>.apk`. UI: a "Backup APK" action on App List rows + the memory table.
- Driver needs a `pull(serial, remote, local)` capability on `AdbDriver` (new trait method wrapping `adb -s X pull`).
- Split APKs (`pm path` returning base.apk + split_*.apk) must install together via `adb install-multiple` — detect and warn.

**2. App cloning ("Install on another device")**
Shape: combine #1 with the existing `install_apk`: pick app on device A → pull APK(s) to a temp dir → pick target device B → `adb -s B install(-multiple)`. One wizard: "Copy app to another device…" listing other connected devices.
- Caveat to surface in UI: app *data* doesn't come along (no root); DRM/licensed apps may refuse; paid apps should be reinstalled via Play instead.

**3. File manager (browse / push / pull)**
Shape: a new device tab "Files". Backend: `list_dir(serial, path)` via `ls -lA` parsing (or `toybox ls -llA`), `pull_file`, `push_file`, `delete_file` (confirm + path-restricted to `/sdcard` by default). UI: single-pane browser of `/sdcard` with breadcrumbs, Upload / Download / Delete buttons.
- Keep scope to `/sdcard` (user storage) initially — no system paths, avoids foot-guns.

**4. Screenshots**
Shape: `adb -s X exec-out screencap -p > local.png`, save to a user folder, show a preview + "Save / Copy". One button on the device header or Health tab. (Driver needs an exec-out/binary-capture capability.)

### P2 — Power-user utilities

**5. Force-stop** on memory-table rows (`am force-stop <pkg>`) — trivial, pairs with the existing Disable button.
**6. Send text to TV** — `input text '<escaped>'` for typing Wi-Fi passwords/searches from the desktop keyboard. Small input box on the device header. (Escape carefully; relates to the package-validation work.)
**7. Shell runner with bookmarks** — an "Advanced" tab: command input → runs via the driver, shows combined output; bookmark list persisted locally. The catch-all that made aTV Tools sticky.
**8. Bulk cache clear** — `pm trim-caches 999999999999` (one call, no per-app loop).
**9. CPU + network monitor** — add `top -n1` / `/proc/stat` parse and `/proc/net/dev` deltas to the Health report.

**9b. Remote control panel** *(promoted from P3 — requested 2026-06-04)* —
a compact remote area on the device page that goes beyond the batch
send-text shipped in PR #41:
- **Live typing**: a focused capture field that relays keystrokes as you
  type — `input text` per chunk, `input keyevent KEYCODE_DEL` for
  backspace, `KEYCODE_ENTER` for submit. Each keystroke is an adb
  round-trip (~100–300 ms on network ADB), i.e. typing-over-SSH feel —
  fine for passwords/searches.
- **D-pad + nav buttons**: `input keyevent` for up/down/left/right (19–22),
  select (23), back (4), home (3), play/pause (85), volume (24/25).

### P3 — Evaluate later
**10. Screen recording** — `screenrecord` (3-min cap, no DRM), pull + save. Nice demo material.
**11. Permissions viewer/grant/revoke** — `dumpsys package <pkg>` parse + `pm grant/revoke`. Niche; gate behind Advanced.
**Skip:** screen mirroring, gamepad, media remote — phone-form-factor features.

## Related design items (from beta feedback)

**A. "Optional" apps not in the catalog.** Many preinstalled apps aren't in our curated list but are safe-if-unused (user request). Proposal: an "Everything else" section at the bottom of the App List — third-party packages (`pm list packages -3`) not in the catalog, badged `NOT CURATED`, default action **Keep**, with Disable/Uninstall available behind the standard safety gate + a one-line "Optional — remove if you don't use it" description. Keeps the curated list authoritative while making the long tail actionable.

**B. Snapshot ↔ Optimize convergence.** The snapshot preview is now a table (beta.7). Possible next step: render the preview as an *editable* optimize-style plan (per-row dropdowns: apply/skip each disable) so apply-snapshot and optimize share one mental model — and potentially one component.

## Suggested sequencing

1. ~~P1.1 + P1.2 (APK backup → cloning)~~ — shipped (PR #40)
2. ~~P1.4 screenshots and P2.5 force-stop~~ — shipped (PR #39)
3. **P1.3 file manager (largest single piece) — next**
4. **9b remote control panel** — next after (or alongside) the file manager
5. A (optional-apps section) — data-model + UI, pairs naturally with the App List
6. P2.7 (shell runner) + P2.9 (CPU/net monitor) as filler between releases
   (~~P2.6 + P2.8~~ shipped in PR #41)
