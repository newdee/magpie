# 验收记录 2026-08-28（本地日志，拟 v0.1.19）

此前全应用只有 5 处 eprintln 打 stderr——Windows 发布版无控制台，用户机器上
一行日志不留，报 issue 拿不到现场。本次：
1. tauri-plugin-log（log facade 后端，弃 tracing：桌面取证场景无调用链需求，
   core 保持干依赖 log 宏，未来可平滑迁 tracing-subscriber）。Info 级，
   LogDir 单文件轮转（2MB KeepOne）+ Stdout（dev）。插件挂 Builder 链最前。
2. 5 处 eprintln 全部换 log::warn!/error!；新增 info 节点：启动版本行、
   语义模型 ready/失败、ffmpeg 解析档位、更新可用。搜索词永不入日志（隐私）。
3. 设置系统区「日志」行 + 「打开日志文件夹」按钮（open_log_dir 命令，
   app_log_dir + opener，与 open_file 同模式）。i18n 3 键。

三轮：53/53 ×2 复跑一致；clippy 0；tsc+build 过。真机 E2E：debug 版启动
8 秒杀掉，%LOCALAPPDATA%\com.dfine.magpie\logs\magpie.log 从无到有，含
"[INFO] magpie v0.1.18 starting" + "semantic model ready"（带时间戳/target/
级别，UTC）。浏览器 demo：设置区日志行 + 「打开日志文件夹」按钮 + 中文
描述渲染正确，位于设置文件行上方。

---
# 验收记录 2026-08-28（更新红点：定时检查 + 托盘/界面提示，拟 v0.1.19）

此前更新只在启动后 15 秒静默查一次，且唯一提示藏在设置面板里——常驻不重启
就永远不知道有新版。本次：
1. 定时检查：启动 15 秒后首查，常驻期间每 24 小时静默再查；下载安装中跳过
   静默检查（不打断进行中的安装 UI）。
2. 提示三处：footer 设置入口红点（title 带版本号）、托盘图标叠加红点
   （core::badge::overlay_badge 纯像素运算，白描边 + 抗锯齿，default_window_icon
   RGBA 原地画，无新图标资产、无新依赖）、托盘菜单动态插「有新版本 vX…」项
   （点击同 Settings 项：唤窗 + open-settings）。语言切换/设置导入后菜单重建
   保留红点项（refresh_tray_menu 重读 state）。
3. 安装仍手动：「更新并重启」按钮不变，不做静默自动装。
4. set_update_badge 幂等（同版本重报 early return）；relaunch 天然清 badge。

三轮：53/53（新增 badge 2 测：中心像素 [244,63,54,255] 精确、角落不动、
逐字节确定性、坏 buffer/极小尺寸 no-op 不 panic）×2 次复跑一致；clippy 0；
tsc+vite 过。浏览器 demo（新增 ?update=1 模拟开关）：有参数时红点出现
（背景 rgb(244,63,54)、title "新版本 9.9.9 可用。"）+ 设置行 + 更新并重启
按钮；无参数对照红点不出现。托盘图标/菜单为 Tauri 运行时行为，浏览器
不可达，靠单测 + review 覆盖，出包后真机复核。

---
# 验收记录 2026-08-28（图片剪贴历史 + 设置导入导出 + CI pnpm 修复，拟 v0.1.19 批二）

1. 图片剪贴历史：clips 表增列（kind/image/thumb/width/height，PRAGMA 条件
   ALTER 迁移）+ clip_image_vecs（SigLIP 空间，与 e5 文本空间分表）。watcher
   文本优先，无文本时 poll_image（采样哈希 <1ms/4K 截图，同 last_hash 去重），
   RGBA → ≤1600px JPEG + 96px thumb 入库；inline SigLIP embed（try_lock），
   模型就绪时批量补嵌。检索三路 rank_hybrid：FTS + e5(文本条) + SigLIP
   文本→图（图片条）。Enter/copy_image_clip 解码回 RGBA 上剪贴板；
   Shift+Enter 粘贴仅文本条参与合并。行/预览：缩略图 + 尺寸 + 大图。
   条数/时长/清空/机密标记护栏全部天然覆盖（同表）。
