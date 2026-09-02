# 验收记录 2026-09-02（最近打开关掉后列表残留，待出包 v0.1.32）

用户报障（v0.1.31）：关闭「显示最近打开」后回到输入框，列表还在，切换 tab
才消失。

根因：开关只写 localStorage，不触发搜索；结果列表是上一次 `runSearch` 按旧
设置算出来的，挂到下一次 query/tab 变化。v0.1.31 验收时我在"未验证项"里写过
"最近打开在真机上的行渲染"没验——就是这一步没做到位。

修复：开关点击时立刻 `runSearch(queryRef.current, sourceRef.current)`，关→列表
即时清空，开→列表即时出现。demo mock 补了 `recent_hits` 数据（本地 tab 三行），
这类场景以后可在无头里复现。

## 复现与对照（puppeteer 无头 demo，按用户路径）

启动时开关已开 → 空查询列出 3 行 → 设置里关掉 → Esc 回输入框：

| 版本 | 结果 |
|------|------|
| 未修复（`git stash` 掉 App.tsx 改动） | rows=3，**残留**，与报障一致 |
| 修复后 | rows=0，再开回去 rows=3 |

第一版脚本写的是"设置里先开再关"，未修复版下"开"也不会立刻出列表，于是"关"
时本就为空、假 PASS——脚本没复现报障。改为"启动即开"的路径后才对照出差异。
教训：回归脚本必须先在坏版本上跑出 FAIL，才算复现。

## 连续三轮干净

- 回归 5/5（上表）；特性总回归 18/18（`recent_hits` 期望由"null 不崩"改为
  "渲染 3 行"）；静态扫描 0 FAIL（demo mock 19 case）；tsc + vite 通过。
  Rust 侧零改动，套件不重跑

---
# 验收记录 2026-09-02（七个日常小功能，待出包 v0.1.31）

用户从候选清单里定了七条（汇率换算明确不做）：Ctrl+C 对所有行生效、空查询显示
最近打开（设置里启用）、剪贴板文本动词、日期计算、搜索过滤语法、划词搜索热键、
快速记录；小贴士、README、落地站一并更新。

## 实现要点

- **Ctrl+C 全行**：文件/视频→路径、repo→html_url、书签/历史→url、文本剪贴条→
  内容、图片剪贴条→图片本体（copy_image_clip）、应用→launch target。
  Ctrl+Shift+C 仍只对文件（文件本体）
- **最近打开**：frontend 开关（localStorage `magpie.recents`，默认关，纳入导出）；
  后端 `recent_hits(source)` 从 hit_stats 取该 tab 涉及的 kind，按 key 反查回
  完整行（新增 `file_by_path` / `bookmark_by_url` / `history_by_url`，repo 走
  `repos_by_ids`，app 从内存表找），已删除/卸载的自然掉出
- **文本动词**：`json / upper / lower / trim / slug / lines / count`，无参数时
  作用于剪贴板（标签带 "clipboard →"），有参数作用于参数；Enter 复制回去。
  多行结果前端只显示首行 + 行数，复制仍是全文
- **日期计算**：`today + 30d`、`2026-10-01 - today`、`until <date>`、
  `since <date>`、`<date> ± N months` 等；结果带星期与距今天数。单独 `today`
  仍是搜索词，只有 ISO 日期能单独成立；`2026-13-40` 这类无效日期继续走原来
  的减法（既有行为，不改）
- **过滤语法**：`ext:pdf` / `.md` / `>10mb` / `<500kb` / `7d 2w 3m 1y` /
  `in:projects`，从文本里剥离后再进 FTS 与向量；纯过滤（无文本）走 SQL
  WHERE 直查（`files_matching`），避免把 2000 行缩略图拉出来筛；带文本时
  候选数 ×8 再筛
- **划词搜索**：第二个全局热键（meta `hotkey_selection`），注册与唤出热键合并
  在 `register_hotkeys` 里（插件只有 unregister_all）；按下→合成 Ctrl/Cmd+C→
  160ms 后读剪贴板→若内容没变视为无选区（空查询唤出）→emit
  `search-selection`→show_window
- **快速记录**：`note <text>` → 顶行命中，Enter 追加
  `- YYYY-MM-DD HH:MM  text` 到 meta `note_path`（默认库旁 notes.md，即
  Roaming 目录），设置里可改路径、可直接打开

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 既有测试 | `parse_days` 用 `split_at(len-1)` 切字符串，中文 token 切在 UTF-8 码点中间 panic（OCR 中文用例抓到） | 按最后一个 char 切；加 CJK/重音 token 用例 |
| 2 | 我的测试 | 断言 `2026-13-40` 应为 None，实际既有减法路径返回 1973 | 改断言为既有行为并注明 |
| 3 | R2 | 图片剪贴条 Ctrl+C 什么都不做（identity 只想到了文本） | 图片剪贴条 → copy_image_clip |
| 4 | clippy | `iter().any(==)` 应为 `contains` | 改 |

## 连续三轮干净（第 3 项修复后重新计数）

- A 机制通路（puppeteer 无头 demo，IPC 调用拦截可观测）18/18：note 行出现、
  `note` 单词不触发、Enter 调 `append_note{text}`；Ctrl+C 六种行各复制正确
  标识（文件路径、repo URL、web URL、图片剪贴调 copy_image_clip、文本剪贴内容）；
  三个新设置行渲染；最近打开开关持久化到 `magpie.recents=1`；空查询确实调
  `recent_hits{source:"local"}`、mock 返回 null 不崩；过滤语法不破坏搜索；
  0 页面错误（favicon 404 为 dev server 仪器噪音）
- B 静态一致 + 文档对称：命令面 54 定义 = 54 注册全被引用；i18n 138+27 键全有
  中文；21 个 meta 设置项写读平衡；17 个 set_/toggle_ 命令 UI 可达；站点 rest
  条目 en 15 = zh 15；README 条目 43=43、快捷键行 13=13、破折号 0/0；七个新
  功能在 README 双语与站点双语各有提及（14 项核对全过）
- C 真机（生产 exe，VK 注入，Shift 切英文输入）8/8：`json` 行显示
  `= { … (7 lines)`，Enter 后 Get-Clipboard 拿到格式化 JSON；`2026-10-01 + 2w`
  → `2026-10-15 · Thursday · in 43 days`；`note acceptance run ok` → notes.md
  出现 `- 2026-09-02 08:58  acceptance run ok` 且浮窗自行收起；`ext:png` 只列
  png；种入 `Ctrl+Alt+F9` 后按下浮窗可见；日志尾无注册警告。套件 107 测试连跑
  3 次一致；clippy 0；tsc+vite 通过

