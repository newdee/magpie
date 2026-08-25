// Dev-only IPC mock: lets the real UI run in a plain browser (no Tauri shell)
// with fixture data mirroring a real backend. Loaded only when VITE_DEMO=1.
// Also used to produce README screenshots — the fixtures below are styled
// after real data so the shots look like the shipped app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

// stub the Tauri window/webview singletons so getCurrentWindow()/
// getCurrentWebview() don't throw in a plain browser
mockWindows("main");
const internals = (window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> })
  .__TAURI_INTERNALS__;
if (internals && typeof internals.invoke !== "function") {
  internals.invoke = () => Promise.resolve();
}

// screenshot backdrop: float the palette over a wallpaper-ish gradient so the
// frosted panel reads the way it does on a real desktop
const style = document.createElement("style");
style.textContent = `
  body { background: linear-gradient(135deg, #1b2735 0%, #090a0f 55%, #2c1b35 100%); min-height: 100vh; }
  #root { max-width: 740px; margin: 42px auto; }
`;
document.head.appendChild(style);

const folders = [
  { id: 1, path: "C:\\Users\\dfine\\Documents\\projects", file_count: 1284 },
  { id: 2, path: "C:\\Users\\dfine\\Pictures\\screenshots", file_count: 411 },
];

const status = {
  version: "0.1.14",
  repo_count: 1778,
  file_count: 1695,
  folder_count: 2,
  bookmark_count: 128,
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
const day = 86_400;

const repoHits = [
  { id: 1, full_name: "k4yt3x/video2x", description: "A machine learning-based video super resolution and frame interpolation framework.", language: "C++", stars: 21432, html_url: "https://github.com/k4yt3x/video2x", archived: false, pushed_at: new Date((now - 5 * day) * 1000).toISOString(), score: 0.09 },
  { id: 2, full_name: "Lightricks/LTX-Video", description: "Official repository for LTX-Video", language: "Python", stars: 11020, html_url: "https://github.com/Lightricks/LTX-Video", archived: false, pushed_at: new Date((now - 12 * day) * 1000).toISOString(), score: 0.08 },
  { id: 3, full_name: "OpenTalker/video-retalking", description: "[SIGGRAPH Asia 2022] VideoReTalking: Audio-based Lip Synchronization for Talking Head Video Editing", language: "Python", stars: 7310, html_url: "https://github.com/OpenTalker/video-retalking", archived: false, pushed_at: new Date((now - 400 * day) * 1000).toISOString(), score: 0.07 },
  { id: 4, full_name: "HKUDS/VideoRAG", description: "[KDD'2026] \"VideoRAG: Chat with Your Videos\"", language: "Python", stars: 3355, html_url: "https://github.com/HKUDS/VideoRAG", archived: false, pushed_at: new Date((now - 60 * day) * 1000).toISOString(), score: 0.07 },
  { id: 5, full_name: "abhiTronix/vidgear", description: "A High-performance cross-platform Video Processing Python framework powerpacked with unique tricks", language: "Python", stars: 3712, html_url: "https://github.com/abhiTronix/vidgear", archived: false, pushed_at: new Date((now - 90 * day) * 1000).toISOString(), score: 0.06 },
  { id: 6, full_name: "williamyang1991/Rerender_A_Video", description: "[SIGGRAPH Asia 2023] Rerender A Video: Zero-Shot Text-Guided Video-to-Video Translation", language: "Jupyter Notebook", stars: 3020, html_url: "https://github.com/williamyang1991/Rerender_A_Video", archived: false, pushed_at: new Date((now - 700 * day) * 1000).toISOString(), score: 0.06 },
];

const fileHits = [
  { id: 1, path: "C:\\Users\\dfine\\Documents\\projects\\notes\\vector-search.md", name: "vector-search.md", ext: "md", size: 8210, mtime: now - day, score: 12.5, thumb: null, snippet: "brute-force dot product over L2-normalized \u0001vectors\u0002 stays under 15 ms \u2026" },
  { id: 2, path: "C:\\Users\\dfine\\Documents\\projects\\magpie\\core\\src\\search.rs", name: "search.rs", ext: "rs", size: 18944, mtime: now - 2 * 3600, score: 9.1, thumb: null, snippet: "fuse FTS and \u0001vector\u0002 candidates with reciprocal rank fusion \u2026" },
  { id: 3, path: "C:\\Users\\dfine\\Documents\\projects\\paper\\rag-survey.pdf", name: "rag-survey.pdf", ext: "pdf", size: 2843210, mtime: now - 6 * day, score: 7.8, thumb: null, snippet: "dense \u0001vector\u0002 retrieval consistently outperforms sparse baselines when \u2026" },
  { id: 4, path: "C:\\Users\\dfine\\Documents\\projects\\slides\\launch-deck.pptx", name: "launch-deck.pptx", ext: "pptx", size: 5312876, mtime: now - 14 * day, score: 5.2, thumb: null, snippet: null },
];

const appHits = [
  { name: "Visual Studio Code", target: "C:\\ProgramData\\...\\Visual Studio Code.lnk", score: 0.9 },
];

const webHits = [
  { kind: "bookmark", id: 1, url: "https://tauri.app/v2/guides/", title: "Tauri v2 Guides", folder: "Dev/Tauri", browser: "chrome", added_at: now - 30 * day, score: 0.12 },
  { kind: "history", id: 11, url: "https://docs.rs/rusqlite/latest/rusqlite/", title: "rusqlite - Rust", browser: "chrome", visit_count: 37, last_visit: now - 2 * 3600, score: 0.1 },
  { kind: "bookmark", id: 2, url: "https://sqlite.org/fts5.html", title: "SQLite FTS5 Extension", folder: "Dev/DB", browser: "edge", added_at: now - 90 * day, score: 0.09 },
  { kind: "history", id: 12, url: "https://github.com/newdee/magpie", title: "newdee/magpie: Spotlight-style local search", browser: "chrome", visit_count: 24, last_visit: now - 20 * 60, score: 0.09 },
  { kind: "history", id: 13, url: "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX", title: "onnx-community/siglip2-base-patch16-224-ONNX · Hugging Face", browser: "edge", visit_count: 9, last_visit: now - 3 * day, score: 0.07 },
];

const clipHits = [
  { id: 1, content: "cargo build --release -p magpie", first_copied: now - 300, last_copied: now - 300, copy_count: 1, score: 0 },
  { id: 2, content: "https://github.com/newdee/magpie/releases/latest", first_copied: now - 3600, last_copied: now - 1200, copy_count: 3, score: 0 },
  { id: 3, content: "SELECT rowid FROM files_fts WHERE files_fts MATCH ?1\nORDER BY bm25(files_fts, 8.0, 4.0, 4.0, 1.0)\nLIMIT 30", first_copied: now - 2 * 3600, last_copied: now - 2 * 3600, copy_count: 1, score: 0 },
  { id: 4, content: "ffmpeg -i input.mp4 -c:v libx264 -crf 20 out.mp4", first_copied: now - day, last_copied: now - 5 * 3600, copy_count: 2, score: 0 },
  { id: 5, content: "kian@anureka-demo.example", first_copied: now - 2 * day, last_copied: now - 2 * day, copy_count: 1, score: 0 },
];

mockIPC((cmd, args) => {
  const q = (args as { query?: string })?.query ?? "";
  switch (cmd) {
    case "get_status":
      return status;
    case "list_folders":
      return folders;
    case "add_folder":
    case "remove_folder":
      return folders;
    case "search_stars":
      return q ? repoHits : [];
    case "search_apps":
      return q.toLowerCase().startsWith("v") ? appHits : [];
    case "search_local":
      return q ? fileHits : [];
    case "search_web":
      return q ? webHits : [];
    case "search_clips":
      return clipHits; // recent list on empty query, matches on typed ones
    case "plugin:event|listen":
      return 1;
    default:
      return null; // never throw: unknown commands are inert in the demo
  }
});
