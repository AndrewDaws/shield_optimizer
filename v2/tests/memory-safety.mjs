// Every row of Top Memory Users sat on "Checking" forever.
//
// The cause is a Svelte 5 trap worth a permanent guard: `report` is $state, so
// assigning an object stores a deep *proxy*. A later `report !== nextReport`
// identity check therefore compares a proxy against the raw object and is
// always true — so the guard meant to drop a superseded load dropped every
// load, and the resolved verdicts were never written.
//
// Nothing threw and nothing logged; the table just never finished. Only
// rendering it catches that.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Health" }).click();
  await page.getByText("Top Memory Users").waitFor();

  const verdicts = page.locator("table.mem-table tbody tr td.center");
  await page.waitForFunction(
    () =>
      [...document.querySelectorAll("table.mem-table tbody tr td.center")].every(
        (cell) => cell.textContent.trim() !== "CHECKING",
      ),
    null,
    { timeout: 10000 },
  );

  const shown = await verdicts.allInnerTexts();
  assert.ok(shown.length > 0, "the demo device has memory rows");
  assert.ok(
    !shown.includes("CHECKING"),
    `every lookup must resolve, got: ${JSON.stringify(shown)}`,
  );

  // Fail closed: an uncatalogued package is Unknown, never Safe. A row that
  // resolved to a real verdict is the point — "Unavailable" everywhere would
  // pass the check above while meaning the lookup broke.
  const allowed = new Set(["PROTECTED", "CAUTION", "UNKNOWN"]);
  assert.ok(
    shown.some((v) => allowed.has(v)),
    `at least one row must carry a real verdict, got: ${JSON.stringify(shown)}`,
  );
  assert.ok(!shown.includes("SAFE"), "memory rows never claim Safe");

  console.log(
    `Memory safety passed: ${shown.length} rows resolved to real verdicts, none left Checking.`,
  );
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
