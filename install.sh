#!/usr/bin/env bash
# 编译并安装 obsidian-todo-tui（不需要 root）
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

cd "$DIR"

echo "==> 编译 release"
cargo build --release

echo "==> 装到 $BIN_DIR/obsidian-todo-tui"
mkdir -p "$BIN_DIR"
install -m 0755 target/release/obsidian-todo-tui "$BIN_DIR/obsidian-todo-tui"

echo "==> 桌面入口（fuzzel 搜「待办核对」）"
mkdir -p "$APPS_DIR"
cat > "$APPS_DIR/obsidian-todo.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Obsidian 待办核对
Comment=勾选已完成的历史待办并写回 vault（带备份与撤销）
Exec=foot -T "Obsidian 待办核对" -e "$BIN_DIR/obsidian-todo-tui"
Terminal=false
Categories=Office;Utility;
Keywords=obsidian;todo;task;待办;
EOF
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS_DIR" 2>/dev/null || true

echo "==> 自检"
"$BIN_DIR/obsidian-todo-tui" --help | head -3
echo
echo "完成。启动：obsidian-todo-tui   （或在 fuzzel 里搜「待办核对」）"
