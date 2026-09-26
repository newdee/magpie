// Query-box extras: bang-style web shortcuts and emoji lookup.
// Pure functions over localStorage-backed config — no backend involvement.
import emojilib from "emojilib";

export const BANGS_KEY = "magpie.bangs";
export const DEFAULT_BANGS = `g = https://www.google.com/search?q={q}
b = https://www.bing.com/search?q={q}
gh = https://github.com/search?q={q}&type=repositories
bd = https://www.baidu.com/s?wd={q}`;

/** One rule per line: `prefix = url-with-{q}`. Bad lines are skipped. */
export function parseBangs(text: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const line of text.split("\n")) {
    const eq = line.indexOf("=");
    if (eq < 1) continue;
    const prefix = line.slice(0, eq).trim().toLowerCase();
    const url = line.slice(eq + 1).trim();
    if (prefix && !prefix.includes(" ") && url.includes("{q}") && /^https?:\/\//i.test(url)) {
      out.set(prefix, url);
    }
  }
  return out;
}

export function loadBangs(): Map<string, string> {
  try {
    return parseBangs(localStorage.getItem(BANGS_KEY) ?? DEFAULT_BANGS);
  } catch {
    return parseBangs(DEFAULT_BANGS);
  }
}

export interface BangMatch {
  prefix: string;
  url: string;
  host: string;
  rest: string;
}

/** `g rust tokio` -> the g rule with rest "rust tokio". Needs a space and a
 * non-empty query so plain prefixes still search normally. */