2. 设置导入导出：EXPORTABLE_META 白名单（token 显式排除）+ 前端
   localStorage 九键合并为单 JSON；导入即时生效（别名重挂 + 托盘语言），
   前端 reload 重读。
3. CI 门修复：报错 "packages field missing or empty" 根因是版本错配——
   CI 钉 pnpm 9，而 pnpm-lock.yaml / pnpm-workspace.yaml（纯配置、无
   packages 字段）由本地 pnpm 11 生成，pnpm 9 拒收。第一刀只删了
   setup-node 的 cache 配置（表象），同错误换到 install 步骤复发；
   第二刀 CI 对齐 pnpm 11，转绿。教训：修 CI 报错先对齐工具链版本。

三轮：51/51（新增 image_clip_roundtrip_and_dedupe：编码尺寸保持/去重计数
/JPEG 逐字节回读/异图异哈希）；clippy 0 告警；tsc+build 过；浏览器实测
图片剪贴行（"图片"+1600×1000+缩略图）、预览大图 + meta、设置文件行
导出/导入按钮。批一（frecency/多屏/粘贴/视频跳转）已单独验收提交。

---
# 验收记录 2026-08-28（结果预览面板，拟 v0.1.18）

`→`（光标在输入末尾）展开 / `←` 收起；窗口 720→1100 宽度自适应。各源预览：
文本文件=get_preview 按 query 定位命中窗口（±400/2400 字符、char boundary
安全）+ 前端逐词高亮；图片=磁盘现读 560px（索引只存 96px）；视频=镜头条带
（≤60 thumbs + 时间标签，当前命中镜头描边）；repo=description + topics
chips + README 开头 2400 字符（原文本就在 repos 表）；书签/历史/剪贴/应用
用 hit 自带字段（剪贴多行全文是老痛点）。数据全部本地索引/本机磁盘，零联网。

三轮（发现 2，均已修）：
- R1：tsc/vite/cargo 三端编译（image crate 依赖归位 core：preview_b64_for
  贴 thumb_b64_for；base64 Engine trait 导入）；48/48。
- R2：浏览器实测交互链——真实焦点路径（事件自 input 冒泡）→ 展开、文本
  预览 <mark>Vector</mark> 高亮、↓ 换选刷新、← 收起、hint 随态切换（→ 预览
  / ← 收起）、面板跨 tab 保持展开。发现①：切 tab 瞬间陈旧数据可渲染
  （text 分支不校验 data 归属，hit 已是 repo 仍显示上一文件正文）→ 修：
  hit 变化立即清 preview + text/image/shots 分支加 hit.kind 守卫，结构上
  封死错配渲染。发现②（测试层）：合成 ArrowRight 派发在 panel 节点不进
  React 合成事件，需派发在 input 上冒泡——真实键盘无此问题，测试脚本修正。
- R3：修复后复测 repo 预览（desc+chips 完整）、videos 镜头网格 8 格 +
  时间标签、file 文本高亮复现；tsc+build 复跑 0 错。
- 光标语义护栏实测：`→` 仅在光标处于末尾时展开，输入中不劫持。

---
# 验收记录 2026-08-27（解码限制设置项，拟 v0.1.18）

DecodeOpts{threads, hwaccel}：设置 → 解码限制行（线程 1/2/4/自动 pills +
硬解开关）。默认 threads=2（后台礼貌值，不吃满全核）、硬解默认关；
`-hwaccel auto` 失败进程级记忆（HWACCEL_BROKEN）自动回退软解不反复撞墙。
meta：video_decode_threads / video_hwaccel；set_video_decode 命令；
detect_shots/frame_at/index_video 全链路透传。

三轮：48/48（decode_args 纯函数单测：空/线程/硬解组合 + 默认值断言）；
clippy 0 告警；tsc+build 过；真机 E2E 复跑（frame_at 走 threads=2 抓帧，
命中与基线逐字节一致 score 1.000）。