新增单测 32 个：filters 8、calc 日期 5、transform 动词 6、notes 3、frecency
recent 1，加上既有。

## 仪器坑（记账）

- Tauri 的 `app_data_dir` 在 Windows 是 **Roaming**（`%APPDATA%`），`app_log_dir`
  才是 Local。第一次真机跑把 meta 种进了不存在的库、notes.md 也查错目录，两条
  假 FAIL。真机脚本已改，此后别再混

## 未验证项

- 划词搜索的"复制选区"半程：真机只验了热键注册与唤出（无选区→空查询）。合成
  Ctrl+C 与 `paste_clip` 同一套 enigo 代码（已在生产用），剪贴板读取有单测；
  完整链路需在另一应用里选中文字，待用户实测
- 最近打开在真机上的行渲染：demo 里 mock 返回 null 只验了不崩；后端反查逻辑有
  `frecency::recent` 单测，JSON 形状与各 Hit 类型逐字段核对过，未在真机点开验
- macOS/Linux 上 `Key::Meta` 的复制合成与 Ctrl+C 系统级行为

---
# 验收记录 2026-09-01（搜索取消：过期查询不再占住连接，待出包 v0.1.30）

用户报障：反复输入、删除 "vscode" 时会先触发很耗时的单字符 "v" 查询；旧版
只丢弃结果、不取消查询，数据库连接被占住，后续搜索排队卡死。用户转述 GPT 的
建议方案：前端只保留最新请求，后端用 SQLite InterruptHandle 取消旧查询。

核实（改前代码）：前端 App.tsx:609 只做内容比对丢弃过期结果，从不取消；后端
`AppState.db` 是 `AsyncMutex<Connection>` 单连接单锁，慢查询握锁时后续全部
`lock().await` 排队。属实。

## 实现（在建议基础上加了一层隔离）

- 专用**读连接** `search_db`（WAL 下与写互不阻塞），四个搜索命令
  （search_stars / search_local / search_web / search_clips）全部迁移过去。
  InterruptHandle 只指向这条连接——**取消永远打不到写入**。索引器本就自开
  连接（先前已核实），三方各自独立
- `take_search_conn`：领票（AtomicU64 自增）→ interrupt() → lock().await →
  到手后若票已过期则直接让锁（None，返回空，前端反正会丢弃）。SQLite 语义
  保证 interrupt 只打断"标志置起时正在跑"的语句，计数归零后新语句不受影响
  （有单测钉住），所以自杀/误杀均不可能
- 前端 runSearch 加单调 seq 票，替换原内容比对：只有最新请求可发布结果
  （后端取消覆盖主干，seq 兜住已在途的尾巴）

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 用户报障 | 过期搜索占住唯一 DB 连接，后续搜索排队 | 如上：读连接 + 票 + interrupt |
| 2 | 设计评审 | 直接在共享连接上 interrupt 会有微秒级窗口误杀写事务（clip 插入、设置写入同连接） | 读连接隔离，interrupt 的可达范围只剩搜索 |

## 连续三轮干净

- R1 静态一致：a1/a2 扫描 0 FAIL；`search_db` 全文件仅 3 处出现（声明/助手/
  初始化），4 个调用点恰好是 4 个搜索命令（逐一核对所属函数）；clippy 0
- R2 机制通路（puppeteer 无头驱动 demo）5/5：搜索 4 行正常；**报障场景重放**
  ——3 轮 × 12 次 25ms 间隔的 "vscode" 输入-删除循环（36 击键）后立即搜
  "vector"，4 行正确返回、输入框内容正确、清空后列表归零。另有 1 条
  favicon.ico 404 为无头浏览器向 dev server 索要图标，Tauri 走 asset 协议
  无此请求，仪器噪音不计
- R3 可复现：84 测试（新增 4 个取消测试）连跑 3 次逐次一致；tsc+vite 通过

新增的 4 个机制单测：interrupt 打断运行中查询且连接存活（无限递归 CTE 在
~60ms 被停）；空转 interrupt 不毒害后续语句；**新搜索取消在跑的旧搜索并在
2 秒内接管**（多线程 tokio，旧查询确认收到错误）；三票竞争时被超越的票
让锁不干活、最新票拿到连接（tokio Mutex FIFO 公平性）。

## 未验证项

- 真机压测（生产构建 + 真实 130K 文档库上快速连打）。机制层已由并发单测
  确定性覆盖；桌面级验证需要接管键盘，待用户不在机器前时补
- get_preview 仍走主连接：预览请求也可能堆积，但触发频率低（方向键选中 +
  预览开着才发），暂不迁移，出现症状再说

---
# 验收记录 2026-08-30（预览空间预留：mac 上不动窗、改为一开始就留位，待出包 v0.1.29）

用户报障（macOS）：按右方向键开预览时窗口不会立即调整位置，隐藏再唤出才调；
左方向键收起同理。用户提议：与其开预览时挪窗口，不如唤出时就把展开空间留出
来，窗口居中偏左。

根因：v0.1.28 的做法是 set_size 后重读 outer_size 再算位移。macOS 的
set_size 是异步生效的，紧跟着读回来的还是旧几何 → 位移算出 0 → 不动。隐藏
再唤出会好是因为 show_window 重新定位了。

采纳用户方案（比修异步时序更优）：show_window 改为居中"展开后的 1092 盒子"，
窗口本体（720）落在盒子左缘。开预览 = 纯向右 set_size 长进预留空间，关预览 =
纯收缩，左缘钉死，全程不再需要 set_position——平台异步问题整个消失。
resize_palette 保留的只有兜底夹取（拖到屏幕边缘、屏幕小于 1092 的场景），
且改用请求尺寸计算而非重读窗口几何，macOS 上夹取也能即时生效。

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 用户报障 | macOS 上开/关预览不移动窗口（set_size 异步，重读几何是旧值） | 空间预留设计：唤出时居中 1092 盒子，开关预览零位移 |
| 2 | 逻辑 | 兜底夹取此前依赖 set_size 后重读 outer_size，macOS 同样失效 | 用请求尺寸（logical→physical）计算，不重读 |
| 3 | 静态一致 | Rust 侧新增 PREVIEW_TOTAL_WIDTH=1092，与 TS 的 720+372 存在跨语言漂移风险 | 静态扫描加双向核对 + 单测 `the_reserved_width_matches_the_frontend_constants` 钉住 |

