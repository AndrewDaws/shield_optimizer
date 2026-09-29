// Version comparison for the update-availability banner. The same rule as
// `is_newer` in `src-tauri/src/commands/update.rs`, kept here as a tiny
// exported helper so it can be unit-tested on its own — the comparison
// backing the "rolling out" banner has to agree with the backend's, not just
// look plausible.

interface ParsedVersion {
  core: [number, number, number];
  pre: string | null;
}

function parseVersion(v: string): ParsedVersion {
  const dash = v.indexOf("-");
  const core = dash === -1 ? v : v.slice(0, dash);
  const parts = core.split(".").map((n) => Number.parseInt(n, 10) || 0);
  return {
    core: [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0],
    pre: dash === -1 ? null : v.slice(dash + 1),
  };
}

/// Compare pre-release strings like `beta.9` vs `beta.10` — split on `.`,
/// compare numeric segments numerically and text segments lexically, so a
/// higher build number always wins regardless of digit count.
function comparePrerelease(a: string, b: string): number {
  const ai = a.split(".");
  const bi = b.split(".");
  const len = Math.max(ai.length, bi.length);
  for (let i = 0; i < len; i++) {
    const x = ai[i];
    const y = bi[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const nx = Number.parseInt(x, 10);
    const ny = Number.parseInt(y, 10);
    const bothNumeric =
      Number.isFinite(nx) &&
      Number.isFinite(ny) &&
      String(nx) === x &&
      String(ny) === y;
    const ord = bothNumeric ? nx - ny : x < y ? -1 : x > y ? 1 : 0;
    if (ord !== 0) return ord;
  }
  return 0;
}

/// Compare `MAJOR.MINOR.PATCH[-pre]`. A stable build beats a pre-release of
/// the same core version; otherwise pre-release identifiers compare
/// segment-by-segment, numerically where both sides are numeric (so
/// `beta.10` is newer than `beta.9`).
export function isNewerVersion(a: string, b: string): boolean {
  const x = parseVersion(a);
  const y = parseVersion(b);
  for (let i = 0; i < 3; i++) {
    if (x.core[i] !== y.core[i]) return x.core[i] > y.core[i];
  }
  if (x.pre === y.pre) return false;
  if (x.pre === null) return true;
  if (y.pre === null) return false;
  return comparePrerelease(x.pre, y.pre) > 0;
}
