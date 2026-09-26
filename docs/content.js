// All page copy, in both languages. One source of truth so the two versions
// can never drift apart: the page swaps text nodes, it never reloads.
window.CONTENT = {
  en: {
    "nav.tour": "Tour",
    "hero.eyebrow": "Local-first · Open source",
    "stat.sources": "sources",
    "stat.local": "on your machine",
    "stat.embed": "per query",
    "stat.platforms": "platforms",
    "nav.download": "Download",
    "hero.title": "Everything you saved.<br />One keystroke away.",
    "hero.lede":
      "A search box you open with a hotkey. It covers your files, screenshots and videos, GitHub stars, browser bookmarks and history, and your clipboard. It matches by meaning, so half a file name is usually enough. The index and the models stay on your computer and work offline.",
    "hero.download": "Download",
    "hero.source": "View source",
    "hero.meta": "Free · MIT · Windows / macOS / Linux",
    "g1.title": "What it searches",
    "g1.sub": "Six sources in one box. <kbd>Tab</kbd> switches between them.",
    "g2.title": "It reads the contents",
    "g2.sub":
      "Besides file names, magpie looks at what is in your pictures and videos, and reads the text in them.",
    "g3.title": "Tools in the box",
    "g3.sub": "Some queries have an answer rather than results. <kbd>Enter</kbd> copies it.",
    "g4.title": "Day to day",
    "g4.sub": "Details you notice once it becomes part of your routine.",
    "g6.title": "For AI assistants",
    "g6.sub": "With the MCP server on, Claude Code, Cursor and other MCP clients can query the same index. It is off by default.",
    "rest.title": "More",
    "rest.sub": "Smaller features, in no particular order.",
    "g5.title": "Privacy",
    "stars.title": "Star history",
    "stars.cta": "Add yours",
    "close.title": "Find what you saved, with one hotkey.",
    "close.docs": "Full documentation",
    "footer.built": "Built with Rust + Tauri.",
  },
  zh: {
    "nav.tour": "功能",
    "hero.eyebrow": "本地运行 · 开源免费",
    "stat.sources": "个搜索来源",
    "stat.local": "本机处理",
    "stat.embed": "理解一次查询",
    "stat.platforms": "个平台",
    "nav.download": "下载",
    "hero.title": "你存过的东西，<br />它都能叼回来。",
    "hero.lede":
      "喜鹊爱把亮晶晶的小玩意儿往窝里衔，magpie 替你衔回来的，是文件、截图、视频、GitHub star、书签和剪贴板。名字只记得半截也不要紧，说个大概就能对上号。东西都放在本机，一步也不出门。",
    "hero.download": "下载",
    "hero.source": "查看源码",
    "hero.meta": "免费 · MIT · Windows / macOS / Linux",
    "g1.title": "能搜什么",
    "g1.sub": "六个来源，一个搜索框，<kbd>Tab</kbd> 键来回切换。",
    "g2.title": "认名字，更认内容",
    "g2.sub": "文件名只是门面。图里画了什么、视频里演了什么、上面印着什么字，magpie 都看在眼里。",
    "g3.title": "搜索框里的百宝箱",
    "g3.sub": "有些事犯不着兴师动众去搜。输进去，按 <kbd>Enter</kbd>，结果就进了剪贴板。",
    "g4.title": "日常用着顺手",
    "g4.sub": "好不好用，往往就差在这些细枝末节上。",
    "g6.title": "给 AI 助手也开一扇门",
    "g6.sub": "打开 MCP 之后，Claude Code、Cursor 等支持 MCP 的工具也能借用这份索引。默认关闭。",
    "rest.title": "零零碎碎，还有这些",
    "rest.sub": "都是些小功能，用久了自然会碰上。",
    "g5.title": "隐私，经得起查",
    "stars.title": "Star 增长",
    "stars.cta": "来一颗",
    "close.title": "东西随手放，找时一键到。",
    "close.docs": "完整文档",
    "footer.built": "用 Rust + Tauri 构建。",
  },
};