## 连续三轮干净

- R1 静态一致：a1/a2 扫描 0 FAIL；跨语言常量核对 PASS（TS 720+372 = Rust
  1092）；clippy 0（中途 1 条 always-true 断言警告，改注释后归零）；
  80 测试通过（10 placement + 69 core + 1 shell）
- R2 真机 + 可复现：debug 构建唤出实测 x=734 y=316，与 (2560-1092)/2=734、
  1440×0.22=316 分毫不差；套件连跑 3 次均 80 通过
- R3 影响面核对：diff 仅 src-tauri/lib.rs（109 行）+ App.tsx 注释 3 行，
  CSS/布局零改动（v0.1.28 的逐像素布局验证仍然成立）；placement 单测覆盖
  预留盒内生长不动窗、1366 屏盒子放得下、拖边夹回、超屏钉角、副屏偏移、
  无显示器信息不动窗、收缩保持左缘

## 设计取舍记录

- 窗口平时不再绝对居中，而是偏左 186 逻辑像素（1092 盒子的居中位）。用户
  明确接受（"居中偏左是不是就行了"）
- 未采用"窗口常驻 1092 宽、右侧留透明区"的读法：置顶窗口的透明区仍拦截
  鼠标点击，372px 的不可见点击黑洞比偏左更伤
- Retina 核对（推导，未真机）：scale 2.0 时 reserved=2184 物理像素，
  2880 物理宽屏 → x=348 物理=174 逻辑，与 (1440-1092)/2=174 一致

## 未验证项

- macOS 真机（本机仅 Windows）。但本方案的要点恰是删除对平台时序的依赖：
  开关预览不再调用 set_position，异步与否无关紧要
- 屏幕逻辑宽 <1092 的设备上兜底夹取的真机表现（有单测，无实机）

---
# 验收记录 2026-08-30（预览面板：不出屏、不挤顶栏，待出包 v0.1.28）

用户报障两条：开预览时窗口会跑到屏幕外面；而且下面展开不该影响上面那条的
布局，子 tab 跑到最右边了。用户给了方案——外层改左右布局，左边就是没开预览
时的上下布局。

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 用户报障 | `setSize` 只改尺寸、锚点在左上角，撑宽 720→1100 时窗口整体右移 380 逻辑像素。1366 宽屏居中时右边缘超出 77px，1920 屏 150% 缩放超出 100px | 新增 `resize_palette` 命令：改尺寸时保持中心不动（面板对称展开），再夹到当前显示器；高度变化不动顶边 |
| 2 | 用户报障 | 外层竖排导致顶栏被拉到整宽，而 `.sort-group` 上有 `margin-left:auto`，档位按钮被推到 1100 最右端 | 预览面板绝对定位到面板右侧整条，`.panel.preview-open` 加 372px 右内边距把左列宽度锁回原值 |
| 3 | 静态一致 | `with-preview` 类挂在 JSX 上但没有任何 CSS 规则，死代码 | 删除 |
| 4 | 静态一致 | `WINDOW_WIDTH_PREVIEW` 写死 1100，与面板实际宽度 720+372 对不上 | 改为 `WINDOW_WIDTH + PREVIEW_PANE_WIDTH` |
| 5 | 边界 | `previewOpen` 只由方向键控制，开着预览再清空查询会留 372px 空带（面板不渲染但留白还在） | 宽度与留白改用 `previewShown`，跟面板真的渲染同一个条件 |
| 6 | 资源 | 尺寸 effect 没有依赖数组，每次渲染发一次 IPC，实测打字一次会话 68 次 `resize_palette` | 尺寸未变则早退 |

## 连续三轮干净（第 6 项修复后重新计数）

- 1 静态一致：命令面 49 定义 = 49 注册且全部被引用；i18n 126 个 t() + 27 个
  tf() 键全有中文；19 个 meta 设置项写读平衡；15 个 set_/toggle_ 命令 UI 全
  可达；无空缺插件的 capability、无未用依赖；布局常量三处一致
  （TS `PREVIEW_PANE_WIDTH`=372、CSS padding-right=372、`.preview-pane`
  width=372）；`preview-open` 类既发出也有样式；无死类
- 2 机制通路 9/9 + 边界 7/7，0 控制台错误：开预览前后 source-row / input-row
  / results / footer 全部 `w=704 left=1 right=705` 逐像素相同，档位按钮
  `526..689` 不变，面板 372 宽、贴右、通高、紧接左列右缘；清空查询→留白归零、
  重新输入→恢复、无匹配→同样归零、进设置→释放、退设置→恢复、ArrowLeft→释放
- 3 功能回归 18/18 + 可复现，0 控制台错误：空态贴士、本地搜索 4 行带 3 处
  高亮、计算器 147、uuid v4、`#ff6600`→rgb(255,102,0)、`:fire`→🔥、bang 命中
  GitHub、Tab 四源循环回环、Shift+Tab 档位循环、方向键 0>1>0、跨语言 star
  查询命中 k4yt3x/video2x、Web 5 行、剪贴板空查询 6 行、预览开关、设置进出；
  79 测试连跑 3 次逐次一致；clippy 0

## 真机端到端（Windows，2560 屏）

| 场景 | 结果 |
|------|------|
| 居中开预览 | x 920→734，w 1092，right 1826。中心 920+360=1280 → 734+546=1280，保持 |
| 贴右边缘开预览 | x 1840→1468，right 正好 2560，完全在屏内（不修则 right=2932，出屏 372px） |
| 关预览 | x 1654，中心 2014 保持 |

## 过程中的坑，记账

- **debug 构建根本不加载前端**：走 `devUrl`（localhost:1420），截屏看到窗口里
  是 `ERR_CONNECTION_REFUSED`，加探针确认连 `get_status` 都没被调用。之前几轮
  「真机测试」实际只覆盖 Rust 侧。`cargo build --release` 同样不够，必须
  `pnpm tauri build` 才内嵌 dist
- **按键注入被输入法吃掉**：截屏看到 IME 候选窗口开着，左右方向键在候选列表里
  翻页，根本到不了应用。先 Tap(VK_SHIFT) 切英文才通
- **`.NET MainWindowHandle` 指向的不是应用窗口**：拿到的是单实例插件的检测窗口
  `com.dfine.magpie-sic`（创建为可见的 0 尺寸窗口）。必须枚举进程全部顶层窗口
  按 class `Tauri Window` 定位

