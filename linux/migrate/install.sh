#!/usr/bin/env bash
# Voice VibeCoding 迁移包 — 一键安装（在新电脑上用普通用户运行，装软件包时会要一次 sudo 密码）
#
#   bash install.sh
#
# 会做四件事：
#   1. 检查系统：Ubuntu 24.04 及以上（glibc ≥ 2.39）、架构与包一致、提示桌面会话类型
#   2. 安装 .deb：apt 自动补齐依赖；带 --no-remove，绝不会连带删除任何已装的包
#   3. 拷入按键映射、设置和识别模型（原有的配置先备份成 .bak-时间；模型大小一致的文件跳过）
#   4. 在托盘启动软件（设置里开着开机自启的话，自启项会随之写好）
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ID="voice-vibecoding"
DATA_DST="$HOME/.local/share/com.remote-bridge-hub.app"

say() { printf '%s\n' "$*"; }
die() { printf '!!! %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -ne 0 ] || die "请用普通用户运行：bash install.sh（需要时脚本自己会调用 sudo）"

DEB="$(ls "$HERE"/*.deb 2>/dev/null | head -1 || true)"
[ -n "$DEB" ] || die "迁移包里没有找到 .deb"
PKG="$(dpkg-deb --field "$DEB" Package)"
VER="$(dpkg-deb --field "$DEB" Version)"
DEB_ARCH="$(dpkg-deb --field "$DEB" Architecture)"

say ">>> [1/4] 检查系统"
# shellcheck disable=SC1091
. /etc/os-release
say "    系统：${PRETTY_NAME:-未知}"
ARCH="$(dpkg --print-architecture)"
[ "$ARCH" = "$DEB_ARCH" ] || die "这个包是 $DEB_ARCH 的，本机是 $ARCH，需要在本机从源码编译"
GLIBC="$(getconf GNU_LIBC_VERSION | awk '{print $2}')"
dpkg --compare-versions "$GLIBC" ge 2.39 ||
  die "需要 Ubuntu 24.04 或更新（glibc ≥ 2.39），本机 glibc 是 $GLIBC，需要在本机从源码编译"
case "${XDG_SESSION_TYPE:-}" in
  x11) say "    桌面会话：X11（已验证）" ;;
  wayland)
    say "    注意：当前是 Wayland 会话。按键映射一般能用，但录快捷键、文字上屏没有验证过。"
    say "          建议注销，在登录界面点用户名后，右下角齿轮里选「Ubuntu on Xorg」再登录。" ;;
  *) say "    桌面会话：${XDG_SESSION_TYPE:-未知}" ;;
esac

say ">>> [2/4] 安装 $PKG $VER（需要 sudo 密码）"
# apt 用 _apt 用户读取本地包，家目录它读不到：先复制到临时目录
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
cp "$DEB" "$TMP_DIR/"
chmod 755 "$TMP_DIR"
chmod 644 "$TMP_DIR/$(basename "$DEB")"
# --no-remove：依赖若要求删掉任何已装的包就直接中止（-y 不会替你确认删除）
sudo apt-get install -y --no-remove --reinstall "$TMP_DIR/$(basename "$DEB")"

# 已经在运行的话先停掉，再换配置（只认 argv[0] 是 voice-vibecoding 或以 /voice-vibecoding 结尾的进程）
PIDS="$(ps -eo pid,args | awk -v n="$APP_ID" '$2 == n || substr($2, length($2) - length(n)) == "/" n { print $1 }')"
if [ -n "$PIDS" ]; then
  # shellcheck disable=SC2086
  kill -TERM $PIDS 2>/dev/null || true
  for _ in $(seq 50); do
    # shellcheck disable=SC2086
    kill -0 $PIDS 2>/dev/null || break
    sleep 0.2
  done
fi

say ">>> [3/4] 拷入按键映射、设置和识别模型 → $DATA_DST"
mkdir -p "$DATA_DST"
STAMP="$(date +%Y%m%d-%H%M%S)"
for f in settings.json xiaomi.json linux.json; do
  src="$HERE/data/$f"
  dst="$DATA_DST/$f"
  [ -f "$src" ] || continue
  if [ -f "$dst" ] && ! cmp -s "$src" "$dst"; then
    cp -p "$dst" "$dst.bak-$STAMP"
    say "    原来的 $f 已备份为 $f.bak-$STAMP"
  fi
  cp "$src" "$dst"
  say "    $f"
done
if [ -d "$HERE/data/models" ]; then
  for m in "$HERE/data/models"/*/; do
    [ -d "$m" ] || continue
    name="$(basename "$m")"
    # 逐个文件拷，大小一样的跳过（重复运行很快）；先写 .tmp 再改名，中途打断也不会留下半个文件
    while IFS= read -r -d '' s; do
      rel="${s#"$m"}"
      d="$DATA_DST/models/$name/$rel"
      if [ -f "$d" ] && [ "$(stat -c %s "$s")" = "$(stat -c %s "$d")" ]; then
        continue
      fi
      mkdir -p "$(dirname "$d")"
      cp "$s" "$d.tmp"
      mv "$d.tmp" "$d"
    done < <(find "$m" -type f -print0)
    say "    识别模型 $name（$(du -sh "$DATA_DST/models/$name" | cut -f1)）"
  done
else
  say "    （迁移包里没带识别模型：打开软件后在小米页「语音输出」里点「下载模型」）"
fi

say ">>> [4/4] 启动"
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  setsid -f "/usr/bin/$APP_ID" --minimized >/dev/null 2>&1
  say "    已在托盘启动（顶栏右上角的图标）；在应用列表里点「Voice VibeCoding」可打开窗口"
else
  say "    当前不是桌面会话，未自动启动；登录桌面后在应用列表里打开「Voice VibeCoding」"
fi

say
say ">>> 完成：$(dpkg-query -W -f='${Package} ${Version}' "$PKG")"
say "    下一步：配对遥控器。打开「设置 → 蓝牙」，同时长按遥控器「主页」+「菜单」约 3 秒，"
say "    指示灯闪烁后点列表里的「MI RC」。遥控器很可能只记得一台电脑，回原来那台可能要重新配对。"
say "    卸载：sudo apt remove $PKG（设置、模型、日志留在 $DATA_DST）"
