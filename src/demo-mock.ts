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
  version: "0.1.19",
  repo_count: 1778,
  file_count: 1695,
  folder_count: 2,
  bookmark_count: 128,
  history_count: 1515,
  clip_count: 42,
  clipboard_enabled: true,
  clip_retention_days: 30,
  clip_max_entries: 2000,
  app_aliases: "proxy = clash",
  video_count: 12,
  video_shot_count: 486,
  video_indexing_enabled: true,
  video_indexing: false,
  video_note: "",
  ffmpeg_status: "system",
  video_decode_threads: 2,
  video_hwaccel: false,
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

// --- image-search fixtures: canvas-drawn abstract scenes (fictional data;
// keeps the demo free of binary assets). Used by the promo capture too. ---
function scenePng(draw: (c: CanvasRenderingContext2D, w: number, h: number) => void): string {
  const cv = document.createElement("canvas");
  cv.width = 640;
  cv.height = 400;
  const c = cv.getContext("2d")!;
  draw(c, 640, 400);
  return cv.toDataURL("image/jpeg", 0.92).split(",")[1];
}

const sunsetScene = (hue: number) => (c: CanvasRenderingContext2D, w: number, h: number) => {
  const sky = c.createLinearGradient(0, 0, 0, h * 0.72);
  sky.addColorStop(0, `hsl(${hue + 18} 62% 30%)`);
  sky.addColorStop(0.55, `hsl(${hue} 78% 52%)`);
  sky.addColorStop(1, `hsl(${hue - 14} 88% 64%)`);
  c.fillStyle = sky;
  c.fillRect(0, 0, w, h * 0.72);
  c.fillStyle = `hsl(${hue - 6} 92% 78%)`;
  c.beginPath();
  c.arc(w * 0.62, h * 0.56, 46, 0, Math.PI * 2);
  c.fill();
  const sea = c.createLinearGradient(0, h * 0.72, 0, h);
  sea.addColorStop(0, `hsl(${hue - 30} 45% 34%)`);
  sea.addColorStop(1, `hsl(${hue - 40} 40% 18%)`);
  c.fillStyle = sea;
  c.fillRect(0, h * 0.72, w, h * 0.28);
  c.fillStyle = "rgba(255,255,255,0.25)";
  for (let i = 0; i < 9; i++) c.fillRect(w * 0.5 + (i % 3) * 28 - 20, h * 0.74 + i * 9, 60 - i * 4, 2);
};

const mountainScene = (c: CanvasRenderingContext2D, w: number, h: number) => {
  const sky = c.createLinearGradient(0, 0, 0, h);
  sky.addColorStop(0, "hsl(215 45% 22%)");
  sky.addColorStop(1, "hsl(28 70% 60%)");
  c.fillStyle = sky;
  c.fillRect(0, 0, w, h);
  c.fillStyle = "hsl(230 25% 16%)";
  c.beginPath();
  c.moveTo(0, h);
  c.lineTo(w * 0.22, h * 0.34);
  c.lineTo(w * 0.46, h);
  c.fill();
  c.fillStyle = "hsl(228 22% 22%)";
  c.beginPath();
  c.moveTo(w * 0.3, h);
  c.lineTo(w * 0.62, h * 0.22);
  c.lineTo(w * 0.95, h);
  c.fill();
  c.fillStyle = "rgba(255,255,255,0.85)";
  c.beginPath();
  c.moveTo(w * 0.56, h * 0.34);
  c.lineTo(w * 0.62, h * 0.22);
  c.lineTo(w * 0.68, h * 0.34);
  c.lineTo(w * 0.62, h * 0.4);
  c.fill();
};

