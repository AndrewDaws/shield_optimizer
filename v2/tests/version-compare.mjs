// The "rolling out" banner in +layout.svelte and the backend's update check
// (src-tauri/src/commands/update.rs::is_newer) must agree on what "newer"
// means, or a staged prerelease rollout can show a stale banner or hide a
// genuinely newer release. Pre-release segments compare numerically, so
// `beta.10` must beat `beta.9` rather than losing a lexical string compare.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = readFileSync(join(v2Root, "src/lib/version.ts"), "utf8");
const module = await import(
  "data:text/javascript," +
    encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }))
);
const { isNewerVersion } = module;

// --- core version ordering --------------------------------------------------
assert.ok(isNewerVersion("0.1.1", "0.1.0"));
assert.ok(isNewerVersion("0.2.0", "0.1.9"));
assert.ok(!isNewerVersion("0.1.0", "0.1.0"));
assert.ok(!isNewerVersion("0.1.0", "0.2.0"));

// --- stable beats prerelease of the same core -------------------------------
assert.ok(isNewerVersion("0.1.0", "0.1.0-beta.9"));
assert.ok(!isNewerVersion("0.1.0-beta.9", "0.1.0"));

// --- the bug this exists to catch: numeric, not lexical, segment compare ---
assert.ok(
  isNewerVersion("2.3.0-beta.10", "2.3.0-beta.9"),
  "beta.10 must be newer than beta.9",
);
assert.ok(!isNewerVersion("2.3.0-beta.9", "2.3.0-beta.10"));
assert.ok(isNewerVersion("0.1.0-rc.1", "0.1.0-beta.9"));

// --- higher core beats any prerelease ---------------------------------------
assert.ok(isNewerVersion("0.2.0-beta.1", "0.1.0"));

console.log(
  "Version compare passed: prerelease segments compare numerically, so beta.10 beats beta.9.",
);
