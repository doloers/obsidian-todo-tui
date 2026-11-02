# obsidian-todo-tui

Obsidian 待办核对器（Rust + ratatui TUI）。把 vault 里**已经做完但一直挂着 `- [ ]`** 的历史任务
快速勾成 `- [x]`，直接写回 Markdown —— 写前整份备份、可连续撤销。

配色与 [`power-tui`](../power-tui/) 同一套**电致发光黄**（256 色索引一致，`c` 键在琥珀 / 电纸黄之间切）。

## 为什么需要它

日志里的任务常年不勾，Obsidian 里那条 `tasks where !completed` 的 dataview 会把两年前的项也列出来。
一条条手改太慢，而「批量改文件」又容易改坏 —— 所以做成：

- **看清再勾**：按文件分组、标「距今 N 天」，一眼分辨哪些是陈年挂账；
- **改得安全**：写前逐行校验内容（同步/编辑过就跳过并报出），写前整份备份，`u` 一键回滚；
- **不搬动内容**：勾掉的任务留在原日志里，靠 dataview 的 `!completed` 自动消失。

## 安装

```bash
git clone https://github.com/doloers/obsidian-todo-tui.git
cd obsidian-todo-tui
./install.sh            # 编译 release，装到 ~/.local/bin，并写桌面入口（fuzzel 可搜「待办核对」）
```

依赖：Rust 工具链 + 终端 256 色。

> 国内网络拉 crates.io 慢的话，可选用 rsproxy 镜像（只影响本项目，不动全局 `~/.cargo`）：
> ```bash
> mkdir -p .cargo && cp cargo-mirror.toml.example .cargo/config.toml
> ```

## 用法

```bash
obsidian-todo-tui                      # 有上次的 vault 就直接进；没有则进选择界面
obsidian-todo-tui --vault ~/Notes/vault
obsidian-todo-tui --list               # 只在终端列清单（脚本可用）
obsidian-todo-tui --date 2026-11-02    # 指定写入的 completion 日期
obsidian-todo-tui --all                # 连文件名含「模板」的笔记一起扫
```

### 按键

底栏只放常用键，**全部按键按 `Ctrl+G`**（或 `?`）唤出。

| 键 | 作用 |
|---|---|
| `j` / `k` `↑↓` | 上下移动（左栏选文件，右栏选条目） |
| `h` / `l`、`Tab` | 文件栏 ↔ 条目栏 |
| `g` / `G`、`Ctrl+D` / `Ctrl+U` | 首 / 尾、翻页 |
| `空格` | 勾选当前条目（在文件栏 = 整个文件） |
| `a` / `A` | 本文件全选 / 全部全选（受过滤影响） |
| `/` | 按关键字过滤（企业名、项目名…），`Esc` 清除 |
| `d` | 设置 completion 日期（默认今天） |
| `Enter` | 写入 vault（带确认弹窗） |
| `u` | 撤销最近一次写入（可连续回滚） |
| `r` / `v` / `c` / `o` | 重新扫描 / 换 vault / 切配色 / 在 Obsidian 打开当前笔记 |
| `Ctrl+G`、`?` / `q` | 全部按键 / 退出（有未写入勾选时二次确认） |

### 写入规则

```
- [ ] 某任务            →  - [x] 某任务 [completion:: 2026-11-02]
> - [ ] 引用式任务      →  > - [x] 引用式任务 [completion:: 2026-11-02]
```

- 行内若已有 `completion::` 不重复追加；CRLF 行尾原样保留。
- 已经是 `[x]` 的、内容与扫描时不一致的，一律**跳过并列在「跳过」面板**里，不会硬写。

## 安全设计

| 机制 | 说明 |
|---|---|
| 备份 | 写前把整份原文拷到 `~/.local/state/obsidian-todo-tui/backups/<时间戳>/`，附 `manifest.tsv` |
| 撤销 | `u` 恢复最近一次未撤销的备份；持续标记，可连续回滚多次；也可手动拷回 |
| 校验 | 逐行核对「行号 + 正文」仍与扫描时一致，防止同步盘/编辑器改动后错行 |
| 幂等 | 同一行重复提交被识别为「已经是完成状态」跳过 |
| 只读扫描 | 扫描阶段不写任何文件；未按 `Enter` 确认不会有任何改动 |

## 配置

`~/.config/obsidian-todo-tui/config.toml`（首次选择 vault 后自动生成，可手改）：

```toml
vault = "~/Notes/work-vault"
last_date = ""
theme = "amber"                       # amber | paper
recent = ["…"]                        # 选择界面里的「最近使用」
exclude_dirs = [".obsidian", ".trash", "06-帮助"]
exclude_substrings = ["模板"]          # 文件名含这些词的笔记不当待办源（模板骨架）
```

## 目录

```
src/
├── main.rs        CLI 解析、终端初始化/恢复（含 panic 兜底）、--list
├── app.rs         状态机与按键：文件/条目两栏、勾选、确认、写回、撤销
├── ui.rs          ratatui 渲染：布局、折行（按显示宽度）、弹窗、vault 选择
├── theme.rs       电致发光黄 / 电纸黄两套配色（与 power-tui 同索引）
├── scan.rs        vault 扫描与任务行解析（含 mark_done 改写规则）+ 单元测试
├── writeback.rs   备份 / 写盘 / 撤销（含幂等与内容漂移的测试）
├── config.rs      零依赖 key=value 配置
└── date.rs        零依赖日期（civil-from-days）
```

依赖仅 `ratatui` + `crossterm` + `unicode-width`；`cargo test` 覆盖日期往返、任务行解析、
改写规则、URI 编码、写入→撤销往返（含幂等与漂移）。

## 已知限制

- TUI 里输入中文需要终端支持 IME preedit；多数终端里搜索框宜输 ASCII，或先用 `--list` + grep 定位文件。
- 只处理 Markdown 任务行（`- [ ]`），不解析 Obsidian Tasks 插件的 emoji 元数据（📅 等会原样保留）。