export function matchBang(query: string, bangs: Map<string, string>): BangMatch | null {
  const sp = query.indexOf(" ");
  if (sp < 1) return null;
  const prefix = query.slice(0, sp).toLowerCase();
  const rest = query.slice(sp + 1).trim();
  const tpl = bangs.get(prefix);
  if (!tpl || !rest) return null;
  const url = tpl.replace("{q}", encodeURIComponent(rest));
  let host = "";
  try {
    host = new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
  return { prefix, url, host, rest };
}

// ---------- quick notes ----------

export interface NoteMatch {
  text: string;
}

/** `note buy milk` -> a note hit; `note` alone or `notebook` stay searches. */
export function matchNote(query: string): NoteMatch | null {
  const m = /^note\s+(\S[\s\S]*)$/i.exec(query.trim());
  if (!m) return null;
  const text = m[1].trim();
  return text ? { text } : null;
}

// ---------- recent opens on the empty box ----------

export const RECENTS_KEY = "magpie.recents";

export function recentsEnabled(): boolean {
  try {
    return localStorage.getItem(RECENTS_KEY) === "1";
  } catch {
    return false;
  }
}

export function setRecentsEnabled(on: boolean): void {
  try {
    localStorage.setItem(RECENTS_KEY, on ? "1" : "0");
  } catch {
    /* the choice just won't persist */
  }
}

// ---------- launch tips ----------

export const TIPS_KEY = "magpie.tips";

/** One-liner discoverability tips, shown in the empty state. English text
 * doubles as the i18n key (translated in i18n.ts like everything else). */
export const TIPS: string[] = [
  "Ctrl+P pins a clip — it sorts first and never gets pruned",
  "The query box is a calculator: 3*(5+2)^2 — Enter copies the answer",
  "Type : then a keyword to find emoji — :fire or :火",
  "gh magpie searches GitHub straight from the box — prefixes are editable in settings",
  "Shift+Enter pastes a clip straight into the app you came from",
  "→ previews the selected result — file text, images, a video's shots",
  "Ctrl+Shift+C puts the file itself on the clipboard — paste it as an attachment",
  "Drop or paste an image to search your files by visual similarity",
  "ts 1700000000 turns a unix timestamp into local time",
  "#ff6600 shows the color with rgb/hsl — Enter copies the hex",
  "pwd 24 generates a cryptographically random password",
  "Enable OCR in settings — words inside screenshots and videos become searchable",
  "Enter on a video hit starts playback right at the matched scene",
  "App names match by pinyin too — wx finds 微信, vsc finds VS Code",
  "Ctrl+Enter hands your query to the browser — URLs open directly",
  "Ctrl+1 to Ctrl+9 jump straight to a tab (⌘ on a Mac)",
  "Ctrl+K on a result lists everything you can do with it (⌘K on a Mac)",
  "Ctrl+K on a PDF copies or saves it as Markdown, every page in order",
  "kill chrome lists running Chrome processes; Enter twice ends one",
  "lock, sleep or dark mode run straight from the box",
  "tokyo time, or 3pm pst to beijing: time zones work offline",
  "Unit conversion lives in the box: 100 mb to gb, 32 f to c",
  "Give apps aliases in settings: proxy = clash",
  "Shift+Tab cycles the local scope: all / text / images / videos",
  "Export your whole setup from Settings → About — the GitHub token stays out",
  "Ctrl+C copies whatever identifies a row — a path, a URL, a clip's text",
  "Recent opens on the empty box — switch it on in Settings → General",
  "json alone pretty-prints your clipboard — upper, lower, slug, lines, count too",
  "json min squeezes JSON onto one line, json sort orders its keys; bad JSON shows where it breaks",
  "md5, sha1 or sha256 hash your clipboard, or the text you type after them",
  "After a screenshot, type ocr and the text on it is ready to copy",
  "port 3000 shows what is listening on it; Enter twice ends it",
  "Ctrl+K on a file opens its folder in a terminal, or the file in your editor",
  "Ctrl+K on a file can also move it to the Recycle Bin (the Trash on a Mac); Enter twice confirms",
  "dl lists the newest files in your Downloads folder",
  "大写 1234.56 writes an amount in Chinese capitals; py 重庆 gives its pinyin",
  "timer 25m standup starts a countdown; a notification comes when it ends",
  "diff compares the last two texts you copied, line by line",
  "qr with an image on the clipboard reads the QR code in it",
  "sys shows CPU, memory and free disk space; pick, dice and coin decide for you",
  ":sym lists symbols like → × ¥ ⌘; ip shows your local network address",
  "jwt decodes the token on your clipboard and tells you when it expires — nothing leaves your machine",
  "snake, kebab, camel, pascal, title: snake MyMacCleaner gives my_mac_cleaner",
  "qr turns your clipboard into a QR code; Enter copies it as an image",
  "Date math lives in the box: today + 30d, until 2026-10-01, 2026-10-01 - today",
  "Narrow file searches: ext:pdf, >10mb, 7d, in:projects",
  "note buy milk — one timestamped line into your notes file",
  "Ctrl+Alt+Space looks up the text you have selected in any app — Option+Shift+Space on a Mac",
];

export function tipsEnabled(): boolean {
  try {
    return localStorage.getItem(TIPS_KEY) !== "0";
  } catch {
    return true;
  }
}

export function randomTip(): string {
  return TIPS[Math.floor(Math.random() * TIPS.length)];
}

/** A different tip than `current` — the rotation must never repeat itself
 * in place, which reads as a stuck UI. */
export function nextTip(current: string): string {
  if (TIPS.length < 2) return TIPS[0] ?? current;
  let pick = current;
  while (pick === current) pick = randomTip();
  return pick;
}

// ---------- emoji ----------

export interface EmojiHit {
  emoji: string;
  name: string;
}

// A hand-picked Chinese keyword layer over emojilib's English keywords, for
// the emoji people actually reach for.
const ZH: Record<string, string> = {
  "😂": "笑哭 大笑", "❤️": "爱心 红心", "👍": "赞 好", "🔥": "火 热",
  "😭": "哭", "🎉": "庆祝 撒花", "😄": "笑 开心", "🙏": "拜托 感谢 祈祷",
  "😅": "尬笑 汗", "🤣": "笑翻", "💪": "加油 肌肉", "👏": "鼓掌",
  "🌹": "玫瑰 花", "😊": "微笑", "🎂": "蛋糕 生日", "☕": "咖啡",
  "😎": "酷 墨镜", "🤔": "思考 疑惑", "👌": "OK 没问题", "💯": "满分",
  "😴": "困 睡觉", "🍺": "啤酒 干杯", "🌙": "月亮 晚安", "☀️": "太阳",
  "🐶": "狗", "🐱": "猫", "🚀": "火箭 起飞", "⭐": "星星",
  "💰": "钱", "🎁": "礼物", "😡": "生气 愤怒", "🥰": "喜欢 爱",
  "🤝": "握手 合作", "✅": "对勾 完成", "❌": "叉 错误", "⚠️": "警告 注意",
  "🙈": "捂脸", "🍉": "西瓜 吃瓜", "🧧": "红包", "🐂": "牛",
};

const ALL: [string, string[]][] = Object.entries(emojilib as Record<string, string[]>);

// Symbols that are not emoji but get typed all the time, with English and
// Chinese keywords (the first English one is the name). `:sym` / `:符号`
// lists them all; otherwise they rank after emoji at the same match level.
const SYMBOLS: [string, string, string][] = [
  ["×", "multiply times x", "乘 乘号"], ["÷", "divide", "除 除号"], ["±", "plus_minus", "正负"],
  ["≈", "approx almost", "约等于"], ["≠", "not_equal", "不等于"], ["≤", "less_equal", "小于等于"],
  ["≥", "greater_equal", "大于等于"], ["∞", "infinity", "无穷"], ["√", "sqrt root", "根号"],
  ["∑", "sum sigma", "求和"], ["π", "pi", "圆周率"], ["°", "degree", "度"],
  ["℃", "celsius", "摄氏度"], ["℉", "fahrenheit", "华氏度"], ["‰", "permille", "千分号"],
  ["→", "arrow right", "右箭头 箭头"], ["←", "arrow left", "左箭头 箭头"], ["↑", "arrow up", "上箭头 箭头"],
  ["↓", "arrow down", "下箭头 箭头"], ["↔", "arrow both", "双向箭头 箭头"], ["⇒", "implies double_arrow", "推出"],
  ["⇔", "iff equivalent", "等价"], ["•", "bullet dot", "圆点 项目符号"], ["·", "middle_dot interpunct", "间隔号 点"],
  ["…", "ellipsis dots", "省略号"], ["—", "em_dash dash", "破折号"], ["–", "en_dash dash", "连接号"],
  ["¥", "yen yuan rmb cny", "人民币 元 日元"], ["€", "euro eur", "欧元"], ["£", "pound gbp", "英镑"],
  ["¢", "cent", "分"], ["©", "copyright", "版权"], ["®", "registered", "注册"],
  ["™", "trademark tm", "商标"], ["§", "section", "章节"], ["¶", "paragraph pilcrow", "段落"],
  ["✓", "check tick", "对勾 勾"], ["✗", "cross wrong", "叉 错"], ["★", "star black", "实心星 星"],
  ["☆", "star white", "空心星 星"], ["♠", "spade", "黑桃"], ["♥", "heart suit", "红桃"],
  ["♦", "diamond", "方块"], ["♣", "club", "梅花"], ["⌘", "command cmd", "命令键"],
  ["⌥", "option alt", "选项键"], ["⇧", "shift", "上档键"], ["⌃", "control ctrl", "控制键"],
  ["⏎", "return enter", "回车"], ["⌫", "backspace delete", "退格"], ["⇥", "tab", "制表"],
  ["⎋", "escape esc", "退出键"], ["「」", "corner_brackets quote", "直角引号 引号"], ["『』", "white_corner_brackets", "双直角引号 引号"],
  ["【】", "lenticular_brackets", "方头括号 括号"], ["《》", "book_title angle_brackets", "书名号"], ["²", "squared superscript_2", "平方 上标"],
  ["³", "cubed superscript_3", "立方 上标"], ["½", "half", "二分之一"], ["¼", "quarter", "四分之一"],
  ["α", "alpha", "阿尔法"], ["β", "beta", "贝塔"], ["γ", "gamma", "伽马"],
  ["Δ", "delta", "德尔塔 变化量"], ["μ", "micro mu", "微"], ["Ω", "omega ohm", "欧姆"],
];
const SYMBOL_ENTRIES: [string, string[], string[]][] = SYMBOLS.map(([s, en, zh]) => [s, en.split(" "), zh.split(" ")]);

/** Search by English keywords (emojilib) or the Chinese layer, symbols
 * included. Empty query returns a popular starter set. */
export function searchEmoji(q: string, limit = 40): EmojiHit[] {
  const query = q.trim().toLowerCase();
  if (!query) {
    return Object.keys(ZH).slice(0, limit).map((e) => ({ emoji: e, name: ZH[e] }));
  }
  if (query === "sym" || query === "symbol" || query === "符号") {
    return SYMBOL_ENTRIES.slice(0, Math.max(limit, SYMBOL_ENTRIES.length)).map(([s, en, zh]) => ({
      emoji: s,
      name: `${en[0].replace(/_/g, " ")} ${zh[0]}`,
    }));
  }
  // primary-name match first, then any exact keyword, then substrings;
  // emoji before symbols at each level
  const primary: EmojiHit[] = [];
  const exact: EmojiHit[] = [];
  const partial: EmojiHit[] = [];
  const entries: [string, string[], string[] | null][] = [
    ...ALL.map(([e, k]): [string, string[], string[] | null] => [e, k, null]),
    ...SYMBOL_ENTRIES,
  ];
  for (const [emoji, keywords, symbolZh] of entries) {
    const zh = symbolZh ? symbolZh.join(" ") : ZH[emoji];
    const zhWords = zh ? zh.split(" ") : [];
    const hit = {
      emoji,
      name: symbolZh ? `${keywords[0].replace(/_/g, " ")} ${symbolZh[0]}` : (zh ?? keywords[0].replace(/_/g, " ")),
    };
    // a symbol is never a primary match: :heart should lead with ❤️, not ♥
    if (!symbolZh && (keywords[0] === query || zhWords[0] === query)) {
      primary.push(hit);
    } else if (keywords.includes(query) || zhWords.includes(query)) {
      exact.push(hit);
    } else if (keywords.some((k) => k.includes(query)) || zhWords.some((w) => w.includes(query))) {
      partial.push(hit);
    }
    if (primary.length >= limit) break;
  }
  return primary.concat(exact, partial).slice(0, limit);
}