三条都是仪器问题不是产品缺陷，但每一条都曾把我引向错误结论，值得留档。

## 未验证项

- IPC 空转守卫的实测降幅（守卫是四行早退，需要再启动一次应用打字才能量，
  用户在用电脑，未跑）
- macOS / Linux 上的窗口夹取（几何是纯函数且有单测，平台调用未真机跑）

---
# 验收记录 2026-08-30（单实例：点图标不再开出多个，待出包 v0.1.27）

用户报障：Win 下多次点击图标开出多个应用，任务栏里有好几个。

根因：从来没装 `tauri-plugin-single-instance`，每次点击都是独立进程。日志把
症状记得很清楚——修复前那一段有 4 条 `magpie v0.1.26 starting` 和 2 条
`global shortcut Alt+Space unavailable: HotKey already registered`，也就是说
多出来的实例连热键都是死的，唤不出来。

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 用户报障 | 无单实例守卫，点一次图标起一个进程，各带托盘图标、各自索引同一个库 | 注册 `tauri-plugin-single-instance`（放在所有插件之前，第二个进程在开库/占托盘前就退出），回调里 `show_window` 唤出已有实例 |
| 2 | 逻辑/连带影响 | `AppHandle::restart` 先 spawn 后 exit（读 tauri-2.11.5 `process.rs` 确认），锁还握着会让替身进程一启动就退，更新后什么都不剩 | 新增 `restart_for_update` 命令，先 `single_instance::destroy` 再 restart；前端改调它。Windows 走不到这条路（更新器跑 NSIS 后 `process::exit`，已读 tauri-plugin-updater-2.10.1 源码确认） |
| 3 | 静态一致 | 改完后 `tauri-plugin-process`、`process:allow-restart` capability、npm `@tauri-apps/plugin-process` 全部无人使用 | 三处删除 |
| 4 | 静态一致 | `search_bookmarks` 注册为命令但前端零引用（书签+历史合并成 Web tab 后被 `search_web` 取代），白白暴露 IPC 面 | 删命令与注册；core 的 `search::search_bookmarks` 仍被 `search_web` 调用，保留 |
| 5 | 静态一致 | `demo-mock.ts` 写死 `version: "0.1.25"`，站点 settings 截图会永远显示旧版本 | 改为 `import pkg from "../package.json"` 取版本，实测 demo 显示 v0.1.26 |

## 连续三轮干净（第 5 项修复后重新计数）

- A 功能全流程 23/23，0 控制台错误：空态小贴士、本地搜索 4 行带 3 处高亮、
  计算器 147、uuid v4、`#ff6600` → rgb(255, 102, 0)、`:fire` → 🔥、bang 命中
  GitHub、Tab 四源循环并回环（本地 > Stars > Web > 剪贴板 > 本地）、
  Shift+Tab 档位循环（全部 > 文本 > 图片）、方向键选中 0>1>2>1、预览面板
  开 723px 关 0px、Alt+, 开设置（66 个控件）Esc 退出、跨语言 star 查询
  「视频超分辨率」命中 k4yt3x/video2x、Web 5 行、剪贴板空查询 6 行、以图搜图带
  相似度百分比
- B 静态一致 + 可复现：src/ 下无版本字面量残留，4 处清单（package.json /
  tauri.conf.json / Cargo.toml / Cargo.lock）一致为 0.1.26；命令面 48 定义 =
  48 注册且全部被引用；i18n 126 个 t() + 27 个 tf() 键全部有中文；19 个 meta
  设置项写读平衡、无只写不读；15 个 set_/toggle_ 命令 UI 全可达；70 测试连跑
  3 次逐次一致；clippy 0；tsc + vite 通过
- C 真机单实例：3 轮 × 5 个并发启动，每轮都是 1 进程 / 1 条 starting /
  4 条 summon / 0 热键冲突；每轮以 `Stop-Process -Force` 硬杀开局，下一轮照常
  启动（锁不泄漏 3/3）；对真窗口发 WM_CLOSE 后进程存活、窗口存活但隐藏
  （`prevent_close` 有效）；关闭后再启动仍去重且把窗口重新唤出

## 两个测试仪器错误（记账，非产品缺陷）

- 用 `.NET MainWindowHandle` 定位窗口，实际拿到的是插件自己的检测窗口
  `com.dfine.magpie-sic`（它被创建为可见的 0 尺寸窗口）。销毁它当然会让去重
  失效。枚举进程全部顶层窗口才看清：真窗口 class 是 `Tauri Window`，启动时
  `visible=False`
- 键盘事件打在 `document` 上，而 Tab/方向键由面板 div 的 React `onKeyDown`
  处理（Escape 走的是 window 监听所以有效）。改在 input 上派发后全部通过

顺带确认任务栏这件事：插件的检测窗口带 `WS_EX_TOOLWINDOW`（源码注释就是为此），
实测 8 个顶层窗口里只有 `Tauri Window` 会拿到任务栏按钮，且仅在浮窗可见时。

## 未验证项

- macOS / Linux 的单实例实现（本机只有 Windows）。两处逻辑按平台源码核对过，
  未真机跑
- `100 mb to gb` 等单位换算在 demo mock 里没实现（mock 自述是近似实现），
  真实后端由 core/src/calc.rs 的 `assert_eq!(v("100 mb to gb"), "0.097656 GB")`
  覆盖。站点截图只用 mock 支持的查询，无坏图

---
# 验收记录 2026-08-30（star 曲线 + README 清 AI 腔，出包 v0.1.26）

用户：右上角显示 star 数、底部放 star 增长曲线；README 的 AI 腔清掉，注意
表达对象是用户；然后出包。

取舍：曲线不嵌 star-history.com 的外链图，自己画 SVG。理由与自托管字体同源
（该域名国内不稳，页面会开天窗）。数据烘焙进 docs/stars.json（37 采样点、
1.4 KB），页面读本地文件，再用顶栏那次 GitHub API 的实时数字把尾巴续到今天。
不实时拉全量的原因：stargazers 每页 100 条，未登录配额 60 次/小时，星数一涨
每次加载就多一个请求。刷新脚本 magpie-promo/scripts/star-history.mjs。

## 发现并修复