const cityScene = (c: CanvasRenderingContext2D, w: number, h: number) => {
  c.fillStyle = "hsl(250 30% 12%)";
  c.fillRect(0, 0, w, h);
  for (let i = 0; i < 14; i++) {
    const bw = 34 + ((i * 17) % 30);
    const bh = h * (0.3 + ((i * 29) % 50) / 100);
    c.fillStyle = `hsl(${248 - (i % 4) * 6} 25% ${15 + (i % 3) * 4}%)`;
    c.fillRect(i * 46, h - bh, bw, bh);
    c.fillStyle = "hsl(45 90% 62%)";
    for (let win = 0; win < 8; win++) {
      if ((i * 13 + win * 7) % 3 === 0)
        c.fillRect(i * 46 + 6 + (win % 2) * 14, h - bh + 10 + Math.floor(win / 2) * 18, 7, 9);
    }
  }
};

const queryImageB64 = scenePng(sunsetScene(28));
const imageHits = [
  { kind: "file", id: 101, path: "C:\\Users\\dfine\\Pictures\\screenshots\\sunset-cliff.jpg", name: "sunset-cliff.jpg", ext: "jpg", size: 482133, mtime: now - 12 * day, score: 0.94, thumb: scenePng(sunsetScene(24)), snippet: null },
  { kind: "video", id: 201, shot_id: 3101, path: "C:\\Users\\dfine\\Documents\\projects\\clips\\drone-coast.mp4", name: "drone-coast.mp4", start_ms: 204_000, end_ms: 221_500, ts_ms: 212_000, thumb: scenePng(sunsetScene(12)), duration_ms: 754_000, score: 0.89 },
  { kind: "file", id: 102, path: "C:\\Users\\dfine\\Pictures\\screenshots\\harbor-dusk.jpg", name: "harbor-dusk.jpg", ext: "jpg", size: 391002, mtime: now - 40 * day, score: 0.87, thumb: scenePng(sunsetScene(0)), snippet: null },
  { kind: "file", id: 103, path: "C:\\Users\\dfine\\Pictures\\screenshots\\alps-morning.jpg", name: "alps-morning.jpg", ext: "jpg", size: 512908, mtime: now - 90 * day, score: 0.79, thumb: scenePng(mountainScene), snippet: null },
  { kind: "file", id: 104, path: "C:\\Users\\dfine\\Pictures\\screenshots\\night-skyline.jpg", name: "night-skyline.jpg", ext: "jpg", size: 355670, mtime: now - 150 * day, score: 0.71, thumb: scenePng(cityScene), snippet: null },
];

// promo capture hook: simulate pasting an image so the palette enters the
// image-similarity state without a native clipboard
(window as unknown as { __demoPasteImage?: () => Promise<void> }).__demoPasteImage = async () => {
  const cv = document.createElement("canvas");
  cv.width = 640;
  cv.height = 400;
  sunsetScene(28)(cv.getContext("2d")!, 640, 400);
  const blob: Blob = await new Promise((r) => cv.toBlob((b) => r(b!), "image/jpeg", 0.92));
  const file = new File([blob], "sunset.jpg", { type: "image/jpeg" });
  const dt = new DataTransfer();
  dt.items.add(file);
  window.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt }));
};

