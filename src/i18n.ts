// Tiny built-in i18n: English strings are the keys, `t()` looks them up in
// the zh dictionary when the UI language is Chinese. No library — the app is
// a single window with ~130 strings, and English text doubles as the fallback
// for any key the dictionary misses.

export type LangPref = "auto" | "en" | "zh";
export type Lang = "en" | "zh";

const ZH: Record<string, string> = {
  // tabs
  "Local Files": "本地文件",
  "GitHub Stars": "GitHub Stars",
  Web: "Web",
  Clipboard: "剪贴板",
  // pills: sorts / scopes
  match: "匹配",
  recent: "最近",
  stars: "星数",
  all: "全部",
  text: "文本",
  images: "图片",
  bookmarks: "书签",
  history: "历史",
  // theme
  auto: "跟随系统",
  light: "浅色",
  dark: "深色",
  // misc option labels
  Unlimited: "无上限",
  "hf-mirror.com (China)": "hf-mirror.com（国内镜像）",
  // relative time
  today: "今天",
  "{n}d": "{n} 天",
  "{n}mo": "{n} 月",
  "{n}y": "{n} 年",
  // progress
  "listing stars… {n}": "拉取 star 列表… {n}",
  "readmes {a}/{b}": "README {a}/{b}",
  "indexing stars {a}/{b}": "索引 star {a}/{b}",
  "scanning files… {n}": "扫描文件… {n}",
  "indexing images {a}/{b}": "索引图片 {a}/{b}",
  "indexing files {a}/{b}": "索引文件 {a}/{b}",
  "indexing videos {a}/{b}": "索引视频 {a}/{b}",
  // footer status
  "error: {e}": "错误：{e}",
  "model download failed, keyword search only (set a mirror in settings)":
    "模型下载失败，仅关键词搜索可用（可在设置中切换镜像）",
  "preparing semantic model (first run downloads ~500 MB)":
    "正在准备语义模型（首次运行下载约 500 MB）",
  "preparing image model (first run downloads ~200 MB)":
    "正在准备图像模型（首次运行下载约 200 MB）",
  "{n} repos indexed": "已索引 {n} 个仓库",
  "{a} bookmarks · {b} history": "{a} 书签 · {b} 历史",
  "{n} clips recorded": "已记录 {n} 条剪贴",
  "clipboard history is off — enable it in settings": "剪贴板历史未开启 — 在设置中开启",
  "{n} files indexed": "已索引 {n} 个文件",
  // search input
  "Searching by image similarity": "按图像相似度搜索",
  "Search bookmarks and browser history": "搜索书签和浏览器历史",
  "Search your clipboard history": "搜索剪贴板历史",
  "Search {n} starred repos": "搜索 {n} 个已 star 仓库",
  "Search your stars": "搜索你的 stars",
  "Describe the image, or pick / drop / paste one": "描述图片，或选择 / 拖入 / 粘贴一张",
  "Search {n} local files, drop or paste an image": "搜索 {n} 个本地文件，可拖入或粘贴图片",
  "Search indexed folders": "搜索已索引的文件夹",
  "Search {s} (Shift+Tab cycles)": "搜索{s}（Shift+Tab 循环）",
  "Sort by {s}": "按{s}排序",
  "Search with an image file": "用图片文件搜索",
  "Re-fetch starred repos": "重新拉取 star 仓库",
  "Re-scan folders": "重新扫描文件夹",
  // collapse bars
  "Connect GitHub to sync your stars": "连接 GitHub 同步你的 stars",
  "No folders indexed yet, add some to search locally": "还没有索引文件夹，添加后即可本地搜索",
  // settings: frame
  Settings: "设置",
  "Back to search (Esc)": "返回搜索 (Esc)",
  Connection: "连接",
  Indexing: "索引",
  "Appearance & behavior": "外观与行为",
  Privacy: "隐私",
  System: "系统",
  // settings: github
  "Paste a new token to replace the current one.": "粘贴新 token 可替换当前的。",
  "A personal access token, no scopes needed — it only reads your public stars.":
    "一个 personal access token，无需任何 scope — 只读取你的公开 stars。",
  "not connected": "未连接",
  Checking: "验证中",
  Connect: "连接",
  "Create one on github.com": "去 github.com 创建",
  "Rebuild star index": "重建 star 索引",
  "Wipe the star index and sync everything from scratch": "清空 star 索引并从头重新同步",
  // settings: indexing
  "Indexed folders": "索引文件夹",
  "Scanned recursively; hidden and gitignored paths are skipped.":
    "递归扫描；跳过隐藏文件及 gitignore 路径。",
  "Add folder": "添加文件夹",
  "No folders yet.": "还没有文件夹。",
  "{n} folder(s) indexed but the list failed to load — please report this with the error below.":
    "已索引 {n} 个文件夹但列表加载失败 — 请附下方错误反馈。",
  "Rebuild this folder's index from scratch": "从零重建该文件夹的索引",
  "Remove from index": "从索引中移除",
  "Max file size": "文件大小上限",
  "Video shot search": "视频镜头搜索",
  "Search videos by name, or describe a scene": "按文件名搜视频，或描述一个画面",
  "system install": "系统已装",
  downloaded: "已下载",
  videos: "视频",
  "Image text (OCR)": "图片文字（OCR）",
  "Reads the text inside indexed images and video frames (screenshots, scans, subtitles) so you can search it — video hits jump to the moment the text appears. Off by default; enabling downloads a small model (~15 MB).":
    "识别已索引图片和视频帧里的文字（截图、扫描件、字幕），让它们可以被搜到——视频命中直接跳到文字出现的时刻。默认关闭；开启后下载一个小模型（约 15 MB）。",
  "OCR model": "OCR 模型",
  "Scanned PDFs": "扫描版 PDF",
  "Also read pages of PDFs that have no text layer. Large scans take a while, so this is your call.":
    "同时识别没有文字层的 PDF 页面。大部头扫描件较耗时，开不开由你。",
  "Decode limits": "解码限制",
  "Caps ffmpeg while indexing videos, so it never owns the machine. Hardware decode falls back to software if the driver fails.":
    "限制视频索引时 ffmpeg 的占用，后台跑不打扰前台。硬件解码失败自动回退软解。",
  "auto threads": "自动线程",
  "hw decode": "硬解",
  "Hardware decode (falls back to software on failure)": "硬件解码（失败自动回退软解）",
  "Videos in your folders are split into shots; each shot is searchable by image or description. Needs ffmpeg (auto-downloaded if missing).":
    "文件夹里的视频按镜头切分；每个镜头可用图片或文字描述搜索。需要 ffmpeg（缺失时自动下载）。",
  Video: "视频",
  "Larger files index by name only. Changing rebuilds.": "超限文件仅按文件名索引。修改后重建。",
  "Model download source": "模型下载源",
  "Pick the mirror if huggingface.co is unreachable from your network.":
    "网络连不上 huggingface.co 时选镜像。",
  "Semantic model": "语义模型",
  "Image model": "图像模型",
  ready: "就绪",
  "downloading (~500 MB, first run)…": "下载中（首次约 500 MB）…",
  "downloading (~200 MB, first run)…": "下载中（首次约 200 MB）…",
  // settings: appearance & behavior
  Theme: "主题",
  Language: "语言",
  "Palette and settings text; the tray menu follows.": "浮窗与设置文案；托盘菜单跟随。",
  "Pinyin app matching": "拼音匹配应用",
  "Latin queries match Chinese app names by full pinyin or initials (wx → 微信).":
    "拉丁输入按全拼或首字母匹配中文应用名（wx → 微信）。",
  "App aliases": "应用别名",
  "One rule per line: alias = app name. The alias matches like a second name (pinyin included).":
    "一行一条：别名 = 应用名。别名当作第二名字参与匹配（含拼音）。",
  "Save aliases": "保存别名",
  "Summon shortcut": "唤出快捷键",
  Currently: "当前",
  "Click and press a new combination; Backspace clears. OS-reserved chords (like ⌘Space) can't be captured.":
    "点击后按下新组合键；Backspace 清除。系统保留组合（如 ⌘Space）无法捕获。",
  "press keys…": "按下按键…",
  Apply: "应用",
  saved: "已保存",
  "use Ctrl/Alt/Win plus a key, or an F-key": "请用 Ctrl/Alt/Win 加一个键，或单独 F 键",
  Clear: "清除",
  "Reset to Alt+Space": "重置为 Alt+Space",
  Tabs: "标签页",
  "Tick which sources appear as tabs (at least one stays on). Drag the handle (or use the arrows) to reorder; ★ marks the tab that opens on launch.":
    "勾选要显示为标签页的源（至少保留一个）。拖动手柄（或用箭头）排序；★ 为启动时打开的标签页。",
  "At least one tab must stay visible": "至少保留一个可见标签页",
  "Show {s}": "显示 {s}",
  "Hide {s}": "隐藏 {s}",
  "Opens on launch": "启动时打开",
  "Make this the launch tab": "设为启动标签页",
  // settings: privacy
  "Clipboard history": "剪贴板历史",
  "Recorded locally, searchable in the Clipboard tab. Password-manager secrets are never stored.":
    "仅记录在本地，可在剪贴板标签页搜索。密码管理器的机密内容永不记录。",
  off: "关",
  on: "开",
  "Keep at most": "最多保留",
  unlimited: "无限制",
  "Keep for": "保留时长",
  "7 days": "7 天",
  "30 days": "30 天",
  forever: "永久",
  "Clear history": "清空历史",
  "Delete every recorded clip permanently.": "永久删除所有已记录的剪贴内容。",
  // settings: system
  Updates: "更新",
  "Version {v} is available.": "新版本 {v} 可用。",
  "You are on the latest version.": "已是最新版本。",
  "Installed in place; your index and settings are kept.": "原地安装；索引和设置保持不变。",
  "Update & restart": "更新并重启",
  "Check now": "检查更新",
  "Checking…": "检查中…",
  Logs: "日志",
  "Local activity log (errors, model/ffmpeg status) — attach it to a bug report. Queries are never logged.":
    "本地运行日志（错误、模型/ffmpeg 状态）——报 issue 时附上。搜索内容永不记录。",
  "Open log folder": "打开日志文件夹",
  "Settings file": "设置文件",
  "Everything except the GitHub token — move your setup to another machine.":
    "除 GitHub token 外的全部配置——换机器一键搬家。",
  Export: "导出",
  Import: "导入",
  // empty states
  "No matches in your stars": "stars 中无匹配",
  "No matching bookmarks or history": "无匹配的书签或历史",
  "No matching clips": "无匹配的剪贴内容",
  "No matches in indexed folders": "索引文件夹中无匹配",
  // result rows
  Application: "应用程序",
  App: "应用",
  Bookmark: "书签",
  History: "历史",
  "{n} lines": "{n} 行",
  archived: "已归档",
  "added {d}": "添加于 {d}",
  "modified {d}": "修改于 {d}",
  "last push {d}": "最后推送 {d}",
  // footer hints
  navigate: "移动",
  copy: "复制",
  open: "打开",
  source: "切换源",
  select: "多选",
  delete: "删除",
  scope: "范围",
  sort: "排序",
  web: "网页",
  hide: "隐藏",
  settings: "设置",
  preview: "预览",
  paste: "粘贴",
  "close preview": "收起预览",
  "Clipboard entry": "剪贴内容",
  Image: "图片",
  Shots: "镜头",
  "No preview": "无可预览内容",
};

export function resolveLang(pref: LangPref): Lang {
  if (pref === "en" || pref === "zh") return pref;
  try {
    return navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
  } catch {
    return "en";
  }
}

export function loadLangPref(): LangPref {
  try {
    const saved = localStorage.getItem("magpie.lang");
    if (saved === "en" || saved === "zh" || saved === "auto") return saved;
  } catch {
    /* default below */
  }
  return "auto";
}

let current: Lang = resolveLang(loadLangPref());

/// Must be called BEFORE the re-render that uses the new language (t() reads
/// module state synchronously during render).
export function setLang(l: Lang) {
  current = l;
}

export function t(s: string): string {
  return current === "zh" ? (ZH[s] ?? s) : s;
}

export function tf(s: string, vars: Record<string, string | number>): string {
  let out = t(s);
  for (const [k, v] of Object.entries(vars)) out = out.split(`{${k}}`).join(String(v));
  return out;
}