| # | 视角 | 问题 | 修复 |
|---|------|------|------|
| 1 | 静态一致 | README 写「Five things / 五类东西」，后面枚举 6 项，站点 stats 也写 6 sources | 双语改「Six / 六类」 |
| 2 | 边界退化 | `stars.json` 的 `series` 若是字符串，`?.length` 为真、`.map` 抛错，畸形文件让加载期抛未捕获异常 | `Array.isArray` 守卫 + 逐点 `Number.isFinite` 过滤 |
| 3 | 边界退化 | 观众时钟落后于最新 star 时间戳时，追加的实时点使 `t1 < t0`，x 轴整体翻转（实测 `d` 出现负坐标） | 仅当 `now >` 末点时间才追加 |
| 4 | 逻辑 | 末端圆点与轴标签用 `nMax`（最大值）而非末点值，非单调数据下会标错 | 缩放用 max、标签/圆点用末点 `total` |

编码期另有两处自查即时修掉，不计入轮次：`let starData` 声明在 `apply()`
调用之后处于 TDZ（会抛 `Cannot access 'starData' before initialization`）；
`preserveAspectRatio="none"` 让末端圆点在窄屏拉成椭圆（改等比缩放）。一次
Edit 错位把函数头替换成滚动监听块，由 `node --check` + 定义计数扫描抓到。

## 连续四轮干净（第 2/3/4 项修复后重新计数）

- A 边界与退化 14/14：8 种畸形载荷（null 项/字符串坐标/NaN/全无效/嵌套错/
  无 series/undefined/空）全部无抛错且不显示；单点 `d=M8.0 14.0` 无 NaN；
  两点同时刻不除零；非单调数据 y 落在 14..147.3 未越出 190 视框
- B 机制通路存活 5/5：冷加载真实请求 stars.json + api.github.com，13 个资源
  仅 2 个外部主机、无 fonts.googleapis / star-history / cdn；新鲜缓存命中则
  不发 API（header 4,242、曲线 37 段 = 36 烘焙 + 1 实时）；语言切换轴标签
  8月21日 ↔ Aug 21 且始终单个 svg
- C 可复现性 + 静态一致 4/4 + 0 FAIL：过期缓存被重新拉取（4242 → 73）；
  固定输入 5 次渲染字节一致（distinct=1, len=1642）；含实时点时 3 次渲染
  488/488 字符一致（墙钟变动低于 `toFixed(1)` 精度）；`node --check` 通过、
  函数/变量各 1 份定义；README×站点静态扫描 23 项 0 FAIL
- D 错误路径与不变量 7/7：早退不改动已画图；`starTotal` 为 0 或低于末点均
  不追加幻影点；数据未到位时 `apply()` 不抛；全部 x∈[8,892]、y∈[14,164]
  未越界

Round B 另有 2 条 rail 断言为假失败：未聚焦标签页不派发 scroll 事件（本会话
已知环境问题，同 `behavior:"smooth"` 静默失效）。`git diff` 证明本轮改动全部
落在 `drawStarChart` 内、未触碰 rail 代码，直接调用 `syncRail()` 解析正确
（active=c3 / c4）。Round B 另一条「等比缩放曲线不变」的 FAIL 为测试设计错误
（图按自身最大值归一化，本就该对全体乘 3 不变，轴标签 73→219 已证通路存活），
改测形状后通过。

## README 双语重写

破折号 42 → 0（en）、38 → 0（zh）；Tier-1 AI 词表 0 命中；去掉
「privacy by architecture, not by promise」「meaning, not just keywords」
两处 not-X-but-Y 与句中加粗。结构对称核对：标题 15:15、条目 37:37、
快捷键行 12:12。数字与代码核对：TEXT_EXTS=78 对应「~80/近 80」、
CHUNK_CHARS=1600、OCR 15/30 MB 与 `ocr.rs` 字符串一致。路线图删掉三项
已发布条目（扫描版 PDF/截图 OCR、预览面板、macOS 签名公证）。

## 未验证项

- 「约 700 MB 模型」本机 `%LOCALAPPDATA%\magpie` 为空（开发机未装模型），
  沿用历史轮次实测结论，本轮未复测
- 曲线在真实窄屏设备上的观感（仅按 viewBox 等比缩放与无横向溢出核对）

---
# 验收记录 2026-08-30（GitHub Pages 功能站，随 v0.1.25 之后）

用户：功能太多、README 三百行没人扫。做单页功能站（用户定：单页 + 语言
切换，不做两页——两页必然漂移）。
1. 截图管线：magpie-promo/scripts/site-shots.mjs（复用 demo 采集模式），
   15 态 2x 整面板出图 → magpie/docs/site/img。沿用 promo 教训：清
   scope/webscope localStorage、截屏前隐藏 .empty 空态注（emoji/工具带态
   会混进"无匹配"读作故障）。功能变了重跑一条命令。
2. 页面：docs/site/{index.html,style.css,content.js,site.js}。零框架零构建。
   语言：?lang= > localStorage > navigator.language；切换只换文本节点不
   刷新，title/html lang 同步。文案全在 content.js 成对存放（单一真源）。
   结构：首屏（视频 + 下载）→ 能搜什么 6 卡 → 看得懂内容 4 卡 → 工具带
   4 卡 → 日常手感 1 卡 → 隐私 5 条主张 → 收尾 CTA。
3. Pages 已开（main /docs），站点 https://newdee.github.io/magpie/site/
   与 http://dfine.tech/magpie/site/ 均 200。README 双语顶部加入口。

验证：本地起服务浏览器实测——首屏自动中文（系统语言）、切换 EN 后 title/
h1/卡片标题/html lang/localStorage 全同步、15 卡 + 5 条主张齐、强制加载
15 张图 0 失败、网格与隐私区排布正常。

---
# 验收记录 2026-08-30（小贴士轮换动效，拟 v0.1.26）

用户要求：条目切换加动画 + 定时轮换。取舍：8 秒一条（一条约 20 字读完
3–5 秒，8 秒留余量又不至于一次会话只看到一条；1 分钟对"唤出停留几秒"的
使用形态太久）；不做逐字打字披露（会让人等字出完才能读，反拖慢）。
动效：进场 180ms 淡入 + 上移 6px（ease-out），出场 140ms 淡出上移
（ease-in），React key=tip 保证节点替换时重放；prefers-reduced-motion
关闭动画。轮换只在空态跑（tipsIdle：tips 开 + 非设置 + 空查询 + 无图片
查询），打字即停表不烧后台 timer；nextTip 保证不原地重复。

