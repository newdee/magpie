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
