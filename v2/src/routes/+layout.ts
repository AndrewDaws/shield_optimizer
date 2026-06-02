// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Screenshot / offline-UI demo mode. With VITE_DEMO=1 there's no Tauri host,
// so we install a fixture-backed `invoke()` before any page component mounts.
// No-op (and tree-shaken) in real builds. See src/lib/demo-mock.ts.
if (import.meta.env.VITE_DEMO === "1" && typeof window !== "undefined") {
  const { installDemoMock } = await import("$lib/demo-mock");
  installDemoMock();
}