// Feature cards: one screenshot each, grouped by section id.
window.FEATURES = {
  g1: [
    {
      img: "files.png",
      tag: "Local files",
      en: {
        t: "Files, contents too",
        d: "Add the folders you work in. Any file can be found by name, and about 80 text formats plus PDF and Office documents can be searched by their contents. The matching line is highlighted.",
      },
      zh: {
        t: "本地文件，连正文也搜",
        d: "把常用的文件夹交给它。按名字找自不必说，约 80 种文本格式外加 PDF、Office 文档，连正文也能搜，命中的那一行高亮标出。",
      },
    },
    {
      img: "apps.png",
      tag: "App launcher",
      en: {
        t: "Also an app launcher",
        d: "Type part of an app's name and press Enter. It matches the start of a word, the middle, or initials such as vsc. Chinese app names match by pinyin, and you can add your own aliases.",
      },
      zh: {
        t: "顺带当个启动器",
        d: "敲应用名里的几个字，回车即开。开头、中间、首字母缩写（vsc 这种）都认得；中文应用名打拼音就行，还能给应用起个顺口的别名。",
      },
    },
    {
      img: "stars.png",
      tag: "GitHub stars",
      en: {
        t: "Stars, READMEs too",
        d: "Every repo you starred is indexed together with its README, so a single sentence you remember from the README is enough to find it. Sort by relevance, by when you starred it, or by star count.",
      },
      zh: {
        t: "star 过就跑不了",
        d: "所有 star 过的仓库连同 README 一并收录。哪怕只记得 README 里的某一句话，也能顺藤摸瓜找回项目。可按相关度、star 时间、星数排序。",
      },
    },
    {
      img: "web.png",
      tag: "Bookmarks + history",
      en: {
        t: "Bookmarks and history",
        d: "Read from Chrome, Edge and other Chromium browsers, and from Firefox and its forks such as LibreWolf, Zen and Floorp, across all profiles. A URL saved in several browsers is listed once, with each browser's icon. Pages you open often rank higher.",
      },
      zh: {
        t: "书签和浏览记录",
        d: "Chrome、Edge 这些 Chromium 系，Firefox、LibreWolf、Zen、Floorp 这些 Firefox 系，所有 profile 的书签和历史一网打尽。同一个网址散在几个浏览器里，合并成一条，右边挂着各自的浏览器图标；常去的网页排在前头。",
      },
    },
    {
      img: "clipboard.png",
      tag: "Clipboard",
      en: {
        t: "Clipboard history",
        d: "Off by default and stored locally. Copy an old entry again, paste it into the app you were using, or pin the ones you use daily. Anything your password manager marks as confidential is never stored.",
      },
      zh: {
        t: "剪贴板历史",
        d: "默认关闭，开了也只存在本机。旧内容可以再复制一遍、直接粘回刚才的应用，天天用的还能钉住。密码管理器标成机密的内容，一个字也不留。",
      },
    },
    {
      img: "clip-image.png",
      tag: "Copied images",
      en: {
        t: "Copied images",
        d: "Images you copy are kept with a thumbnail. Describe what was in the picture to find it again, the same way you search image files.",
      },
      zh: {
        t: "复制的截图有迹可循",
        d: "以图片形式复制的内容会连缩略图一起记下。回头描述一下画面，就能把它找出来，和搜图片文件一个路数。",
      },
    },
    {
      img: "videos.png",
      tag: "Video scenes",
      en: {
        t: "A scene inside a video",
        d: "Videos are split into shots. Describe a scene, or drop in a picture, to jump to that point with its timestamp. Enter starts playback there.",
      },
      zh: {
        t: "视频里的某个画面",
        d: "视频会被切成一个个镜头。描述记忆里的画面，或者拖进一张图，就能定位到那一段，回车从那里开始播。从前是大海捞针，现在是按图索骥。",
      },
    },
  ],
  g2: [
    {
      img: "images.png",
      tag: "Images",
      en: {
        t: "Images by content",
        d: "Describe an image in any language, or drop one in to find similar ones. The image model runs locally.",
      },
      zh: {
        t: "看图说话，按画面找图",
        d: "用一句话描述图片内容，中文英文都行；也可以拖一张图进来，找长得相像的。识图模型就在本机运行。",
      },
    },
    {
      img: "ocr.png",
      tag: "Text in images",
      en: {
        t: "Text in screenshots",
        d: "Turn on OCR in settings and the Chinese and English text in your images becomes searchable. Useful for error messages and chat logs you took screenshots of.",
      },
      zh: {
        t: "截图里的字，也能搜",
        d: "在设置里打开 OCR，图片里的中英文会被识别出来。几周前截下的报错、聊天记录，不必再一张张去翻。",
      },
    },
    {
      img: "ocr-video.png",
      tag: "Text in videos",
      en: {
        t: "Text in videos",
        d: "The same applies to video frames: subtitles, slides, code on a shared screen. Opening a result starts playback where the text appears.",
      },
      zh: {
        t: "视频里的字幕和板书",
        d: "视频画面里的字也照读不误，字幕、幻灯片、别人共享屏幕上的代码都在其列。点开结果，从那段文字出现的地方开始播放。",
      },
    },
    {
      img: "crosslang.png",
      tag: "Any language",
      en: {
        t: "Any language",
        d: "A Chinese query finds English READMEs, and the reverse. The model covers 100+ languages, so it doesn't matter which one the original was in.",
      },
      zh: {
        t: "中文问，英文答",
        d: "用中文搜，英文 README 照样找得到，反之亦然。模型懂一百多种语言，不必记得当初读的是哪国文字。",
      },
    },
    {
      img: "preview.png",
      tag: "Preview",
      en: {
        t: "Preview first",
        d: "<kbd>→</kbd> opens a preview beside the list: file text with your terms highlighted, the full image, a video's shots, the top of a README.",
      },
      zh: {
        t: "先看一眼，再打开",
        d: "按 <kbd>→</kbd> 在列表旁展开预览：文件正文带高亮，图片看大图，视频看镜头，仓库看 README 开头。",
      },
    },
  ],
  g3: [
    {
      img: "calc.png",
      tag: "Calculator",
      en: {
        t: "Math, units and dates",
        d: "3*(5+2)^2, 0xff + 1, 100 mb to gb, 32 f to c, today + 30d, until 2026-10-01. The answer appears above the results, and Enter copies it.",
      },
      zh: {
        t: "算数、换单位、算日子",
        d: "3*(5+2)^2、0xff + 1、100 mb to gb、32 f to c、today + 30d、until 2026-10-01，答案直接出现在结果上方，回车复制。",
      },
    },
    {
      img: "color.png",
      tag: "Generators",
      en: {
        t: "Colors, IDs, passwords",
        d: "#ff6600 shows the color with its rgb and hsl values. The same box generates a uuid, converts ts 1700000000 to a date, makes a password with pwd 24, and encodes or decodes b64 and URLs.",
      },
      zh: {
        t: "颜色、UUID、密码",
        d: "输入 #ff6600，色块和 rgb、hsl 一并给出。同一个框还能生成 uuid、把 ts 1700000000 换算成日期、用 pwd 24 生成密码，b64 和网址的编码解码也不在话下。",
      },
    },
    {
      img: "bang.png",
      tag: "Web shortcuts",
      en: {
        t: "Web search shortcuts",
        d: "gh magpie searches GitHub, g … Google, bd … Baidu. The prefixes can be edited in settings, and you can add your own, for example fy for a translator.",
      },
      zh: {
        t: "直达网站搜索",
        d: "gh magpie 去 GitHub 搜，g … 走 Google，bd … 走百度。前缀可以在设置里改，也可以自己添，比如配一个 fy 直通翻译。",
      },
    },
    {
      img: "emoji.png",
      tag: "Emoji",
      en: {
        t: "Emoji search",
        d: "Type : and a keyword, such as :fire or :火. Click an emoji or press Enter to copy it.",
      },
      zh: {
        t: "找表情",
        d: "输入 : 加关键词，比如 :fire 或 :火，点一下或回车即复制。",
      },
    },
  ],
  g4: [
    {
      img: "tips.png",
      tag: "Tips",
      en: {
        t: "Tips on the empty box",
        d: "When the search box is empty, a line below it shows a shortcut or a feature you may not have tried yet. It changes every few seconds.",
      },
      zh: {
        t: "边用边学",
        d: "搜索框空着时，下方会显示一行小提示：一个快捷键，或一个多半还没发现的功能，隔几秒换一条。",
      },
    },
  ],
  // Chapter 05: the index, handed to AI assistants. The visual is a terminal
  // transcript (`code`, see site.js visualOf) rather than a screenshot: the
  // feature is a command, and the real output of it.
  g6: [
    {
      id: "mcp",
      tag: "MCP",
      code: [
        "$ claude mcp add --transport http magpie http://127.0.0.1:53928/mcp \\",
        "    --header \"Authorization: Bearer ••••••••••••••••\"",
        "$ claude mcp list",
        "magpie: http://127.0.0.1:53928/mcp (HTTP) - ✔ Connected",
      ],
      en: {
        t: "MCP server",
        d: "Turn it on in settings and paste one command. Claude Code, Cursor and other MCP clients then get three read-only tools: search every source with the same ranking as the palette, read a file's indexed text, and list what you opened recently. It listens on localhost only, requires a token, and is off by default.",
      },
      zh: {
        t: "MCP 服务",
        d: "在设置里打开 MCP 服务，粘贴一条命令即可。之后 Claude Code、Cursor 等工具可以搜索 magpie 的全部内容、读取文件的文本、查看最近打开的记录，只读不写。服务只监听本机，需要密钥，默认关闭。",
      },
    },
  ],
  // Chapter 06: everything that doesn't need a screenshot to explain.
  rest: {
    en: [
      ["Keywords and meaning together", "Exact-word matches and matches by meaning are ranked together, so the best of both come first. Long files are indexed in chunks, which means a sentence on page 100 can still be found."],
      ["Ranking that adapts", "Results you open move up over time. The counts are kept locally, fade with time, and never outrank a better match."],
      ["Every row has a copy", "<kbd>Ctrl+C</kbd> copies what the row refers to: a path, a URL, or the text of a clip. <kbd>Ctrl+Shift+C</kbd> copies the file itself, ready to paste as an attachment."],
      ["Text tools for the clipboard", "Type <code>json</code> by itself and the JSON you just copied comes back formatted. <code>upper</code>, <code>lower</code>, <code>slug</code>, <code>lines</code> and <code>count</code> work the same way. Enter puts the result back on the clipboard."],
      ["Filters for file search", "<code>ext:pdf</code>, <code>>10mb</code>, <code>7d</code>, <code>in:projects</code>. They combine with keywords: <code>invoice ext:pdf 30d</code>."],
      ["Quick notes", "<code>note buy milk</code> appends a timestamped line to a Markdown file of your choice. Enter saves it and closes the window."],
      ["Search the selected text", "Select text in any app and press <kbd>Ctrl+Alt+Space</kbd> (<kbd>Option+Shift+Space</kbd> on a Mac). magpie opens with that text as the query."],
      ["Works over full-screen apps", "On a Mac the window opens on the current Space, including over a full-screen browser or terminal. It lives in the menu bar and has no Dock icon."],
      ["Recent opens on the empty box", "When enabled, each tab lists what you recently opened from it, so a file from a minute ago is two keystrokes away."],
      ["Git worktrees indexed once", "If several worktrees of one project sit inside an indexed folder, their files are read once rather than once per worktree."],
      ["Limited CPU use", "Indexing uses at most four CPU threads per model. You can choose 1, 2, 8 or all cores, and the change applies immediately."],
      ["Changes picked up in seconds", "A file watcher reports changed files and only those are re-read. A full scan also runs on a schedule you set, to catch anything missed."],
      ["Memory used only when needed", "The image model loads only after an image, a video or a copied image has been indexed. If you only index text, it never loads."],
      ["Jump to a tab", "<kbd>Ctrl+1</kbd> to <kbd>Ctrl+9</kbd> (<kbd>⌘</kbd> on a Mac) switch tabs in the order shown. You can change the key to Alt or turn it off in settings."],
      ["Hide on click-out", "Clicking another window hides magpie. This is on by default on a Mac. On Windows and Linux it is off by default, so files can still be dragged in, and it can be turned on in settings."],
      ["App icons", "Apps are shown with the icon your OS uses for them. In the preview, a video shows its shots, or a single frame until it has been indexed."],
      ["Launch at login", "A switch in settings. At login magpie starts in the tray and opens when you press the hotkey."],
      ["Action menu", "<kbd>Ctrl+K</kbd> (<kbd>⌘K</kbd> on a Mac) lists what you can do with a result: open it, show it in its folder, copy the path or the file, run an app as administrator, copy a clone command or a Markdown link, pin a clip, or end a process."],
      ["JSON, hashes, JWT, QR codes", "<code>json</code> keeps key order, <code>json min</code> puts it on one line and <code>json sort</code> sorts the keys; invalid JSON reports where it breaks. There is also <code>sha256</code>, <code>jwt</code> (decoded locally, with the expiry time), <code>snake MyMacCleaner</code> → <code>my_mac_cleaner</code>, and <code>qr</code> for a QR code you can paste as an image."],
      ["PDF to Markdown", "On a PDF, <kbd>Ctrl+K</kbd> copies or saves it as Markdown, page by page, keeping headings, lists and tables. With OCR on, scanned pages are included."],
      ["System commands", "Lock, sleep, restart, shut down, empty the trash, toggle dark mode. <code>lock</code>, <code>锁屏</code> and <code>sp</code> all find the lock screen. Commands that could lose unsaved work need a second Enter."],
      ["End a process", "<code>kill chrome</code> lists running processes with that name and their memory use. Press Enter twice to end one. magpie itself and system processes are never listed."],
      ["Text in a copied image", "Take a screenshot, type <code>ocr</code>, and the text on it comes back ready to copy. It uses the OCR engine, so turn that on in settings first."],
      ["Who is on port 3000", "<code>port 3000</code> or <code>kill :3000</code> lists what is listening on a port. Press Enter twice to end it."],
      ["Open in a terminal or an editor", "<kbd>Ctrl+K</kbd> on a file opens its folder in a terminal, or the file in VS Code, Cursor, Trae, Zed or another installed editor. It can also move the file to the Recycle Bin or Trash after a second Enter."],
      ["Newest downloads", "<code>dl</code> lists the newest files in your Downloads folder, without indexing it, and they work like any other file result. Downloads still in progress are left out."],
      ["Chinese capitals and pinyin", "<code>大写 1234.56</code> gives 壹仟贰佰叁拾肆元伍角陆分 for invoices and contracts. <code>py 重庆</code> gives chóng qìng, with tones."],
      ["Timers", "<code>timer 25m standup</code> starts a countdown, and a notification comes when it ends. <code>timer</code> alone lists the running ones."],
      ["Read a QR code, compare two copies", "With an image on the clipboard, <code>qr</code> reads the code in it. <code>diff</code> shows how the last two texts you copied differ."],
      ["System, chance and escapes", "<code>sys</code> shows CPU, memory and free disk space. <code>random 1 100</code>, <code>pick</code>, <code>dice</code> and <code>coin</code> decide for you. <code>半角</code>, <code>全角</code>, <code>unicode</code> and <code>html</code> convert text."],
      ["Local IP and symbols","<code>ip</code> shows your address on the local network, offline. <code>:arrow</code>, <code>:乘</code> and <code>:rmb</code> find symbols such as →, × and ¥; <code>:sym</code> lists them all."],      ["Time zones, offline", "<code>tokyo time</code> or <code>3pm pst to beijing</code>, with daylight saving time handled. <code>tokyo</code> alone is still a normal search."],
      ["Homebrew on a Mac", "<code>brew install --cask newdee/tap/magpie</code>. The cask is updated for every release after the build has been installed on a real Mac."],
      ["Update checks", "magpie checks at launch and once a day. A new version puts a red dot on the tray icon; installing it is up to you."],
      ["English and 简体中文", "The whole interface is translated, including the tray menu. It follows the system language and can be changed in settings."],
      ["Scanned PDFs", "A separate switch runs OCR on PDF pages that have no text layer. It is off by default because large scans take a while."],
      ["Open in the browser", "<kbd>Ctrl+Enter</kbd> sends the query to your default browser: a URL opens directly, anything else becomes a web search. In the Web tab a faint hint after the query shows the shortcut."],
      ["Settings in pages", "General, Local Files, Web, GitHub Stars, Clipboard and About each have a page, and settings open on the page for the current tab. The Web page lists the browsers that were found."],
      ["Multiple displays", "The window opens on the display the pointer is on."],
      ["Portable settings", "Export all settings to one JSON file and import it on another machine. The GitHub token is not exported."],
      ["Model downloads behind a firewall", "Switch to hf-mirror.com with one click. If neither Hugging Face nor the mirror is reachable, magpie downloads from its own GitHub releases."],
    ],
    zh: [
      ["字面和意思，两头都顾", "既找和输入一字不差的词，也找意思相近的内容，再把两边最好的结果排在前面。长文件分段读取，第 100 页的一句话也漏不掉。"],
      ["越用越顺手", "常打开的结果会慢慢往前挪。这些记录只存在本机，时间一长会自然淡化，也不会压过更匹配的结果。"],
      ["每一行都能复制", "<kbd>Ctrl+C</kbd> 复制这一行对应的路径、网址或文字；<kbd>Ctrl+Shift+C</kbd> 复制文件本身，粘贴出去就是附件。"],
      ["复制的文字，顺手整理", "单独输入 <code>json</code>，刚复制的 JSON 立刻排好版。<code>upper</code>、<code>lower</code>、<code>slug</code>、<code>lines</code>、<code>count</code> 如法炮制，回车把结果放回剪贴板。"],
      ["搜文件时圈定范围", "<code>ext:pdf</code>、<code>>10mb</code>、<code>7d</code>、<code>in:projects</code> 可以和关键词混着用，<code>发票 ext:pdf 30d</code> 就是字面意思。"],
      ["随手记一笔", "<code>note 明天回邮件</code> 会把这一行连同时间写进指定的 Markdown 文件，回车即存，窗口随之收起。"],
      ["划词搜索", "在任何应用里选中文字，按 <kbd>Ctrl+Alt+Space</kbd>（Mac 上是 <kbd>Option+Shift+Space</kbd>），magpie 带着这段文字弹出来。"],
      ["全屏应用上照样弹出", "Mac 上它会出现在当前所在的桌面，全屏的浏览器、终端也不例外。常驻菜单栏，不占 Dock。"],
      ["空搜索框显示最近打开", "开启后，每个标签页都会列出最近从这里打开过的内容。一分钟前的文件，两下按键就回来了。"],
      ["git worktree 不重复收录", "同一个项目在索引目录里开了好几个 worktree，文件只读一遍，不会一式多份。"],
      ["不跟你抢电脑", "建索引时每个模型最多用 4 个 CPU 线程，也可以改成 1、2、8 个或全部核心，改完立即生效。"],
      ["新文件几秒内可搜", "文件夹有监听，哪里改了就只重读哪里；另有定期的全面检查兜底，查漏补缺。"],
      ["用不着的模型不占内存", "只有索引过图片、视频或复制过图片之后，识图模型才加载。只搜文字的话，它从头到尾都不会加载。"],
      ["数字键直达标签页", "<kbd>Ctrl+1</kbd> 到 <kbd>Ctrl+9</kbd>（Mac 上是 <kbd>⌘</kbd>）按标签栏顺序直接跳转，也可以在设置里改成 Alt 或关掉。"],
      ["点别处就收起", "点到别的窗口，magpie 随即隐去。Mac 上默认开启；Windows 和 Linux 默认关闭，方便从别的窗口往里拖文件，需要时可以打开。"],
      ["应用带着自己的图标", "应用显示系统里它自己的图标。预览面板里，视频显示各个镜头，还没处理到的先显示一帧画面。"],
      ["开机启动", "设置里一个开关。开机后只在托盘留个图标，要用时按快捷键呼出。"],
      ["每个结果都有操作菜单", "<kbd>Ctrl+K</kbd>（Mac 上是 <kbd>⌘K</kbd>）列出结果能做的事：打开、在文件夹中显示、复制路径或文件、以管理员身份运行、复制 clone 命令或 Markdown 链接、钉住剪贴内容、结束进程。"],
      ["JSON、哈希、JWT、二维码", "<code>json</code> 保留键的原有顺序，<code>json min</code> 压成一行，<code>json sort</code> 按键排序，写错了会指出错在哪。另有 <code>sha256</code>、<code>jwt</code>（本机解码，显示过期时间）、<code>snake MyMacCleaner</code> → <code>my_mac_cleaner</code>，以及可以粘贴成图片的 <code>qr</code> 二维码。"],
      ["PDF 转 Markdown", "在 PDF 上按 <kbd>Ctrl+K</kbd>，复制或保存为 Markdown：逐页按顺序，标题、列表、表格原样保留。开启 OCR 时，扫描页也能读出来。"],
      ["系统命令", "锁屏、睡眠、重启、关机、清空废纸篓、切换深色模式。<code>lock</code>、<code>锁屏</code>、<code>sp</code> 都能找到锁屏；可能丢数据的操作要按两次回车，免得手快误触。"],
      ["结束卡死的进程", "<code>kill chrome</code> 按名字列出正在运行的进程和内存占用，按两次回车结束。magpie 自己和系统进程不在其列。"],
      ["截图取字", "截个图，输入 <code>ocr</code>，图上的字就出来了，回车复制。用的是 OCR 引擎，要先在设置里打开。"],
      ["谁占了 3000 端口", "<code>port 3000</code> 或 <code>kill :3000</code> 列出正在监听这个端口的进程，按两次回车结束。"],
      ["在终端或编辑器里打开", "在文件上按 <kbd>Ctrl+K</kbd>，可以在终端里打开它所在的文件夹，或者用 VS Code、Cursor、Trae、Zed 等已安装的编辑器打开；也能把文件移到回收站，按两次回车确认。"],
      ["最近下载", "<code>dl</code> 列出「下载」文件夹里最新的文件，不用建索引，用起来和其他文件结果一样。还没下完的不会列出来。"],
      ["金额大写和拼音", "<code>大写 1234.56</code> 得到「壹仟贰佰叁拾肆元伍角陆分」，报销、写合同正用得上。<code>py 重庆</code> 得到 chóng qìng，带声调。"],
      ["倒计时", "<code>timer 25m 开会</code> 开始倒计时，时间一到就弹出通知。单独输入 <code>timer</code> 看看还剩多少。"],
      ["识别二维码，对比两次复制", "剪贴板里是图片时，<code>qr</code> 读出图中的二维码。<code>diff</code> 逐行显示最近两次复制的文字差在哪里。"],
      ["系统、随机和转义", "<code>sys</code> 看 CPU、内存和磁盘剩余空间。<code>random 1 100</code>、<code>pick</code>、<code>dice</code>、<code>coin</code> 帮你拿主意。<code>半角</code>、<code>全角</code>、<code>unicode</code>、<code>html</code> 转换文字。"],
      ["本机 IP 和特殊符号","<code>ip</code> 显示本机的局域网地址，不联网也行。<code>:arrow</code>、<code>:乘</code>、<code>:rmb</code> 能找到 →、×、¥ 这类符号，<code>:sym</code> 列出全部。"],      ["时区换算，离线可用", "<code>东京时间</code> 或 <code>3pm pst to beijing</code>，夏令时自动计算。单独输入 <code>tokyo</code> 仍按普通搜索处理。"],
      ["Mac 上用 Homebrew 安装", "<code>brew install --cask newdee/tap/magpie</code>。每次发版 cask 都会跟进，且先在真机 Mac 上装过一遍再提交。"],
      ["自动检查更新", "启动时和之后每天各检查一次。有新版本时托盘图标上会出现红点，装不装、何时装由你定。"],
      ["English 和简体中文", "整个界面连托盘菜单都已翻译，默认跟随系统语言，设置里可以切换。"],
      ["扫描版 PDF", "单独一个开关，专门读取没有文字层、只是图片的 PDF 页面。默认关闭，大部头的扫描件比较费时。"],
      ["直接交给浏览器", "<kbd>Ctrl+Enter</kbd> 把输入内容交给默认浏览器：网址直接打开，其他内容变成网页搜索。在 Web 标签页里，输入的文字后面会有一行浅灰色提示。"],
      ["设置分页", "通用、本地文件、Web、GitHub Stars、剪贴板、关于，各占一页；打开设置时直接落在当前标签页对应的那一页。Web 页还列出了已发现的浏览器。"],
      ["多显示器", "鼠标在哪块屏幕上，magpie 就在哪块屏幕上出现。"],
      ["设置随身带", "全部设置可以导出成一个 JSON 文件，换台电脑导入即可。GitHub token 不会随之导出。"],
      ["模型下载，不怕网络卡脖子", "一键切到 hf-mirror.com。Hugging Face 和镜像都连不上时，改从 magpie 自己的 GitHub 发布页下载。"],
    ],
  },
  g5: {
    en: [
      "<b>No full-disk scanning.</b> magpie reads only the folders you add. It respects .gitignore, skips hidden files, and does not follow symlinks out of those folders.",
      "<b>Nothing leaves your machine.</b> The index is a single SQLite file in your user profile. The models run locally and work offline once downloaded.",
      "<b>Queries are not logged.</b> The log records errors and model status, enough for a useful bug report. What you typed is not in it.",
      "<b>Secrets are not stored.</b> Clips marked confidential by your password manager are never saved, and exported settings leave out the GitHub token.",
      "<b>Signed and notarized.</b> macOS builds carry a Developer ID signature and Apple notarization. Updates are installed in place after their signature is verified.",
    ],
    zh: [
      "<b>不做全盘扫描。</b>只读取你添加的文件夹，遵守 .gitignore，跳过隐藏文件，也不会顺着符号链接走出文件夹。",
      "<b>数据不出本机。</b>索引就是用户目录下的一个 SQLite 文件。模型在本机运行，下载完成后离线可用。",
      "<b>搜索内容不入日志。</b>日志只记录错误和模型状态，提 bug 时够用；你输入过什么，日志里一个字也没有。",
      "<b>机密不留痕。</b>密码管理器标为机密的剪贴内容一律不存；导出设置时也不带 GitHub token。",
      "<b>签名、公证俱全。</b>Mac 版带开发者签名并经苹果公证，更新前先校验签名，再原地安装。",
    ],
  },
};
