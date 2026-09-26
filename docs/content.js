// All page copy, in both languages. One source of truth so the two versions
// can never drift apart: the page swaps text nodes, it never reloads.
window.CONTENT = {
  en: {
    "nav.tour": "Tour",
    "hero.eyebrow": "Runs on your computer · Open source",
    "stat.sources": "places to search",
    "stat.local": "stays on your machine",
    "stat.embed": "to understand a search",
    "stat.platforms": "systems",
    "nav.download": "Download",
    "hero.title": "Everything you saved.<br />One keystroke.",
    "hero.lede":
      "Files, screenshots, videos, GitHub stars, bookmarks, things you copied. Type the bit you still remember and magpie finds the rest. Nothing leaves your computer.",
    "hero.download": "Download",
    "hero.source": "View source",
    "hero.meta": "Free · MIT license · Windows / macOS / Linux",
    "g1.title": "What it searches",
    "g1.sub": "Six places, one search box. Press <kbd>Tab</kbd> to hop between them.",
    "g2.title": "It knows what's inside",
    "g2.sub":
      "Not just file names. magpie looks at your pictures and videos, and reads the words in them.",
    "g3.title": "Handy tools in the search box",
    "g3.sub": "Sometimes you just want an answer. Type it, press <kbd>Enter</kbd>, and it's copied.",
    "g4.title": "Nice to use every day",
    "g4.sub": "The little things that make you keep using it after the first week.",
    "g6.title": "Your AI assistant can use it too",
    "g6.sub": "Let Claude Code, Cursor or any other MCP app search what magpie knows. It stays off until you turn it on.",
    "rest.title": "More it can do",
    "rest.sub": "Smaller things you'll bump into once magpie is part of your day.",
    "g5.title": "Privacy you can check for yourself",
    "stars.title": "Star history",
    "stars.cta": "Add yours",
    "close.title": "Press a key. Find the thing.",
    "close.docs": "Full documentation",
    "footer.built": "Built with Rust + Tauri.",
  },
  zh: {
    "nav.tour": "功能",
    "hero.eyebrow": "跑在你自己电脑上 · 开源",
    "stat.sources": "个地方能搜",
    "stat.local": "留在你电脑上",
    "stat.embed": "看懂一次搜索",
    "stat.platforms": "个系统",
    "nav.download": "下载",
    "hero.title": "你存过的东西，<br />按一下就找回来。",
    "hero.lede":
      "文件、截图、视频、GitHub star、书签、复制过的内容，都能搜。记不清全名没关系，输入你还记得的那点就行。所有东西都留在你自己的电脑上。",
    "hero.download": "下载",
    "hero.source": "查看源码",
    "hero.meta": "免费 · MIT 许可 · Windows / macOS / Linux",
    "g1.title": "能搜什么",
    "g1.sub": "六个地方，一个搜索框。按 <kbd>Tab</kbd> 来回切。",
    "g2.title": "看得懂里面的内容",
    "g2.sub": "不只是看文件名。图片和视频里有什么、上面写了什么字，magpie 都认得。",
    "g3.title": "搜索框里的小工具",
    "g3.sub": "有时候你只想要个答案。输进去，按 <kbd>Enter</kbd>，就复制好了。",
    "g4.title": "天天用也顺手",
    "g4.sub": "让你用过第一周还想接着用的那些小细节。",
    "g6.title": "你的 AI 助手也能用",
    "g6.sub": "让 Claude Code、Cursor 或其他支持 MCP 的应用，也能搜 magpie 里的东西。你不打开，它就一直关着。",
    "rest.title": "还能做这些",
    "rest.sub": "用久了你会慢慢碰到的小功能。",
    "g5.title": "隐私，你可以自己查",
    "stars.title": "Star 增长",
    "stars.cta": "来一颗",
    "close.title": "按一下，就找到。",
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
        t: "Your files, and what's in them",
        d: "Add the folders you actually use. magpie finds any file by its name, and for about 80 kinds of text files, PDFs and Office documents, by what's written inside too. The matching line is highlighted.",
      },
      zh: {
        t: "你的文件，连同里面写的内容",
        d: "把你常用的文件夹加进来。任何文件都能按名字找到；大约 80 种文本文件、PDF 和 Office 文档，还能按里面写的内容找，搜到的那一行会高亮。",
      },
    },
    {
      img: "apps.png",
      tag: "App launcher",
      en: {
        t: "Open apps, too",
        d: "Type part of an app's name and press Enter. The start of a word works, so does the middle, and so do initials like vsc. Chinese app names work with pinyin, and you can give apps your own nicknames.",
      },
      zh: {
        t: "顺便打开应用",
        d: "输入应用名的一部分，回车就打开。开头几个字、中间几个字、首字母缩写（比如 vsc）都行。中文应用名可以用拼音搜，还能给应用起你自己习惯的别名。",
      },
    },
    {
      img: "stars.png",
      tag: "GitHub stars",
      en: {
        t: "GitHub stars you forgot about",
        d: "Every repo you've starred, READMEs included. Even a sentence buried deep in a README is enough to find the repo again. Sort by how well it matches, when you starred it, or how many stars it has.",
      },
      zh: {
        t: "被你遗忘的 GitHub star",
        d: "你 star 过的所有项目，连 README 一起收进来。哪怕只记得 README 深处的一句话，也能把项目找回来。可以按匹配程度、star 的时间或星数排序。",
      },
    },
    {
      img: "web.png",
      tag: "Bookmarks + history",
      en: {
        t: "Bookmarks and browsing history",
        d: "magpie reads them straight from your browsers: Chrome, Edge and every other Chromium browser, plus Firefox and its relatives like LibreWolf, Zen and Floorp, every profile. A page saved in several browsers shows up once, with each browser's icon, and pages you open often come first.",
      },
      zh: {
        t: "书签和浏览记录",
        d: "直接从你的浏览器里读：Chrome、Edge 等 Chromium 内核的浏览器，还有 Firefox 以及 LibreWolf、Zen、Floorp 这些 Firefox 内核的浏览器，所有 profile 都算。同一个网页存在好几个浏览器里的话只出现一次，后面带着各浏览器的图标；你常打开的网页排在前面。",
      },
    },
    {
      img: "clipboard.png",
      tag: "Clipboard",
      en: {
        t: "Things you copied",
        d: "Off until you turn it on, and kept only on your computer. Copy an old item again, paste it straight into the app you were in, or pin the ones you use every day. Anything your password manager marks as secret is never saved.",
      },
      zh: {
        t: "复制过的东西",
        d: "默认是关着的，打开之后也只存在你自己电脑上。可以把以前复制的内容再复制一次、直接粘贴回刚才的应用，或者把天天要用的钉住。密码管理器标记为机密的内容，一律不会保存。",
      },
    },
    {
      img: "clip-image.png",
      tag: "Copied images",
      en: {
        t: "Copied screenshots, too",
        d: "Images you copy are kept with a little preview. Later, just describe what was on the screen to find one, the same way you search your picture files.",
      },
      zh: {
        t: "复制的截图也在",
        d: "复制过的图片会连着小预览图一起记下来。之后描述一下当时屏幕上有什么，就能找回来，和搜图片文件一样。",
      },
    },
    {
      img: "videos.png",
      tag: "Video scenes",
      en: {
        t: "Find a moment in a video",
        d: "magpie splits your videos into scenes. Describe what you remember seeing, or drop in a picture, and it takes you to that moment. Press Enter and the video starts playing right there.",
      },
      zh: {
        t: "找到视频里的某个画面",
        d: "magpie 会把视频分成一个个镜头。描述一下你记得的画面，或者拖一张图进来，就能跳到那一刻。回车，视频直接从那里开始播。",
      },
    },
  ],
  g2: [
    {
      img: "images.png",
      tag: "Images",
      en: {
        t: "Search pictures by what's in them",
        d: "Describe a picture in your own words, in any language, or drop one in to find others that look like it. The image model runs on your own computer.",
      },
      zh: {
        t: "按画面内容找图",
        d: "用自己的话描述一张图，什么语言都行；或者拖一张图进来，找长得像的。看图用的模型就跑在你自己电脑上。",
      },
    },
    {
      img: "ocr.png",
      tag: "Text in images",
      en: {
        t: "The words in your screenshots",
        d: "Turn on text recognition (OCR) in settings and magpie reads the words in your pictures, Chinese and English. That error message you screenshotted three weeks ago? One search away.",
      },
      zh: {
        t: "截图里的字",
        d: "在设置里打开文字识别（OCR），magpie 就会读出图片里的字，中英文都行。三周前截图里那条报错？搜一下就回来了。",
      },
    },
    {
      img: "ocr-video.png",
      tag: "Text in videos",
      en: {
        t: "…and in your videos",
        d: "It reads the words in videos too: subtitles, slides, code on someone's shared screen. Open a result and the video starts where those words appear.",
      },
      zh: {
        t: "……视频里的字也行",
        d: "视频里的字也能读：字幕、幻灯片、别人共享屏幕上的代码，都能搜到。点开结果，视频就从那段字出现的地方开始播。",
      },
    },
    {
      img: "crosslang.png",
      tag: "Any language",
      en: {
        t: "Search in one language, find another",
        d: "Search in Chinese and English READMEs still turn up, and the other way round. It understands 100+ languages, so you don't have to remember which one you read something in.",
      },
      zh: {
        t: "中文搜，英文也能找到",
        d: "用中文搜，英文的 README 照样找得到，反过来也一样。它懂 100 多种语言，你不用记得当初看的是哪种语言。",
      },
    },
    {
      img: "preview.png",
      tag: "Preview",
      en: {
        t: "Take a look before you open",
        d: "Press <kbd>→</kbd> to see a preview next to the list: a file's text with your words highlighted, a picture at full size, the scenes of a video, the start of a repo's README.",
      },
      zh: {
        t: "打开之前先看一眼",
        d: "按 <kbd>→</kbd> 在列表旁边看预览：文件正文里你搜的词会高亮，图片看大图，视频看各个镜头，项目看 README 开头。",
      },
    },
  ],
  g3: [
    {
      img: "calc.png",
      tag: "Calculator",
      en: {
        t: "Math, units and dates",
        d: "3*(5+2)^2, 0xff + 1, 100 mb to gb, 32 f to c, today + 30d, until 2026-10-01. The answer shows up right above your results, and Enter copies it.",
      },
      zh: {
        t: "算数、换算单位、算日期",
        d: "3*(5+2)^2、0xff + 1、100 mb to gb、32 f to c、today + 30d、until 2026-10-01。答案就显示在结果上面，回车就复制。",
      },
    },
    {
      img: "color.png",
      tag: "Handy bits",
      en: {
        t: "Colors, IDs, passwords",
        d: "Type #ff6600 to see the color and its rgb and hsl values. The same box makes a uuid, turns ts 1700000000 into a date, gives you a pwd 24, and encodes or decodes b64 and URLs.",
      },
      zh: {
        t: "颜色、ID、密码",
        d: "输入 #ff6600 就能看到颜色和它的 rgb、hsl 值。同一个框还能生成 uuid、把 ts 1700000000 换成日期、用 pwd 24 生成密码，以及 b64 和网址的编码、解码。",
      },
    },
    {
      img: "bang.png",
      tag: "Web shortcuts",
      en: {
        t: "Search a website directly",
        d: "gh magpie searches GitHub, g … searches Google, bd … searches Baidu. Change these shortcuts in settings, or add your own, like fy for a translator.",
      },
      zh: {
        t: "直接搜某个网站",
        d: "gh magpie 去 GitHub 搜，g … 用 Google 搜，bd … 用百度搜。这些前缀可以在设置里改，也可以自己加，比如加个 fy 直接去翻译。",
      },
    },
    {
      img: "emoji.png",
      tag: "Emoji",
      en: {
        t: "Find an emoji",
        d: "Type : and a word, like :fire or :火. Click one or press Enter to copy it.",
      },
      zh: {
        t: "找表情",
        d: "输入 : 加一个词，比如 :fire 或者 :火。点一下或者回车就复制。",
      },
    },
  ],
  g4: [
    {
      img: "tips.png",
      tag: "Tips",
      en: {
        t: "It shows you around",
        d: "When the search box is empty, a one-line tip shows a shortcut or something you probably haven't found yet. A new one comes up every few seconds.",
      },
      zh: {
        t: "自己教你怎么用",
        d: "搜索框空着的时候，下面会有一行小提示，教你一个快捷键，或者一个你多半还没发现的功能。过几秒换一条。",
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
        t: "Your AI can search it too",
        d: "Turn on the MCP server in settings and paste one command. Claude Code, Cursor and other MCP apps can then search everything magpie knows, read a file's text, and see what you opened lately. They can look, but never change anything. It only listens on your own computer, needs a key, and is off by default.",
      },
      zh: {
        t: "你的 AI 也能搜",
        d: "在设置里打开 MCP 服务，粘贴一条命令。之后 Claude Code、Cursor 这类支持 MCP 的应用就能搜 magpie 里的所有东西、读某个文件的文字、看你最近打开过什么。它们只能看，改不了任何东西。只在你自己电脑上监听，需要密钥，默认关闭。",
      },
    },
  ],
  // Chapter 06: everything that doesn't need a screenshot to explain.
  rest: {
    en: [
      ["Finds it by words and by meaning", "It looks for the exact words you typed and for things that mean the same, then puts the best of both on top. Long files are read in pieces, so a sentence on page 100 still turns up."],
      ["Learns what you like", "Results you actually open slowly move up. This stays on your computer, fades over time, and never pushes a better match down."],
      ["Every row has a copy", "<kbd>Ctrl+C</kbd> copies what the row points to: a path, a web address, the text of a clip. <kbd>Ctrl+Shift+C</kbd> copies the file itself, so you can paste it as an attachment."],
      ["Quick fixes for copied text", "Type <code>json</code> on its own and the JSON you just copied comes back neatly formatted. <code>upper</code>, <code>lower</code>, <code>slug</code>, <code>lines</code> and <code>count</code> work the same way. Enter puts the result back on your clipboard."],
      ["Narrow down a file search", "<code>ext:pdf</code>, <code>>10mb</code>, <code>7d</code>, <code>in:projects</code>. Mix them with words: <code>invoice ext:pdf 30d</code> means just what it says."],
      ["Quick notes", "<code>note buy milk</code> adds one line with the time to a Markdown file you pick. Press Enter and it's saved, and magpie gets out of your way."],
      ["Look up what you selected", "Select some text in any app and press <kbd>Ctrl+Alt+Space</kbd> (<kbd>Option+Shift+Space</kbd> on a Mac). magpie opens with it already typed in."],
      ["Works over full-screen apps", "On a Mac, magpie opens right where you are, even over a full-screen browser or terminal. It sits in the menu bar, not the Dock."],
      ["Recent opens on the empty box", "Turn it on and each tab shows what you opened from it lately. Getting back to a file from a minute ago takes two keys."],
      ["Git worktrees counted once", "If you have several worktrees of the same project in one indexed folder, magpie reads the files once instead of once per worktree."],
      ["Doesn't hog your computer", "While indexing, it uses at most four CPU threads per model. You can pick 1, 2, 8 or all of them, and the change applies right away."],
      ["New files show up in seconds", "A watcher notices what changed in your folders and reads just that. A full check still runs every so often, in case something slipped by."],
      ["Uses memory only when needed", "The image model loads only after you've indexed a picture, a video or a copied image. If you only search text, it never loads."],
      ["Jump to any tab", "<kbd>Ctrl+1</kbd> to <kbd>Ctrl+9</kbd> (<kbd>⌘</kbd> on a Mac) jump straight to a tab, in the order they're shown. You can switch this to Alt or turn it off in settings."],
      ["Hides when you click away", "Click another window and magpie steps aside. This is on by default on a Mac. On Windows and Linux it's off by default so you can still drag files in, but you can turn it on."],
      ["Apps look like apps", "Apps show the same icon your OS gives them. In the preview, a video shows its scenes, or a single frame until magpie has gone through it."],
      ["Launch at login", "One switch in settings. magpie then waits quietly in the tray until you need it."],
      ["A menu for every result", "<kbd>Ctrl+K</kbd> (<kbd>⌘K</kbd> on a Mac) shows what you can do with a result: open it, show it in its folder, copy its path or the file, run an app as administrator, copy a clone command or a Markdown link, pin a clip, or close a program."],
      ["JSON, hashes, JWT, QR codes", "<code>json</code> keeps the keys in order, <code>json min</code> squeezes it onto one line and <code>json sort</code> sorts it. Broken JSON tells you where it breaks. Also <code>sha256</code>, <code>jwt</code> (decoded on your computer, with when it expires), <code>snake MyMacCleaner</code> → <code>my_mac_cleaner</code>, and <code>qr</code> for a QR code you can paste as a picture."],
      ["PDF to Markdown", "On a PDF, press <kbd>Ctrl+K</kbd> to copy or save it as Markdown, every page in order, with headings, lists and tables kept. With OCR on, scanned pages are read too."],
      ["Control your computer", "Lock, sleep, restart, shut down, empty the trash, switch dark mode. <code>lock</code>, <code>锁屏</code> and <code>sp</code> all find the lock screen. Anything that could lose your work asks you to press Enter twice."],
      ["Close a stuck program", "<code>kill chrome</code> lists what's running under that name and how much memory it uses. Press Enter twice to close it. magpie and system processes are never on the list."],
      ["Time zones, even offline", "<code>tokyo time</code> or <code>3pm pst to beijing</code>, with daylight saving handled for you. Just <code>tokyo</code> on its own is still a normal search."],
      ["Homebrew on a Mac", "<code>brew install --cask newdee/tap/magpie</code>. It keeps up with every release, and each one is installed on a real Mac first."],
      ["Updates come to you", "magpie checks when it starts and once a day. When there's a new version the tray icon gets a red dot, and you decide when to install it."],
      ["English and 简体中文", "The whole app, tray menu included. It follows your system language, and you can change it in settings."],
      ["Scanned PDFs", "A separate switch reads PDF pages that are just pictures of text. It's off by default because big scans take a while."],
      ["Straight to your browser", "<kbd>Ctrl+Enter</kbd> sends what you typed to your browser. A web address opens, anything else becomes a web search. In Web, a small hint after your text reminds you."],
      ["Settings in pages", "General, Local Files, Web, GitHub Stars, Clipboard and About each have their own page, and settings open on the one for the tab you're in. The Web page lists the browsers magpie found."],
      ["Multiple screens", "magpie shows up on whichever screen your mouse is on."],
      ["Take your settings with you", "Save all your settings to one file and load it on another computer. Your GitHub token stays behind."],
      ["Models download anywhere", "Switch to hf-mirror.com with one click. If neither Hugging Face nor the mirror can be reached, magpie downloads from its own GitHub releases instead."],
    ],
    zh: [
      ["既按字找，也按意思找", "既找和你输入一模一样的词，也找意思相近的内容，再把两边最好的结果排到前面。长文件会分段读，第 100 页的一句话也能搜到。"],
      ["越用越懂你", "你常打开的结果会慢慢往前排。这些记录只在你电脑上，时间久了会淡掉，也不会把更匹配的结果挤下去。"],
      ["每一行都能复制", "<kbd>Ctrl+C</kbd> 复制这一行指向的东西：路径、网址或复制过的文字。<kbd>Ctrl+Shift+C</kbd> 复制文件本身，粘贴出去就是附件。"],
      ["顺手整理复制的文字", "单独输入 <code>json</code>，刚复制的 JSON 就排好版了。<code>upper</code>、<code>lower</code>、<code>slug</code>、<code>lines</code>、<code>count</code> 也是一样的用法，回车把结果放回剪贴板。"],
      ["搜文件时缩小范围", "<code>ext:pdf</code>、<code>>10mb</code>、<code>7d</code>、<code>in:projects</code>。可以和关键词一起用，<code>发票 ext:pdf 30d</code> 就是字面意思。"],
      ["随手记一笔", "<code>note 明天回邮件</code> 会把这一行连同时间，记到你指定的 Markdown 文件里。回车就存好，窗口随即收起。"],
      ["划词搜索", "在任何应用里选中一段文字，按 <kbd>Ctrl+Alt+Space</kbd>（Mac 上是 <kbd>Option+Shift+Space</kbd>），magpie 会带着这段文字弹出来。"],
      ["全屏应用上也能用", "在 Mac 上，magpie 会直接出现在你当前的桌面上，就算是全屏的浏览器或终端也一样。它待在菜单栏里，不占 Dock。"],
      ["空搜索框显示最近打开", "打开后，每个标签页都会列出你最近从这里打开过的东西。回到一分钟前那个文件，按两下就行。"],
      ["git worktree 只算一次", "同一个项目在索引的文件夹里有好几个 worktree 的话，magpie 只读一遍文件，不会每个 worktree 都读一遍。"],
      ["不会把电脑占满", "建索引的时候，每个模型最多用 4 个 CPU 线程。也可以选 1、2、8 个或者全部，改完马上生效。"],
      ["新文件几秒就能搜到", "magpie 会监听你的文件夹，哪里变了就只重读哪里。另外还会隔一段时间全面检查一遍，以防漏掉。"],
      ["用到时才占内存", "只有在索引过图片、视频或复制过的图片之后，看图用的模型才加载。如果你只搜文字，它根本不会加载。"],
      ["数字键直达标签页", "<kbd>Ctrl+1</kbd> 到 <kbd>Ctrl+9</kbd>（Mac 上是 <kbd>⌘</kbd>）直接跳到对应的标签页，顺序和标签栏一样。也可以在设置里改成 Alt，或者关掉。"],
      ["点别处就收起", "点到别的窗口，magpie 就自动让开。Mac 上默认打开；Windows 和 Linux 上默认关着，方便从别的窗口往里拖文件，想要的话也可以打开。"],
      ["应用带着自己的图标", "应用会显示系统里它自己的图标。预览里，视频会显示各个镜头；还没处理到的视频先显示一帧画面。"],
      ["开机启动", "设置里一个开关。打开后 magpie 会安静地待在托盘里，要用的时候再叫它。"],
      ["每个结果都有操作菜单", "<kbd>Ctrl+K</kbd>（Mac 上是 <kbd>⌘K</kbd>）看看这个结果能做什么：打开、在文件夹里显示、复制路径或文件、以管理员身份运行应用、复制 clone 命令或 Markdown 链接、钉住复制过的内容、关掉程序。"],
      ["JSON、哈希、JWT、二维码", "<code>json</code> 保持键的原有顺序，<code>json min</code> 压成一行，<code>json sort</code> 按键排序，JSON 写错了会告诉你错在哪。还有 <code>sha256</code>、<code>jwt</code>（在你电脑上解码，并显示什么时候过期）、<code>snake MyMacCleaner</code> → <code>my_mac_cleaner</code>，以及能直接粘贴成图片的二维码 <code>qr</code>。"],
      ["PDF 转 Markdown", "在 PDF 上按 <kbd>Ctrl+K</kbd>，就能复制或保存成 Markdown：每一页按顺序，标题、列表、表格都保留。打开 OCR 的话，扫描页也能读出来。"],
      ["管理你的电脑", "锁屏、睡眠、重启、关机、清空废纸篓、切换深色模式。<code>lock</code>、<code>锁屏</code>、<code>sp</code> 都能找到锁屏。可能让你丢东西的操作，要按两次回车才执行。"],
      ["关掉卡住的程序", "<code>kill chrome</code> 会列出叫这个名字、正在运行的程序和它们占的内存，按两次回车就关掉。magpie 自己和系统进程不会出现在列表里。"],
      ["时区换算，不联网也能用", "<code>东京时间</code> 或 <code>3pm pst to beijing</code>，夏令时会自动算好。单独输入 <code>tokyo</code> 还是普通搜索。"],
      ["Mac 上用 Homebrew 装", "<code>brew install --cask newdee/tap/magpie</code>。每次发新版都会跟着更新，而且每个版本都先在真的 Mac 上装过一遍。"],
      ["更新会主动找你", "magpie 启动时检查一次，之后每天检查一次。有新版本时托盘图标会亮一个红点，什么时候装由你决定。"],
      ["English 和简体中文", "整个应用连托盘菜单都有。默认跟着系统语言，也可以在设置里改。"],
      ["扫描版 PDF", "单独一个开关，用来读那些其实是图片的 PDF 页面。默认关着，因为大的扫描件比较花时间。"],
      ["直接交给浏览器", "<kbd>Ctrl+Enter</kbd> 把你输入的内容交给浏览器：是网址就打开，不是就变成网页搜索。在 Web 标签页里，输入的文字后面会有一行小提示提醒你。"],
      ["设置分页", "通用、本地文件、Web、GitHub Stars、剪贴板、关于，各有各的页面；打开设置时会直接进入当前标签页对应的那一页。Web 页还会列出 magpie 找到了哪些浏览器。"],
      ["多个显示器", "你的鼠标在哪块屏幕上，magpie 就出现在哪块屏幕上。"],
      ["设置跟你走", "把所有设置存成一个文件，到另一台电脑上导入就行。GitHub token 不会跟着一起导出。"],
      ["模型在哪都能下", "一键切到 hf-mirror.com。要是 Hugging Face 和镜像都连不上，magpie 会改从自己的 GitHub 发布页下载。"],
    ],
  },
  g5: {
    en: [
      "<b>It doesn't scan your whole disk.</b> magpie only reads the folders you add yourself. It follows .gitignore, skips hidden files, and won't wander out of your folder through a shortcut link.",
      "<b>Nothing leaves your computer.</b> Everything magpie learns is kept in one file in your user folder. The models run on your own computer and work offline once downloaded.",
      "<b>What you search is never written down.</b> The log only keeps errors and whether the models are ready, so a bug report can be useful. What you typed isn't in it.",
      "<b>Secrets stay out.</b> Anything your password manager marks as secret is never saved from the clipboard, and exporting your settings leaves your GitHub token behind.",
      "<b>Signed and checked by Apple.</b> The Mac version is signed by the developer and notarized by Apple. Updates install in place, and their signature is checked first.",
    ],
    zh: [
      "<b>不会扫你整个硬盘。</b>magpie 只读你自己加进来的文件夹。它遵守 .gitignore，跳过隐藏文件，也不会顺着快捷方式跑到文件夹外面去。",
      "<b>东西不会离开你的电脑。</b>magpie 记下的所有东西，都放在你用户目录里的一个文件里。模型在你自己电脑上跑，下载好之后断网也能用。",
      "<b>你搜了什么，不会被记下来。</b>日志里只有出错信息和模型是否就绪，方便你提 bug 时有用。你输入过什么，里面没有。",
      "<b>机密不会留下。</b>密码管理器标记为机密的内容，不会存进剪贴板记录；导出设置时，也不会带上你的 GitHub token。",
      "<b>有签名，也经过苹果检查。</b>Mac 版有开发者签名，也通过了苹果的公证。更新时会先核对签名，再原地安装。",
    ],
  },
};