---
# 验收记录 2026-08-27（videos 档位 + ffmpeg 自托管/预下载/状态行，拟 v0.1.16）

四块：
1. 本地 tab 第四档「videos」：video_name_search（LIKE 中缀 + 前缀优先排序，
   files 表视频扩展名过滤）+ SigLIP 文本语义镜头搜索，rrf_fuse 按文件融合；
   同一视频双路命中保留镜头表现（带时间范围/缩略图），纯名字命中显示为
   文件级行（时长 + 首镜头缩略图借用）。images 档还原为纯图片。
2. ffmpeg 自托管：magpie 仓库 ffmpeg-1 prerelease 挂三平台单文件 zip
   （gyan.dev win 37MB / evermeet mac-x64 26MB Rosetta / johnvansickle
   linux 29MB，GPL 构建来源注入 README 与 release notes）。解析链：系统
   PATH → 本地缓存 → magpie Release 资产（download.rs 断点续传 + 进度%）
   → sidecar 上游兜底 → 报错提示手动安装。MAGPIE_FORCE_MANAGED_FFMPEG
   测试口。
3. 启动预检查：开关开 + 文件夹里存在视频 → 立即解析/预下载（不等模型；
   无视频用户零下载）。
4. 设置行常驻 ffmpeg 状态（系统已装/已下载/下载中 N%/missing）。

## 连续三轮干净（发现数 2，均已修）

- R1：47/47（新 videos_scope 融合测试：名字中缀命中文件级表现、语义命中带
   3000..6000ms 范围、双路命中去重取镜头、非视频文件不漏入）；真机 E2E
   managed 链——强制跳过系统 ffmpeg 后从 release 资产下载（进度打点到
   100%）→ unzip_single 解出 → `ffmpeg 9.0.1-essentials` -version 运行成功。
- R2：clippy 两轮共发现 2 条（params 宏路径笔误编译期即挡；manual checked
   division lint）→ 修复后 0 告警；tsc + vite build 0 错。
- R3：浏览器 demo——四档 pill（全部/文本/图片/视频）、videos 档 placeholder、
   两种结果形态实测（镜头行 3:24–3:42+缩略图+12:34 时长；文件级行仅时长
   32:02）、设置行「· ffmpeg: 系统已装」、Ctrl+, 顺带复测；47/47 与
   search 7/7 复跑一致。
- 待 CI：mac/linux 编译；上游兜底路径未真机测（代码路径与 sidecar 文档
   一致，失败落 video_note）。

---
# 验收记录 2026-08-27（应用别名层 + 视频镜头搜索，拟 v0.1.15）

两块：
1. 应用别名：AppEntry.aliases。三来源——内置 NAME_GROUPS 中英名对照
   （~30 组，双向：装成飞书输 lark 能中，装成 Lark 输 feishu 经别名拼音
   也能中）+ 用户规则（meta app_aliases，"proxy = clash" 一行一条，
   set_app_aliases 命令即时重挂）+ Linux .desktop Keywords=/GenericName=。
   匹配：别名走同一 score_name 通路（含拼音），×0.95 保证本名优先。
2. 视频镜头搜索：videos.rs——ffmpeg（系统优先，缺失 sidecar 自动下载）
   2fps/160px 解码 → 64-bin RGB 直方图卡方距离切镜头（纯 Rust，无 OpenCV，
   阈值 0.4 + 最短 1s）→ 每镜头代表帧（中点 + 长镜头每 20s 一帧，≤3/镜头
   ≤200/视频）→ 480px 抽帧 → SigLIP embed + 96px 缩略图 → video_index/
   video_shots 表（FK 级联）。检索 search_video_shots 按视频分组取最佳
   镜头；图查询按余弦与图片混排，文查询（images 范围）追加在文件后
   （分数不同尺度不硬混）。结果行：缩略图 + 时间范围 + 时长 + 相似度%。
   开关 set_video_indexing（默认开）；worker 持锁按视频粒度，坏文件记
   mtime 跳过不卡队列；启动（siglip 就绪后）+ 30min 周期触发。

## 连续三轮干净（发现数 1，已修）