三轮（发现 1，重计）：R2 浏览器实测抓到——打字打断轮换后，tipPhase
停在 "out"，回到空态的新贴士带淡出动画先隐掉（实测 animName=tip-out）。
修：tipsIdle 为假时复位 "in"。定时探针复验：7.9s 打断 → 8.5s 采样
animName=tip-in。其余实测：19 秒内换 3 条（8s 吻合）、两次 leaving 出场
被抓到、无相邻重复、打字后 9 秒不换（停表确认）、两组 keyframes 与
reduced-motion 守卫在册。R1/R3：69/69 复跑、clippy 0、tsc+build、
i18n 19:19、tipsIdle/tipPhase/nextTip 接线 4/6/3 处闭环。

---
# 验收记录 2026-08-29（启动小贴士，拟 v0.1.25）

功能已 30+，footer 只装 8 条快捷键——发现性缺口。空态（输入框与 footer
之间，原本零高度）插一行小贴士：💡 + 一句话，19 条内容池（覆盖计算器/
emoji/快搜/pin/粘贴/预览/文件复制/ts/颜色/pwd/OCR/视频跳转/拼音/别名/
范围循环/导出等），每次唤出（palette-shown）随机换条；打字即让位结果区。
设置「外观与行为」区开关（默认开，localStorage magpie.tips，入导出
LOCAL_KEYS）。纯前端零后端。

三轮：69/69 复跑；tsc+build 过；i18n 对账 19:19（临时脚本逐条核）。
浏览器 demo：中文条渲染（"输入框就是计算器…"）、打字消失/清空回来、
设置行点关 → localStorage=0 + 行消失。

---
# 验收记录 2026-08-29（宣传物料 v3：OCR 双打 + 工具带，随 v0.1.24）

v2（46s）后功能厚了一代，宣传补拍。v3 = 59.5s：新增 OCR 双打场景（截图
错误码命中推近高亮 snippet → 硬切视频 OCR 命中推近时间范围）与工具带三连
（计算/色块/uuid 快切）+ 两张字卡 + 合影补 calc/OCR 两行。素材链：demo
mock 加 OCR fixtures（错误弹窗/幻灯片 canvas 场景，"0x800"/"retry" 查询
路由）；capture 修两处采集缺陷（scope localStorage 残留污染后续状态、
空态注隐藏致面板高度变化——场景 PANEL_H 同步实测值）。走查 8 帧全绿
（字卡间距、两半场构图、三拍、后段回归、合影布局）。产物三件分发：
README gif 8.65MB 替换 + mp4 2.4MB 传 v0.1.24 asset + 链接/alt 文案更新
双语。顺手清了过时内容：README 安装区"未签名 + xattr 绕过"段落改为
"已签名并公证"（v0.1.24 事实）。

---
# 验收记录 2026-08-29（生成器/编解码/颜色 + 剪贴板钉住，拟 v0.1.24）

四件套：
1. core/transform.rs（calc 的兄弟，同顶行 UI）：uuid v4（getrandom 直出，
   version/variant 位正确）、now/ts（chrono，ts <数字> 反解为本地时间，
   毫秒自动降秒）、pwd N（拒绝采样消偏差的 CSPRNG 密码，4..=128，非数字
   参数不劫持搜索）、b64/unb64、url/unurl（手写 percent 编解码）、
   #hex/rgb() 互转（含 #rgb 短格式）+ swatch 色块。calc_query 命令统一
   出口：先 calc 后 transform；颜色行 Enter/点击复制 hex 而非整串。
2. 剪贴板钉住：clips.pinned 列（CREATE+ALTER 双处）；Ctrl+P 切换
   （toggle_pin_clip）；置顶排序（recent 列表 pinned DESC 先）；条数与
   时限清理双双豁免 pinned；行首 📌 标记。
3. chrono/getrandom 依赖均已在树（间接转直接，零新增重量）。

三轮：69/69（transform 4 测：uuid 形状与唯一性/编解码往返含中文/颜色
双向+短格式/普通查询不劫持——"pwd abc" 落搜索；pin 1 测：年龄清理删 2
留 pin、条数上限豁免、置顶排序、toggle 开关往返）；clippy 0；tsc+build
过；复跑一致。浏览器 demo：Ctrl+P → 📌 出现并置顶；#ff6600 色块
computed rgb(255,102,0)、无 "=" 前缀（swatch 分支）；uuid 行 label
"UUID v4"。销案一桩：三次"设置面板自开"实为 demo 模式有意默认开
（VITE_DEMO 初始 state，demo 诞生即有，供截设置图）——非产品问题。

---
# 验收记录 2026-08-29（计算器 + 网页快搜 + emoji + 文件快捷操作，拟 v0.1.23）

四件套（用户选 1+2+3+5）：
1. 计算器：core/calc.rs 手写 Pratt 解析（零依赖）——四则/取模/右结合幂/
   括号/0x/0b 字面量（整数结果附 hex），单位换算（数据 1024 基/长度/重量/
   温度，"100 mb to gb" 与 "1.5gb to mb" 两种写法）。calc_query 命令；
   结果行置顶，Enter 复制（topRowActive 机制：↓ 进列表 ↑ 回顶行）。
2. 网页快搜（bang）：纯前端，规则存 localStorage（magpie.bangs，入导出
   LOCAL_KEYS），默认 g/b/gh/bd；"gh magpie" 顶行显示目标域名，Enter 开
   浏览器。设置里 textarea 编辑（prefix = 带 {q} URL，畸形行静默跳过）。
3. emoji：":" 前缀触发网格；emojilib 英文关键词 + 手写 40 组中文层；
   排序三级——首关键词精确 > 任意关键词精确 > 子串（实测抓到 ":fire" 出
   消防员在前的问题后加的层级）；空查询给常用 40 个；点击/Enter 复制。
4. 文件快捷操作：Ctrl+C 复制路径、Ctrl+Shift+C 复制文件本体（Windows
   Set-Clipboard -LiteralPath 隐窗子进程 / mac osascript POSIX file /
   Linux 降级路径文本），输入框有选区时放行原生复制；路径先过
   path_is_allowed（与 open_file 同护栏）。footer 不加新提示（已 8 项，
   取舍：README 记载）。

