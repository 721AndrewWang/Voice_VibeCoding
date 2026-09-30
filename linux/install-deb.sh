#!/usr/bin/env bash
# Voice VibeCoding — Linux：安装 / 升级 .deb（用普通用户运行，装包时会要 sudo 密码）
#
#   bash linux/install-deb.sh                  # 装 dist-linux/ 里最新打好的包
#   bash linux/install-deb.sh path/to/xxx.deb  # 装指定的包
#
# 除了装包，还会：
#   1. 删掉 setup-system.sh 装在 /etc/udev 的同名规则 / hwdb（与包里内容相同时），以后由软件包统一管理
#   2. 删掉 linux/install.sh 装在 ~/.local 的旧版本，免得应用列表里出现两个、命令行先找到旧的
#   3. 软件正在运行的话，换成新版本重启（开机自启项会自动改为指向 /usr/bin/voice-vibecoding）
# 卸载：sudo apt remove voice-vibe-coding（设置、识别模型、日志在 ~/.local/share/com.remote-bridge-hub.app，不会删）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="voice-vibecoding"

if [ "$(id -u)" -eq 0 ]; then
  echo "!!! 请用普通用户运行（需要时脚本自己会调用 sudo）：bash linux/install-deb.sh"
  exit 1
fi

if [ $# -ge 1 ]; then
  DEB="$(realpath "$1")"
else
  DEB="$(ls -t "$ROOT/dist-linux/"*.deb 2>/dev/null | head -1 || true)"
fi
if [ -z "$DEB" ] || [ ! -f "$DEB" ]; then
  echo "!!! 没有找到 .deb，请先打包：bash linux/package-deb.sh"
  exit 1
fi
PKG="$(dpkg-deb --field "$DEB" Package)"

# apt 用 _apt 用户读取本地包，家目录它读不到：先复制到临时目录
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
cp "$DEB" "$TMP_DIR/"
chmod 755 "$TMP_DIR"
chmod 644 "$TMP_DIR/$(basename "$DEB")"

echo ">>> [1/4] 安装 $(basename "$DEB")（需要 sudo 密码）"
sudo apt-get install -y --reinstall "$TMP_DIR/$(basename "$DEB")"

echo ">>> [2/4] udev 规则 / hwdb 改由软件包管理"
# 去掉注释和空行再比较：只是说明文字不同也算同一份
same_rules() { cmp -s <(grep -v '^[[:space:]]*\(#\|$\)' "$1") <(grep -v '^[[:space:]]*\(#\|$\)' "$2"); }
changed=0
for f in rules.d/72-voice-vibecoding.rules hwdb.d/72-voice-vibecoding.hwdb; do
  old="/etc/udev/$f" new="/usr/lib/udev/$f"
  [ -f "$old" ] || continue
  if same_rules "$old" "$new"; then
    sudo rm -f "$old"
    echo "    已移除 $old（包里自带相同内容的 $new）"
    changed=1
  else
    echo "    保留 $old：和包里的不一样（可能手动改过），它会覆盖 $new"
  fi
done
if [ "$changed" = 1 ]; then
  sudo systemd-hwdb update
  sudo udevadm control --reload-rules
fi

echo ">>> [3/4] 清理旧的 ~/.local 安装"
removed=0
for f in "$HOME/.local/bin/$APP_ID" \
         "$HOME/.local/share/applications/$APP_ID.desktop" \
         "$HOME/.local/share/icons/hicolor/32x32/apps/$APP_ID.png" \
         "$HOME/.local/share/icons/hicolor/128x128/apps/$APP_ID.png" \
         "$HOME/.local/share/icons/hicolor/256x256/apps/$APP_ID.png"; do
  if [ -e "$f" ]; then
    rm -f "$f"
    echo "    已删除 $f"
    removed=1
  fi
done
if [ "$removed" = 1 ]; then
  command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
else
  echo "    没有旧安装"
fi

echo ">>> [4/4] 启动新版本"
# 只认 argv[0] 是 voice-vibecoding 或以 /voice-vibecoding 结尾的进程（从应用列表启动时 argv[0] 不带路径；不会误伤本脚本）
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
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  setsid -f "/usr/bin/$APP_ID" --minimized >/dev/null 2>&1
  echo "    已在托盘启动：/usr/bin/$APP_ID"
else
  echo "    当前不是桌面会话，未自动启动；登录桌面后在应用列表里打开「Voice VibeCoding」"
fi

echo
echo ">>> 完成：$(dpkg-query -W -f='${Package} ${Version}' "$PKG")"
echo "    卸载：sudo apt remove $PKG"