- R1：apps 9/9（别名双向/评分次序/用户规则 5 个新断言）+ videos 单测 5/5
  （硬切边界/静态场景零误报/最短镜头抑制/代表帧/扩展名）；真机 E2E：
  ffmpeg 合成三场景视频（红3s|蓝3s|SMPTE3s）→ 恰 3 镜头、边界精确
  3000/6000ms、红帧查询命中 0..3000ms score 1.000、thumb 1092B 入库。
  浏览器 demo：别名编辑区（改动激活保存→保存后回读 status）、视频结果行
  （89% 按分插排 94/87 之间、"3:24 – 3:42"、"视频"徽章、缩略图）、设置行
  计数 pill 486。
- R2：全量 45/45；clippy 发现 2 条 chunks_exact 常量块 lint（本轮唯一
  发现）→ as_chunks 修复后 0 告警；tsc + vite build 0 错。
- R3：E2E 幂等复跑——同库第二次 pending 为空（零重索引），shots 保持
  3 条、查询命中与 score 逐字节一致；videos 5/5 复跑。
- 待 CI：ffmpeg-sidecar mac/linux 编译；国内网络 sidecar 下载不可达时
  的降级路径（video_note 显示错误，功能其余不受影响）已在代码层保证，
  真机验证待用户侧。

---
# 验收记录 2026-08-27（中英双语 UI + 应用启动器拼音匹配，并入 v0.1.14）

两块：
1. 中英双语 UI：src/i18n.ts 自研字典（英文原文作 key，~145 条 zh 译文，
   零依赖）。语言偏好 auto/en/zh 存 localStorage magpie.lang，auto 按
   navigator.language 解析；setLang 在触发 re-render 前同步更新模块态。
   托盘菜单跟随：新命令 set_ui_lang 存 meta ui_lang 并 rebuild 托盘菜单
   （TrayIconBuilder::with_id("main") + tray_by_id + set_menu），启动时
   前端把解析后的语言同步给后端。
2. 拼音匹配应用：core pinyin crate（0.10）。match_pinyin 按字生成候选拼写
   （全拼 + 首字母，多音字全读音），DFS 一次覆盖全拼/首字母/混拼；分隔符可
   跳过、ASCII 字符须原样匹配；起始于词首 0.8-长度罚，词中 0.55，均低于
   原文前缀/子串。护栏：查询 <2 字符或含非 ASCII 不走拼音；纯拉丁名不产生
   拼音候选。开关：设置行（默认开）存 magpie.pinyin，经 search_apps 的
   pinyin 参数透传 match_apps。

## 连续三轮干净（发现数 0）

