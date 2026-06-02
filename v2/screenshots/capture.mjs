// Repeatable screenshot capture for Shield Optimizer v2.
//
// Boots the SvelteKit dev server in demo mode (VITE_DEMO=1 → fixture-backed
// invoke(), no device needed), drives a headless Chromium through every screen
// in dark theme at a fixed retina viewport, and writes one PNG per screen to
// screenshots/frames/. `build-gif.sh` stitches those into gallery.gif.
//
// Run via `npm run screenshots` from v2/.

import { spawn } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { chromium } from "playwright";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const FRAMES = join(HERE, "frames");
const PORT = 1421;
const BASE = `http://localhost:${PORT}`;
const SERIAL = "192.168.1.42:5555";
const DEVICE_URL = `${BASE}/devices/${encodeURIComponent(SERIAL)}`;

// Fixed, uniform frame so every gallery slide is the same size.
const VIEWPORT = { width: 1280, height: 860 };

async function waitForServer(url, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`dev server did not come up at ${url} within ${timeoutMs}ms`);
}

async function main() {
  await rm(FRAMES, { recursive: true, force: true });
  await mkdir(FRAMES, { recursive: true });

  console.log("Starting dev server (demo mode)…");
  const server = spawn("npm", ["run", "dev", "--", "--port", String(PORT)], {
    cwd: V2,
    env: { ...process.env, VITE_DEMO: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", () => {});
  server.stderr.on("data", (d) => process.stderr.write(`[vite] ${d}`));

  const browser = await chromium.launch();
  let exitCode = 0;
  try {
    await waitForServer(BASE);
    console.log("Dev server up. Capturing…");

    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 2, // crisp retina PNGs
      colorScheme: "dark", // forces prefers-color-scheme: dark
    });
    const page = await context.newPage();

    let n = 0;
    const shot = async (name) => {
      n += 1;
      const file = join(FRAMES, `${String(n).padStart(2, "0")}-${name}.png`);
      await page.waitForTimeout(450); // let layout/fonts settle
      await page.screenshot({ path: file });
      console.log(`  ✓ ${file}`);
    };

    // 1. Device list (landing) with a connected Shield.
    await page.goto(BASE, { waitUntil: "networkidle" });
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();
    await shot("devices");

    // 2. Device → Overview (default tab).
    await page.goto(DEVICE_URL, { waitUntil: "networkidle" });
    await page.locator("#tab-overview").waitFor();
    await page.getByRole("heading", { name: "Profile" }).waitFor();
    await shot("overview");

    // 3. Health report.
    await page.locator("#tab-health").click();
    await page.getByText("3840x2160", { exact: false }).first().waitFor();
    await shot("health");

    // 4. Launcher.
    await page.locator("#tab-launcher").click();
    await page.getByText("Projectivy Launcher", { exact: false }).first().waitFor();
    await shot("launcher");

    // 5. App list.
    await page.locator("#tab-apps").click();
    await page.getByText("App List", { exact: false }).first().waitFor();
    await page.waitForTimeout(400);
    await shot("app-list");

    // 6. Optimize wizard — needs a click to load the plan.
    await page.locator("#tab-optimize").click();
    await page.getByRole("button", { name: "Optimize", exact: true }).click();
    await page.getByText("Run", { exact: false }).first().waitFor().catch(() => {});
    await page.waitForTimeout(600);
    await shot("optimize");

    // 7. Tweaks.
    await page.locator("#tab-tweaks").click();
    await page.getByText("HDMI", { exact: false }).first().waitFor();
    await shot("tweaks");

    // 8. Install APK.
    await page.locator("#tab-sideload").click();
    await page.getByText("Install APK", { exact: false }).first().waitFor();
    await shot("install-apk");

    // 9. Snapshot (per-device).
    await page.locator("#tab-snapshot").click();
    await page.waitForTimeout(400);
    await shot("snapshot");

    // 10. Global snapshots page.
    await page.goto(`${BASE}/snapshots`, { waitUntil: "networkidle" });
    await page.waitForTimeout(500);
    await shot("snapshots");

    await context.close();
    console.log(`\nCaptured ${n} frames to ${FRAMES}`);
  } catch (err) {
    console.error("Capture failed:", err);
    exitCode = 1;
  } finally {
    await browser.close();
    server.kill("SIGTERM");
  }
  process.exit(exitCode);
}

main();