const clipHits = [
  { id: 6, content: "", first_copied: now - 120, last_copied: now - 120, copy_count: 1, clip_kind: "image", thumb: scenePng(mountainScene), width: 1600, height: 1000, score: 0 },
  { id: 1, content: "cargo build --release -p magpie", first_copied: now - 300, last_copied: now - 300, copy_count: 1, clip_kind: "text", thumb: null, width: null, height: null, score: 0 },
  { id: 2, content: "https://github.com/newdee/magpie/releases/latest", first_copied: now - 3600, last_copied: now - 1200, copy_count: 3, clip_kind: "text", thumb: null, width: null, height: null, score: 0 },
  { id: 3, content: "SELECT rowid FROM files_fts WHERE files_fts MATCH ?1\nORDER BY bm25(files_fts, 8.0, 4.0, 4.0, 1.0)\nLIMIT 30", first_copied: now - 2 * 3600, last_copied: now - 2 * 3600, copy_count: 1, clip_kind: "text", thumb: null, width: null, height: null, score: 0 },
  { id: 4, content: "ffmpeg -i input.mp4 -c:v libx264 -crf 20 out.mp4", first_copied: now - day, last_copied: now - 5 * 3600, copy_count: 2, clip_kind: "text", thumb: null, width: null, height: null, score: 0 },
  { id: 5, content: "release-notes@plumeworks.example", first_copied: now - 2 * day, last_copied: now - 2 * day, copy_count: 1, clip_kind: "text", thumb: null, width: null, height: null, score: 0 },
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
    case "search_apps": {
      // prefix-narrowing like the real matcher: "v"/"vs"/"vsc" hit VS Code,
      // longer unrelated queries drop the app row
      const ql = q.toLowerCase();
      return ql && ("visual studio code".startsWith(ql) || "vscode".startsWith(ql))
        ? appHits
        : [];
    }
    case "search_local": {
      if ((args as { scope?: string })?.scope === "videos") {
        return q
          ? [
              imageHits[1], // the drone-coast shot hit
              { kind: "video", id: 202, shot_id: 0, path: "C:\\Users\\dfine\\Documents\\projects\\clips\\team-standup-recording.mp4", name: "team-standup-recording.mp4", start_ms: 0, end_ms: 0, ts_ms: 0, thumb: null, duration_ms: 1_922_000, score: 0.02 },
            ]
          : [];
      }
      const ql = q.toLowerCase();
      return ql && ("vector search".startsWith(ql) || ql.includes("vector"))
        ? fileHits.map((f) => ({ kind: "file", ...f }))
        : [];
    }
    case "search_by_image":
      return imageHits;
    case "get_preview": {
      const a = args as { kind?: string; id?: number };
      if (a.kind === "file" && (a.id ?? 0) > 100) {
        return { kind: "image", image: scenePng(sunsetScene(24)) };
      }
      if (a.kind === "file") {
        return {
          kind: "text",
          text: "# Vector search notes\n\nbrute-force dot product over L2-normalized vectors stays under 15 ms at tens of thousands of chunks — 100% recall, no ANN index to maintain.\n\nReciprocal rank fusion merges the FTS and vector candidate lists; each file ranks by its best chunk, so a sentence 100 pages deep still surfaces.\n\n## Follow-ups\n- benchmark sqlite-vec at 1M chunks\n- try int8 quantization for the resident store",
          clipped_head: false,
          clipped_tail: true,
        };
      }
      if (a.kind === "video") {
        return {
          kind: "shots",
          shots: Array.from({ length: 8 }, (_, i) => ({
            start_ms: i * 94_000,
            end_ms: (i + 1) * 94_000,
            ts_ms: i * 94_000 + 47_000,
            thumb: scenePng(i % 3 === 0 ? sunsetScene(20 + i) : i % 3 === 1 ? mountainScene : cityScene),
          })),
        };
      }
      if (a.kind === "repo") {
        return {
          kind: "repo",
          description: "A machine learning-based video super resolution and frame interpolation framework.",
          topics: JSON.stringify(["machine-learning", "video", "upscaling", "anime"]),
          homepage: null,
          starred_at: null,
          readme: "# video2x\n\nA lossless video/GIF/image upscaler achieved with waifu2x, Anime4K, SRMD and RealSR.\n\n## Features\n- Multi-driver support\n- Hardware acceleration\n- Cross-platform",
          readme_clipped: true,
        };
      }
      return { kind: "none" };
    }
    case "preview_thumb":
      return queryImageB64;
    case "search_web":
      return q ? webHits : [];
    case "search_clips":
      return clipHits; // recent list on empty query, matches on typed ones
    case "plugin:event|listen":
      return 1;
    case "plugin:updater|check":
      // ?update=1 simulates a pending release (exercises the footer red dot)
      return new URLSearchParams(location.search).has("update")
        ? { rid: 1, available: true, currentVersion: status.version, version: "9.9.9" }
        : null;
    default:
      return null; // never throw: unknown commands are inert in the demo
  }
});
