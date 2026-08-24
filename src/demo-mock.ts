// Dev-only IPC mock: lets the real UI run in a plain browser (no Tauri shell)
// with fixture data mirroring a real backend. Loaded only when VITE_DEMO=1.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

// stub the Tauri window/webview singletons so getCurrentWindow()/
// getCurrentWebview() don't throw in a plain browser
mockWindows("main");
const internals = (window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> })
  .__TAURI_INTERNALS__;
if (internals && typeof internals.invoke !== "function") {
  internals.invoke = () => Promise.resolve();
}

const folders = [
  { id: 1, path: "C:\\Users\\stebe\\Documents\\dfine\\prompt-shelf", file_count: 190 },
];

const status = {
  version: "0.1.13-demo",
  repo_count: 1778,
  file_count: 190,
  folder_count: 1,
  bookmark_count: 2,
  history_count: 1515,
  clip_count: 42,
  clipboard_enabled: true,
  clip_retention_days: 30,
  clip_max_entries: 2000,
  max_file_mb: 16,
  hotkey: "Alt+Space",
  hf_endpoint: "https://huggingface.co",
  embedded_count: 1778,
  last_sync: new Date().toISOString(),
  username: "newdee",
  has_token: true,
  model: "ready",
  image_model: "ready",
  syncing: false,
  local_indexing: false,
};

const now = Math.floor(Date.now() / 1000);
const fileHits = [
  {
    id: 1,
    path: "C:\\Users\\stebe\\Documents\\dfine\\prompt-shelf\\notes\\sqlite-tips.md",
    name: "sqlite-tips.md",
    ext: "md",
    size: 4210,
    mtime: now - 86400,
    score: 12.5,
    thumb: null,
    snippet: "tuning \u0001sqlite\u0002 WAL mode for local search \u2026",
  },
  {
    id: 2,
    path: "C:\\Users\\stebe\\Documents\\dfine\\prompt-shelf\\src\\db.rs",
    name: "db.rs",
    ext: "rs",
    size: 18944,
    mtime: now - 7200,
    score: 9.1,
    thumb: null,
    snippet: "open a \u0001sqlite\u0002 connection with busy_timeout \u2026",
  },
];

mockIPC((cmd, args) => {
  switch (cmd) {
    case "get_status":
      return status;
    case "list_folders":
      return folders;
    case "add_folder":
    case "remove_folder":
      return folders;
    case "search_local":
      return (args as { query?: string })?.query ? fileHits : [];
    case "search_stars":
    case "search_bookmarks":
      return [];
    case "plugin:event|listen":
      return 1;
    default:
      return null; // never throw: unknown commands are inert in the demo
  }
});