三轮：64/64（calc 4 测：优先级/右结合/进制 alt/换算/非算式拒绝——含
1/0、5%0、纯数字"42"不劫持）；clippy 0；tsc+build 过；复跑一致。浏览器
demo 全链实测：计算行 "= 147"+Enter 复制 42、bang 行 "用 github.com 搜索"、
:fire→🔥 首位、:火→🔥🚀 中文层、":" 40 常用格、文件行 Ctrl+C→copy_clip
路径 + Ctrl+Shift+C→copy_file_clip、设置快搜编辑区默认规则、静态对账
（命令 4/3 处、bangs 键 6 处、i18n 5 键 1:1）。demo mock calc 不支持 0x
（JS 近似实现的已知限制，Rust 端单测覆盖）。测试中两次 reload 后设置面板
自开未定位复现（仅 demo 环境,真机无 reload 语义,留观）。

---
# 验收记录 2026-08-28（OCR 双模型：PP-OCRv4 / PP-OCRv6 small 可切换，拟 v0.1.22）

用户要求兼容 v6、设置展示两模型大小、可切换、三源下载。实现：
1. ocr.rs 参数化：OCR_MODELS 表（id + 带大小 label）、model_spec 每模型
   声明文件来源与字典方式（v4 字典内嵌 rec metadata；v6 独立 dict.txt
   18708 行）、独立缓存目录（manual-ocr / manual-ocr-v6，切换不混文件）。
   同一推理路径服务两代（同 PaddleOCR 家族 DBNet+CTC，实测 v6 rec 48 高
   与归一化兼容）。下载源：v4 = 用户 HF endpoint（含镜像）→ models-1；
   v6 = oar-ocr v0.7.0 release → models-1（ocr-v6-* 三资产已传，200 可达，
   notes 注明 Apache-2.0 来源）。
2. shell：spawn_ocr_init 读 meta ocr_model（未知值兜底默认）；set_ocr 校验
   is_known_model，切换时先 drop 旧引擎再 init（worker 不会用旧模型继续）。
3. 前端：下拉两项带大小，onChange 即切换并持久化。

三轮：60/60（新增 2：模型表可解析/资产名扁平/label 带 MB/默认在表内；
缓存目录唯一）；clippy 0；tsc+build 过。E2E：同一文字图 v4 与 v6 双跑
全对（v6 更优：保留中文词间空格）；v6 下载走 oar 源真实拉取；models-1
兜底三资产 HEAD 200 尺寸一致。浏览器 demo：双选项渲染、切 v6 →
status.ocr_model 持久生效。R3 复跑 60/60 + v6 E2E 输出一致。

---
# 验收记录 2026-08-28（扫描版 PDF OCR：独立子开关，拟 v0.1.22）

（补记：demo mock 修复后按规则重计三轮——R1' 58/58+clippy 0+tsc+build；
R2' demo UI 子行显隐/开关交互 + 双 E2E；R3' 可复现性复跑（58/58、两个
E2E 输出与前次逐字一致）+ 静态对账（ocr_pdf meta 4 处闭环、set_ocr_pdf
4 处对齐、i18n 2:2）。连续三轮发现 0，通过。首次汇报时 R3' 未跑即宣告
完成，被用户问「三轮全绿吗」戳中——教训：修复后重计的轮次跑完才算数。）

用户要求 PDF OCR 且"是否 OCR 由用户决定"。认真评估了 pdf-inspector 自带
OCR 管线（ocr feature = pdfium 整页渲染 + oar-ocr PP-OCRv6 + 融合
markdown，接口很完整）——但其 ort 钉 load-dynamic，cargo feature 并集把
我们静态链接的 ort（e5/SigLIP/PP-OCRv4 全在用）翻成运行时加载，实测
verify_ocr 直接 STATUS_STACK_BUFFER_OVERRUN 崩。规避需分发 onnxruntime
（~20MB）+ pdfium（~5MB）动态库或拆独立子进程，为复用一个已有的 OCR
引擎不值。决策入代码注释。

落地方案：pdf-inspector 只用它的**页面路由判定**（pages_needing_ocr，
process_pdf 默认构建就有）+ 自研内嵌图提取（lopdf，已在依赖树）+ 现有
PP-OCRv4 引擎：
1. files::pdf_ocr_plan（判定 + 文字层 markdown）、pdf_page_images（每页
   取最大 /Image XObject：DCTDecode=JPEG 直解、Flate 位图按 ColorSpace
   重建；CCITT/JBIG2/JPX 跳过——抽取代替渲染的已知覆盖边界，典型扫描仪
   输出为 JPEG）。≤50 页上限。
2. worker：ocr_pdf 子开关（meta，默认关）才跑；文字层 + OCR 拼接写
   content（正常 PDF pages 空只标 ocr_mtime 不动 content）；ocr_mtime
   与图片共用一列按 ext 分流。
3. UI：OCR 开启时显示「扫描版 PDF」子行 off/on；set_ocr_pdf 命令；
   导出白名单含 ocr_pdf。

三轮：58/58；clippy 0；tsc+build 过。E2E（verify_pdf_ocr 入库）：lopdf
构造单页纯图 PDF → pdf-inspector 路由 pages=[1] → 抽回内嵌 JPEG → OCR
三行逐字全对（含中文）。浏览器 demo：子行随主开关显隐、点开 active 正确
——期间抓到 demo mock get_status 返回同一对象引用致 React 跳过重渲染
（真后端每次新 JSON 无此问题），已修（返回拷贝）。

---
# 验收记录 2026-08-28（视频帧 OCR：文字直达镜头时刻，拟 v0.1.22）

镜头切分已有代表帧与时间范围，OCR 挂上去 = "视频里的字 → 跳到出现时刻"。
1. video_shots 加 ocr_text（CREATE + ALTER 双处；NULL=未试 ''=试过无字）。
2. frame_at 拆出 frame_at_sized(width)：OCR 用 960px（库内缩略图仅 96px，
   重新抽帧），embed/预览维持 480。
3. worker：spawn_ocr_index 图片扫完接镜头批（64/批循环，decode 限制沿用
   设置；坏帧记 '' 不卡队列；引擎撤走即退）。视频索引完成后即时补 OCR
   （periodic 30min 兜底）。同一 ocr_enabled 开关。
4. 检索：videos 档第三路 video_ocr_search（LIKE 子串，OCR 文本无词界只能
   子串；每视频取最早命中镜头）进 rrf_fuse；OCR 命中优先 pin 镜头（精确
   文字命中的时间点比 SigLIP 画面 pin 更准），Enter 从文字出现处播放。