- R1（静态一致性）：可见文案 grep 仅剩 aria-label 英文（读屏可接受）+
  "GitHub" 专有名词；t()/tf() 126 个字面 key 与字典对账全命中（差集全部为
  动态键 t(s.label)/t(o.label) 与 split( 正则误报，浏览器实测动态键渲染
  正确）。cargo test apps 7/7；tsc+vite build 0 错。
- R2（机制通路+边界）：浏览器 demo 实测——auto 在中文系统渲染中文；点
  English 全界面即时翻转（eyebrows/tabs/placeholder 数据对比）；点中文+关
  拼音后 reload 状态保留（langActive=中文, pyActive=关）；invoke 捕获
  search_apps args={query:"v",pinyin:false} 与开关一致（通路存活）；结果行
  中文（徽章"应用"/副题"应用程序"）。全量 38/38；clippy 双 crate 0 告警。
- R3（错误路径+回归）：match_apps 22 处调用全带第 4 参；set_ui_lang 四处
  wiring（fn/handler/chooseLang/启动 effect）齐；英文态结果行（App/
  Application）实测；全量 38/38 复跑；pnpm build 两次产物 hash 逐字节一致
  （index-BWDC8VDM.js / index-CRUez5ik.css）。
- 待 CI：托盘 set_menu 路径 mac/linux 真实编译（Windows cargo check 已过）。

---
# 验收记录 2026-08-22（mac 剪贴板机密标记 + tab 自定义，并入 v0.1.12）

两块：
1. mac 剪贴板机密标记：加 objc2/objc2-app-kit/objc2-foundation（仅
   target macos），读 NSPasteboard.availableTypeFromArray 检测
   org.nspasteboard.ConcealedType / TransientType / AutoGeneratedType，
   命中则不记录——与 Windows 的 ExcludeClipboardContentFromMonitorProcessing
   对称。Linux 仍 false（无跨 DE 约定）。
2. tab 自定义：设置里可调 tab 顺序（↑↓）+ 星标指定启动默认 tab。
   localStorage 存 magpie.taborder（id 数组）+ magpie.defaulttab。
   SOURCES 重构为 ALL_SOURCES 常量 + 运行时 orderedSources 派生，未知 id
   丢弃、缺失的规范源自动补齐（应对将来加新源），sourceIdx 越界回退 [0]。

## 风险与验证边界

- mac 机密标记代码 cfg(target_os="macos") 隔离，Windows 本机 cargo check
  编不到——已逐行对照 objc2-app-kit 0.3.2 crate 源核对 API（generalPasteboard
  / availableTypeFromArray / types 均 safe fn；NSArray::from_retained_slice、
  NSString::from_str、objc2::rc::autoreleasepool 路径确认）。真实编译由 CI
  macos-latest 执行，构建失败即修。系统框架 AppKit/Foundation 自带，无额外安装。

## 连续三轮干净（本地可验部分）

- R1（编译+测试）：Windows cargo check 全 workspace 过（mac 码 cfg 隔离），
  35/35 核心测试，clippy -D warnings 双 crate 0 告警，pnpm build tsc 0 错
- R2（前端逻辑复查）：loadTabOrder 容错（坏 JSON 回退 DEFAULT_ORDER）；
  moveTab 相邻交换+持久化+按 id 保活跃 tab；sourceIdx 初始读 defaulttab
  找不到回退 0；source 派生 `sources[idx] ?? sources[0]` 防越界；Tab 循环
  用 sources.length；runSearch 经 sourcesRef 读 srcId 不受重排竞态影响
- R3（静态一致）：CSS var(--hover) 不存在→改 --row-hover（token 核对）；
  旧 magpie.source 键弃用改 defaulttab；onKeyDown deps 补 sources；无发现
- 待 CI：mac 机密标记真实编译（macos-latest）

---

# 验收记录 2026-08-22（浏览器历史 + 应用启动器，拟 v0.1.12）

两块：
1. 浏览器历史并入书签源 → 统一 "Web" 源，Shift+Tab 切 全部/书签/历史。
   history 独立表（visit_count/last_visit 排序信号，按访问量取每 profile
   top 3000），读 Chromium `History` SQLite + Firefox moz_places（temp copy
   避锁）。search_web 合并 bookmarks+history 按分排序，书签 +0.05 curated
   加权，历史加 log(visit_count) 访问量微调。标题+URL 双字段可搜。
2. 应用启动器：枚举开始菜单 .lnk / mac .app / linux .desktop，名字匹配
   （精确>前缀>子串>首字母缩写），Enter 启动；作为本地源置顶命中，不占
   tab。启动时扫描一次常驻。

## 发现并修复（计数清零）

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 真机噪声 | 松散子序列匹配把 "code" 匹到 "RecoveryDrive" 等垃圾，且 app 置顶会污染本地结果 | 改首字母缩写匹配（vsc→Visual Studio Code），拒散落子序列 |
| 2 | 编译 | history embed_pending 单行 collect 导致 stmt 借用未活够久 | 绑定 rows 变量 |

## 连续三轮干净（末次修复后）

- R1（真机数据）：sync_history 读 chrome+edge 共 **1515 条**历史落库，
  FTS "github" 命中真实记录带 visit_count（24×github.com）+ 标题
  （openai/codex 全标题）——证标题可搜非仅 URL；list_apps 枚举 **137** 个
  应用，"chr"→Google Chrome(0.60 子串)、"code"→0 命中(无垃圾)、缩写测试通过
- R2（单测+clippy）：35/35 通过（新增 history roundtrip、apps 缩写/前缀/
  空查询 4 测），clippy -D warnings 双 crate 0 告警
- R3（静态集成复查）：旧 localStorage source="bookmarks" findIndex=-1 自动
  回退 local；web 空查询双 guard 返回空；refresh 走 sync_bookmarks_now 同步
  书签+历史；launch_app 后隐藏窗口；无阻断发现

---

# 验收记录 2026-08-22（剪贴板历史第四源，拟 v0.1.12）

功能：opt-in（默认关）文本剪贴板历史。常驻线程 1s 轮询，变化才落库，
按内容去重（同文本 bump last_copied+copy_count），Windows 尊重密码管理器的
ExcludeClipboardContentFromMonitorProcessing / CF_CLIPBOARD_VIEWER_IGNORE
标记不记录。第 4 个 tab；空查询例外地展示最近复制（剪贴板本能形态）；
Enter=复制回；Ctrl+Del 删选中；Shift+↑↓ 多选（多选 Enter=合并复制）。
设置：开关 + 保留条数（500/2000/无限）+ 保留天数（7/30/永久）+ clear。
超上限“每次打开时清理”（启动 housekeeping）+ 记录时即时裁剪。存自家
SQLite，不依赖系统剪贴板历史。

## 连续三轮干净

- R1（真机 E2E）：桌面会话注入剪贴板文本，dbtool 查真库——捕获成功
  （bilibili URL + 注入 marker），去重实证：重复复制同文本 count 1→2、
  last_copied 刷新、不新增行；不同文本独立成行。OS 剪贴板→轮询→record→库
  全链路通。
- R2（单测+静态）：30/30 通过（新增 count_cap_drops_oldest / dedup /
  retention 三测），clippy -D warnings 双 crate 0 告警
- R3（顺序+边界+并发复查）：看守线程 record→按条数裁剪→嵌入→reload 顺序
  正确（新 clip last_copied 最新必留存）；开关 off 后线程 ≤1s 退出、
  clip_thread_alive 守卫防叠线程、快速 off/on 不泄漏；max_entries=条数
  语义与用户澄清一致，DEFAULT_MAX_LEN 100k 字符仅防超大转储内部保护；
  多选删除后本地 splice + refreshStatus 一致。无发现。
- 清理：测试 clip 已清空（clip_count=0）、clipboard_enabled 复位 0（默认关）

---

# 验收记录 2026-08-22（自动更新，v0.1.11）

方案：tauri-plugin-updater，GitHub Releases 当更新源（latest.json 固定地址），
minisign 签名验证（公钥编译进 app，私钥+口令在 GH secrets 与本机
~/.tauri/magpie.key）。设置页手动检查 + 启动后 15s 静默检查；下载带百分比；
Windows installMode=quiet 全静默装。

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 真机 E2E | 默认 passive 装载模式弹 UI，安装器挂在会话里等交互，更新永不落地 | installMode: quiet |

## 本地全链路验证（真机，先验证后出包）

本机 8787 起静态服务器伪装更新源，构建 A(0.1.10)/B(0.1.11) 双签名安装包：
A 装机运行 → 15s 自动发现 → A 内置公钥验 B 的 minisign 签名 → 下载 12.7MB
→ 静默安装 → 自动重启，进程实测 FileVersion 0.1.11。服务器日志见
latest.json 与安装包各被拉取；签名/下载/安装/重启四环节零人工干预。
测试补丁（localhost 端点、发现即安装钩子）已全部回滚，不入库。

---

# 验收记录 2026-08-21（模型下载镜像兼容 + 进度，拟 v0.1.10）

症状（用户国内机器）：e5 报 "huggingface api error: header etag is missing"、
SigLIP 报 "io: unexpected end of file"。根因：hf-hub 协议强依赖响应 ETag 头
（镜像 CDN 与干扰性中间盒常剥掉它），且元数据往返多失败面。

方案：hf-hub 保持首选（离线缓存零回归）；失败即回退自研纯 GET 下载器
（`{endpoint}/{repo}/resolve/main/{path}` 直链、Range 断点续传、.part+rename
原子落盘、4 次重试、连接 20s/响应头 60s 超时）。进度百分比经 model-status
直达设置页。切镜像重试加 in-flight 守卫 + reinit 排队防并发写 .part。

## 发现并修复（计数清零轮）

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 单测抓获 | 测试服务器只匹配大写 Range 头，ureq 发小写，续传未生效 | 大小写不敏感解析 |
| 2 | 单测暴露 | 客户端盲信 206：服务器 Content-Range 起点与请求不符时会拼接出损坏文件 | 校验起点，失配删 .part 重下 |
| 3 | 逻辑 | 切镜像重试原条件仅 "failed"，挂死的 "loading" 永不重试 | != "ready" 即重试，配守卫与排队 |

## 连续三轮干净

- R1（单测+静态）：27/27 通过，含截断-续传字节级校验与原子重命名断言；
  clippy -D warnings 双 crate 0 告警
- R2（真网机制存活）：verify_download 例程对 hf-mirror.com 全量真下载
  （e5 五文件 + SigLIP 全套，SigLIP 39.8s），回退与主路径同文本嵌入
  余弦 = 1.000000（两模型均是）——pooling/max_length 配置等价实证
- R3（边界+一致性复查）：dest 复用仅发生在完整 rename 之后；reinit 重试
  有界（失败一次额外尝试后止）；known-length 短读必 bail 保 .part 续传；
  复跑 27/27 + clippy（含新 example）全绿。已知限缺口：响应无
  Content-Length 时无法校验完整性（resolve 端点实测均带长度）

---

# 验收记录 2026-08-21（设置页文件夹列表不可见，v0.1.9）

症状（mac 与 Windows 同现）：设置页 folder 计数徽章=1，但列表区完全空白，
无空态文案也无报错。根因：`.folder-list` 是高度受限（max-height:560px）的
flex column 卡片里唯一带 `overflow-y:auto` 的子元素——此类元素允许被压到
0 高，全部 flex 压缩量都落在它头上，行在 DOM 里但高度为 0。修复一行：
`flex-shrink: 0`。

验证链（夹逼定位法）：
- 数据层：dbcheck 例程跑真实库，folder_count=1、serde JSON 正确
- IPC 层：v0.1.8 诊断徽章显示 1（get_status 通）
- 空态分支：截图无红字无 "No folders" → folders 数组非空 → 唯余渲染层
- 渲染层：CSS 审计锁定 flex 挤压；同类元素全查（.results 父容器无高度
  上限，不受影响，仅 .folder-list 一处）
- 修复验证：debug 构建投放真机桌面会话，用户目视确认列表显示（prompt-shelf
  · 190 files + ↻/✕ 按钮）
- 教训：v0.1.5/0.1.6 两轮"修复"（refresh-on-open、空态文案）修在错误层，
  因为当时把"看不见"当成了"没数据"。不可见 ≠ 空。

---

# 验收记录 2026-08-21（书签通用发现，v0.1.7）

变更：discover() 加通用 Chromium 分支扫描（`<app>/Default|Profile*/Bookmarks`、
`<app>/User Data/...`、vendor 嵌套一层），按书签文件路径去重；已知浏览器仍
显式命名优先。动机：用户默认浏览器 ego（Chromium 分支）不在硬编码列表。

## 连续三轮干净

- R1（机制存活）：25/25 单测 + clippy -D warnings 0 告警；新单测证 fork 布局
  两种（直下/User Data 嵌套）均被发现、非浏览器目录忽略、重扫零新增；本机
  实测 discover 扫全量 LOCALAPPDATA 仅出 chrome+edge 两 store，零误报，
  452ms（后台线程可接受）
- R2（边界）：无权限/不存在目录走 read_dir Err 静默；Snapshots/Guest 目录
  不匹配 Default|Profile* 前缀不进入；显式+通用重复发现由路径去重（单测+
  真机双证：chrome 只出现一次且名为 chrome 非 google）；复跑 25/25
- R3（静态一致）：README 双语"Chrome、Edge、Brave、Firefox"改为"任意
  Chromium 内核+Firefox"；grep 全仓无其他写死浏览器列表；cargo check 全
  workspace 过；复跑 25/25。注：cargo fmt 不合规为仓库基线（CI 不查 fmt，
  未动文件同样不合规），不计入本轮
- 附诊断工具：`print_discovered_stores` ignored 测试，真机排查用

---

# 验收记录 2026-08-21（第二轮，全功能形态）

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 静态一致 | clippy：office 解压处多余 mut | 移除 |
| 2 | 静态一致 | README 写 80+ 格式，实际 TEXT_EXTS=78 | 文案改 ~80/近 80 |
| 3 | 并发 | set_max_file_mb 在索引进行中清表可能 busy 失败，留下"上限已改但未重建" | 索引中拒绝执行 |
| 4 | 不变量 | file_chunks 的 DELETE+INSERT 非事务，崩溃可留"新 hash 缺块"且永不自愈 | 每文件 unchecked_transaction |
| 5 | 边界 | 热键录制放行裸键/Shift+键，会全局劫持正常输入 | 必须 Ctrl/Alt/Super 或 F 键 |

## 连续三轮干净

- R1（可复现性）：21/21 单测 ×3 逐次一致；clippy 0；真模型 e2e 全绿（130K 深度 FTS 命中+高亮、180 chunk、图文/图图检索、逐字节确定性）
- R2（机制存活+真机）：debug exe 冒烟 10s 存活；热键 meta 读取通路实证；stars.db 107MB（真实语料全文+向量在跑）
- R3（本轮 diff 复查）：事务回滚路径、守卫残余竞态（busy_timeout 顺序化，无损坏）、README 数字核对，无发现

---

# 验收记录 2026-08-20

## 发现并修复（计数清零轮）

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 逻辑 | `search()` zip 分数错位：`repos_by_ids` 跳过缺失 id 后 score 挂错 repo | id→score map 查找 |
| 2 | 逻辑 | 无 README repo 每次同步重复 404 | `readme_pushed_at` 兼作"已尝试"标记（空 pushed_at 存 ""），加 `stale_readme_targets` 纯函数 + 单测 |
| 3 | 错误路径 | Alt+Space 被占用（如 PowerToys Run）时 setup `?` 直接崩 | 降级为 eprintln 继续跑（冒烟实证生效） |
| 4 | 交互 | 点击行打开的是 selected 而非被点行 | `openHit(r)` 直传 |
| 5 | 机制通路 | RRF 纯排名融合双列表对称必打平，语义强度不参与（单测抓到） | `score += 0.005×cos_sim` |
| 6 | 隐私/正确 | `ignore` crate 默认仅 git 仓库内生效 .gitignore（单测抓到 secret/ 被索引） | `require_git(false)` |
| 7 | 资源 | README 全文入库无上限 | 截 64K 字符 |
| 8 | 静态一致 | 4 个未使用 capability + 未使用 JS opener 依赖 | 删除 |
| 9 | 静态一致 | README 声称模型 ~120MB，实测 487MB（fp32） | 文档改 ~470MB |
| 10 | 可复现 | recent 查询 ORDER BY 无 tie-break | 加 `id DESC` |
| 11 | 边界 | 空结果时方向键把 selected 置 -1 | nav 键空结果早退 |

## 连续三轮干净（全部无发现）

- R1（逻辑/不变量）：cargo test 19/19；clippy 0 警告；tsc+vite 通过
- R2（可复现性）：test suite 连跑 3 次，19/19×3 逐次一致；排序 tie-break 全走 (score, id) 确定序
- R3（边界/退化 + 真机）：debug exe 冒烟 10s 存活，stderr 仅预期的沙箱快捷键降级行；db/模型（12 文件 487,351,935 B）就位

## 未验证项（需要真实交互环境）

- acrylic 磨砂在 Win11 桌面的实际观感、无边框窗口圆角
- 真 token 全量同步（API 路径按文档实现，未跑真数据）
- Alt+Space 在交互桌面注册（沙箱无窗口站测不了）
