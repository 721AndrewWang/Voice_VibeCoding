#!/usr/bin/env bash
# Voice VibeCoding — Linux：从源码打包成 .deb（不需要 sudo）
#
#   bash linux/package-deb.sh    # → dist-linux/voice-vibe-coding_<版本>_<架构>.deb
#
# 包名 voice-vibe-coding 由 Tauri 按产品名「Voice VibeCoding」生成；程序本体是 /usr/bin/voice-vibecoding
#
# 首次构建前先装一次编译依赖：sudo bash linux/setup-system.sh
# 打好后安装 / 升级：bash linux/install-deb.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT/dist-linux"
cd "$ROOT"

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

# 程序里的源码路径（panic 位置等）不带构建机的 home 目录和用户名
CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_DIR="${RUSTUP_HOME:-$HOME/.rustup}"
REMAP="--remap-path-prefix=$CARGO_DIR=/cargo"$'\x1f'"--remap-path-prefix=$RUSTUP_DIR=/rustup"$'\x1f'"--remap-path-prefix=$ROOT=/build"
export CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS:+$CARGO_ENCODED_RUSTFLAGS$'\x1f'}$REMAP"

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCH="$(dpkg --print-architecture)"

echo ">>> [1/3] 前端依赖"
[ -d node_modules ] || npm ci --no-audit --no-fund

echo ">>> [2/3] 构建 release 并打包 .deb（版本 $VERSION）"
npx tauri build --bundles deb

# Tauri 按产品名命名（带空格），这里改成 Debian 惯例的「包名_版本_架构.deb」
BUNDLED="$(ls -t "src-tauri/target/release/bundle/deb/"*"_${VERSION}_${ARCH}.deb" | head -1)"
PKG="$(dpkg-deb --field "$BUNDLED" Package)"
mkdir -p "$OUT_DIR"
DEB="$OUT_DIR/${PKG}_${VERSION}_${ARCH}.deb"
cp -f "$BUNDLED" "$DEB"

echo ">>> [3/3] 包信息"
dpkg-deb --field "$DEB" Package Version Architecture Depends Recommends Installed-Size
echo "    主要文件："
# 去掉前 5 列（权限 属主 大小 日期 时间），保留可能带空格的路径
dpkg-deb --contents "$DEB" | sed -E 's/^([^ ]+ +){5}//; s|^\./|/|; s|^([^/])|/\1|' |
  grep -E '^/usr/(bin/.|lib/udev/[^/]+/.|share/applications/.|share/voice-vibecoding/.)' | sed 's/^/      /' || true

echo
echo ">>> 完成：$DEB（$(du -h "$DEB" | cut -f1)）"
echo "    安装 / 升级：bash linux/install-deb.sh"