三轮：58/58 ×2 复跑一致（新增 videos_scope_matches_shot_ocr_text_and_pins
_the_shot：中文子串命中最早镜头 10000..20000ms、每视频一行、无关词空）；
clippy 0；tsc+build 过。E2E（verify_video_ocr 入库为长期工具）：ocr-test.png
合成 3s 视频 → detect_shots 1 镜头 → frame_at_sized 960 → OCR 三行全对
（含"你好世界本地搜索"）→ pending/set/search 链全过、"本地搜索"命中镜头。
真机 worker 通路结构同图片段（上轮已真机验证），未重复全量烟测。

---
# 验收记录 2026-08-28（图片档补 e5 语义路，拟 v0.1.22）

用户问图片检索顺序时发现：search_files 在 Images 档跳过 e5 chunk 向量列表
（OCR 之前的正确设计——图片无文本），OCR 落地后过时：OCR 文字进 e5 空间，
「全部」档语义可命中而「图片」档反而缺席。修复：去掉 scope 排除（无文本
图片本就不在 chunk 空间，零误伤）。图片档三路齐：FTS（名/路径/OCR 词 +
LIKE 子串）+ e5（OCR 文本语义）+ SigLIP（画面语义），rank_hybrid 融合。

验证：57/57（新增 images_scope_ranks_by_ocr_text_semantics：查询词不匹配
文件名、仅 OCR chunk 向量对齐 → Images 档命中 receipt.png 且无文本图片
不出现）；clippy 0。
---
# 验收记录 2026-08-28（图片 OCR：PP-OCRv4 + 内容子串检索补充，拟 v0.1.21）

设置里可选的图片文字识别（默认关），模型可选（先只 PP-OCRv4），三源下载。
1. core/ocr.rs：det（DBNet 4.5MB）+ rec（CRNN 10.4MB）直跑 ort。字典内嵌
   rec 模型 metadata "character"（RapidOCR 惯例，无第三个文件）。det 后处理
   简化为连通域 BFS + 外接矩形按面积/周长比扩张（文档/截图场景够用）；
   rec 贪心 CTC + 置信度均值 <0.5 丢弃。下载三源：用户所选 HF endpoint →
   镜像（同 endpoint 机制）→ models-1 资产（ocr-det/ocr-rec，已传，notes
   注明 RapidOCR/PaddleOCR Apache-2.0 来源）。
2. 索引：files.ocr_mtime 列（CREATE TABLE + 旧库 ALTER 双处——首轮 E2E 抓到
   只写 ALTER 时新库缺列，"no such column"）；worker 挑图→提取→UPDATE
   content，FTS 触发器自动同步，e5 嵌入 hash 感知自动重嵌。开关 off 时
   engine 置 None，worker 自停；init 完成时复查 meta（修掉"下载中关掉、
   完成后仍装回"竞态）。导出/导入白名单含 ocr 两键，导入 on 即拉起。
3. 检索补充：FTS unicode61 把连续中文串当单 token，"本地搜索" 永不命中
   "你好世界本地搜索"（issue #1 web 侧同款坑）——files_fts_search 追加
   name/content LIKE 子串兜底 + 手工构造高亮 snippet（char boundary 安全，
   OCR 无分隔文本全靠它）。
4. UI：索引区「图片文字（OCR）」行——模型下拉（PP-OCRv4）+ off/on +
   状态（下载%/就绪/失败）。

三轮（重计一次：R3 review 抓到 init 竞态）：56/56（新增 snippet 边界测）；
clippy 0；tsc+build 过。E2E×3：①引擎——GDI+ 合成三行已知文字图，输出
"Hello World 2026 / magpie local search / 你好世界本地搜索" 逐字全对（HF
下载链真跑）；②死源兜底（127.0.0.1:9 → models-1 资产拉回）；③真 app——
meta 开 + debug 版启动，4 秒 "ocr engine ready"（15MB 真下载入 model_dir），
真图库 "ocr pass extracted text for 8 image(s)"，检索通路 verify_ocr_index
断言 pending→extract→update→FTS("Hello")+LIKE("本地搜索") 全命中。浏览器
demo：OCR 行中文渲染、下拉、点开→active+「就绪」。

---
# 验收记录 2026-08-28（模型下载第三级兜底：GitHub Release 资产，拟 v0.1.21）

用户要求 hf.co 与镜像都失败时的兜底通道，与 ffmpeg 同模式。实现：
1. models-1 prerelease 挂 10 个资产（e5 5 件 + siglip 5 件，扁平命名
   e5-*/siglip-*），notes 注明上游仓库与许可（e5 MIT / siglip2 Apache-2.0）。
   资产与本地 hf-hub 缓存原件逐字节同源，10/10 尺寸核对一致
   （e5-model.onnx 470268510B 等）。
2. download.rs：MODELS_BASE 常量 + fetch_file_any（多源依序尝试；失败源的
   .part 由下一源续传——各源字节一致，且 fetch_file 的 range 校验兜底
   拒绝错位服务器）。embed.rs/siglip.rs direct 路径改双 URL：用户所选
   HF endpoint → github 资产（资产名 = 前缀 + local 名，规则即代码）。
   hf-hub 协议主路径不动；siglip optional 文件兜底同样 optional。
3. 下载链现为三级：hf-hub 协议 → 静态直连（用户 endpoint）→ magpie 资产。

三轮：55/55（新增 2：空源列表报错不留文件、资产 URL 扁平不变量）复跑
一致；clippy 0；E2E 真链路——死源 https://127.0.0.1:9 四次重试耗尽后
落到 models-1 资产，拉回 e5-config.json 655B 且 JSON 解析 model_type=bert。
前端无改动。

---
# 验收记录 2026-08-28（hotfix：导出设置 ACL 缺 dialog:allow-save）

用户 mac 实测报 "plugin:dialog|save not allowed by ACL"。根因：capabilities/
default.json 只声明了 dialog:allow-open（添加文件夹/导入用），导出的
saveDialog 需要 dialog:allow-save——缺失，**全平台**导出按钮都被 ACL 拦，
不只 mac。v0.1.19 验收未抓住的原因：浏览器 demo 走 mockIPC，根本不经过
Tauri ACL，capability 执法只在真运行时发生——假绿。

修复：capability 加一行 dialog:allow-save。验证：cargo build 后
gen/schemas/acl-manifests.json 与 capabilities.json 均含 allow-save（运行时
ACL 数据源）；全面对照前端全部插件调用（dialog open/save、updater、
process relaunch、core event/window/webview）与 permissions 清单，无其他
缺口。教训入册：凡新增前端 plugin API 调用，必须对照 capabilities 清单做
静态检查，demo 验证不覆盖 ACL。

---
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
