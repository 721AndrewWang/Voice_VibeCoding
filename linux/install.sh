#!/usr/bin/env bash
# Voice VibeCoding — Linux：从源码构建并安装到当前用户（~/.local，不需要 sudo），适合开发调试
#
#   bash linux/install.sh            # 构建 + 安装
#   bash linux/install.sh --no-build # 只安装已构建好的二进制
#
# 日常使用建议装 .deb：bash linux/package-deb.sh && bash linux/install-deb.sh
#
# 首次使用前需先跑一次（需要 sudo，装编译依赖与 udev 规则）：
#   sudo bash linux/setup-system.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
APP_ID="voice-vibecoding"
BUILD=1
[ "${1:-}" = "--no-build" ] && BUILD=0

cd "$ROOT"

if [ "$BUILD" = 1 ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    # shellcheck disable=SC1091
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  fi
  command -v cargo >/dev/null 2>&1 || {
    echo "!!! 没有找到 Rust 工具链：curl https://sh.rustup.rs -sSf | sh"; exit 1; }
  command -v npm >/dev/null 2>&1 || { echo "!!! 没有找到 Node.js / npm"; exit 1; }
  pkg-config --exists webkit2gtk-4.1 dbus-1 ayatana-appindicator3-0.1 || {
    echo "!!! 缺少系统开发包，请先运行：sudo bash linux/setup-system.sh"; exit 1; }

  # sherpa-onnx 预编译库：有本地缓存就不用再从 GitHub 下载
  CACHE="$HOME/.cache/voice-vibecoding/sherpa-onnx-archives"
  [ -d "$CACHE" ] && export SHERPA_ONNX_ARCHIVE_DIR="$CACHE"

  echo ">>> [1/3] 前端依赖"
  [ -d node_modules ] || npm ci --no-audit --no-fund
  echo ">>> [2/3] 构建（release）"
  npx tauri build --no-bundle
fi

# tauri.linux.conf.json 设了 mainBinaryName，构建后二进制叫 voice-vibecoding（旧构建是 remote-bridge-hub）
BIN_SRC=""
for b in "$ROOT/src-tauri/target/release/$APP_ID" "$ROOT/src-tauri/target/release/remote-bridge-hub"; do
  if [ -x "$b" ] && { [ -z "$BIN_SRC" ] || [ "$b" -nt "$BIN_SRC" ]; }; then BIN_SRC="$b"; fi
done
[ -n "$BIN_SRC" ] || { echo "!!! 找不到构建好的二进制，请先构建"; exit 1; }

if dpkg-query -W -f='${Status}' voice-vibe-coding 2>/dev/null | grep -q "install ok installed"; then
  echo "!!! 注意：已经装了 voice-vibe-coding 的 .deb。~/.local 版会和它并存，命令行与应用列表会优先用 ~/.local 这份；"
  echo "    想回到 .deb 版本，运行 bash linux/install-deb.sh（会清掉 ~/.local 这份）。"
fi

echo ">>> [3/3] 安装到 $PREFIX"
install -Dm755 "$BIN_SRC" "$PREFIX/bin/$APP_ID"
install -Dm644 src-tauri/icons/32x32.png "$PREFIX/share/icons/hicolor/32x32/apps/$APP_ID.png"
install -Dm644 src-tauri/icons/128x128.png "$PREFIX/share/icons/hicolor/128x128/apps/$APP_ID.png"
install -Dm644 src-tauri/icons/128x128@2x.png "$PREFIX/share/icons/hicolor/256x256/apps/$APP_ID.png"

DESKTOP="$PREFIX/share/applications/$APP_ID.desktop"
mkdir -p "$(dirname "$DESKTOP")"
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=Voice VibeCoding
GenericName=遥控器语音编程
Comment=小米蓝牙遥控器 2 Pro：按键映射 + 语音输入（本地识别上屏）
Exec="$PREFIX/bin/$APP_ID"
Icon=$APP_ID
Terminal=false
Categories=Utility;Accessibility;
Keywords=remote;voice;xiaomi;遥控器;语音;
StartupWMClass=voice-vibecoding
EOF
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo
echo ">>> 完成：$PREFIX/bin/$APP_ID"
echo "    在应用列表里搜索「Voice VibeCoding」即可打开（已在托盘运行时会直接弹出窗口）。"
