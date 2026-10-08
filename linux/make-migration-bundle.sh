#!/usr/bin/env bash
# Voice VibeCoding — Linux：打「迁移包」，拷到另一台 Ubuntu 电脑上两条命令装好（不需要 sudo）
#
#   bash linux/make-migration-bundle.sh              # 带上本机已下载的识别模型（约 1.2GB）
#   bash linux/make-migration-bundle.sh --no-models  # 不带模型（约 17MB，新电脑上在界面里下载）
#
# 产物：dist-linux/voice-vibecoding-migrate-<版本>.tar 和 .tar.sha256
#   .deb + install.sh（一键安装）+ README.txt + data/（全局设置、按键映射、识别模型）
# 先要有打好的 .deb：bash linux/package-deb.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_SRC="$HOME/.local/share/com.remote-bridge-hub.app"
WITH_MODELS=1
[ "${1:-}" = "--no-models" ] && WITH_MODELS=0
cd "$ROOT"

DEB="$(ls -t dist-linux/*.deb 2>/dev/null | head -1 || true)"
[ -n "$DEB" ] || { echo "!!! dist-linux/ 里没有 .deb，请先打包：bash linux/package-deb.sh"; exit 1; }
VERSION="$(dpkg-deb --field "$DEB" Version)"
NAME="voice-vibecoding-migrate-$VERSION"
OUT="$ROOT/dist-linux/$NAME.tar"

# 在临时目录里用符号链接搭好目录结构，tar -h 打包时换成真实文件（不用先复制 1GB 模型）
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
B="$STAGE/$NAME"
mkdir -p "$B/data"
ln -s "$(realpath "$DEB")" "$B/$(basename "$DEB")"
install -m 755 linux/migrate/install.sh "$B/install.sh"
sed -e "s|@VERSION@|$VERSION|g" -e "s|@NAME@|$NAME|g" -e "s|@DEB@|$(basename "$DEB")|g" \
  linux/migrate/README.txt > "$B/README.txt"

echo ">>> 打包 $NAME"
for f in settings.json xiaomi.json linux.json; do
  if [ -f "$DATA_SRC/$f" ]; then
    ln -s "$DATA_SRC/$f" "$B/data/$f"
    echo "    设置 $f"
  fi
done
if [ "$WITH_MODELS" = 1 ] && [ -d "$DATA_SRC/models" ]; then
  mkdir -p "$B/data/models"
  for m in "$DATA_SRC/models"/*/; do
    [ -d "$m" ] || continue
    ln -s "${m%/}" "$B/data/models/$(basename "$m")"
    echo "    模型 $(basename "$m")（$(du -sh "$m" | cut -f1)）"
  done
fi

# 模型已是压缩过的 onnx，tar 不再压缩；*.part 是没下完的临时文件，test_wavs 是模型自带的样例音频
tar -C "$STAGE" -chf "$OUT" --exclude='*.part' --exclude='test_wavs' "$NAME"
(cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")

echo
echo ">>> 完成：$OUT（$(du -h "$OUT" | cut -f1)）"
echo "    校验：$OUT.sha256"
echo "    拷到新电脑后："
echo "      tar -xf $NAME.tar"
echo "      bash $NAME/install.sh"
