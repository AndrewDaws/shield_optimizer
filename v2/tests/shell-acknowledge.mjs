// Ticking the expert-shell box did not reliably arm Run.
//
// The wiring was reactive, but two places re-read the stored consent over live
// state: `loadApps` did it after an await, and `resetDeviceState` did it from a
// device that had just been nulled. `setShellAcknowledged` also no-opped when
// there was no hardware id, so the tick was never written in the first place.
// Between them, visiting Health or the App List — which reload constantly —
// silently unticked the box, or left it ticked with Run still disabled.
//
// Nothing threw. The box just stopped meaning anything, which on the one tab
// that runs arbitrary commands is a consent control that does not hold.
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
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Shell", exact: true }).click();

  const shell = page.locator("#tabpanel-shell");
  const ack = shell.getByRole("checkbox", { name: /I understand these risks/ });
  const editor = shell.getByRole("textbox", { name: "Shell command" });
  const run = shell.getByRole("button", { name: "Run", exact: true });
  // A disabled button never gets :hover, so the reason hangs off a wrapper.
  const runWrap = shell.locator(".run-wrap");

  await run.waitFor();
  assert.equal(await run.isDisabled(), true, "Run starts disabled");
  assert.equal(
    await runWrap.getAttribute("data-tip"),
    "Tick the acknowledgement above",
    "the untouched tab names the checkbox, not the empty prompt",
  );

  // Ticked, with a command typed: Run is armed.
  await ack.check();
  await editor.fill("uptime");
  assert.equal(await run.isDisabled(), false, "a ticked box and a command arm Run");
  assert.equal(
    await runWrap.getAttribute("data-tip"),
    null,
    "an armed Run carries no blocked reason",
  );

  // Ticked but empty: the other half of the reason, which the old hint never
  // mentioned — it only ever explained the checkbox.
  await editor.fill("");
  assert.equal(await run.isDisabled(), true, "an empty command still blocks Run");
  assert.equal(
    await runWrap.getAttribute("data-tip"),
    "Type a command first",
    "a ticked box with an empty prompt says so",
  );
  await editor.fill("uptime");

  // The regression itself: Health reloads the device report and the App List
  // reloads package states, and both used to stamp the consent back to false.
  for (const tab of ["Health", "App List", "Shell"]) {
    await page.getByRole("tab", { name: tab, exact: true }).click();
  }
  await ack.waitFor();
  assert.equal(await ack.isChecked(), true, "the tick survives a Health/App List round trip");
  assert.equal(await run.isDisabled(), false, "and Run is still armed after it");

  // Unticking is the control working in the other direction, and it has to
  // hold just as well.
  await ack.uncheck();
  assert.equal(await run.isDisabled(), true, "unticking disables Run");
  assert.equal(
    await runWrap.getAttribute("data-tip"),
    "Tick the acknowledgement above",
    "and the wrapper says which half is missing",
  );
  await page.getByRole("tab", { name: "Health", exact: true }).click();
  await page.getByRole("tab", { name: "Shell", exact: true }).click();
  assert.equal(await ack.isChecked(), false, "an untick survives the same round trip");

  console.log("Shell acknowledgement passed: the tick holds across tab reloads, both ways.");
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
