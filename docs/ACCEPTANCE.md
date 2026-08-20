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
