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
