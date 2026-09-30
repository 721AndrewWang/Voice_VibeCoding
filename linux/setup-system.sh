#!/usr/bin/env bash
# Voice VibeCoding — Linux 一次性系统配置（需要 root，可重复执行）
#
#   sudo bash linux/setup-system.sh
#
# 做三件事：
#   1. apt 安装编译依赖（Tauri 2 的 WebKitGTK / 托盘 / D-Bus 开发包等）
#   2. 安装 udev 规则：当前登录用户可访问 /dev/uinput 与小米遥控器的 evdev/hidraw 节点；
#      以及 hwdb：遥控器语音键(F5)/电源键漏给桌面时不触发任何动作
#   3. 重新加载 udev 规则 / hwdb 并对现有设备生效
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  exec sudo bash "$0" "$@"
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULES_SRC="$HERE/72-voice-vibecoding.rules"
RULES_DST=/etc/udev/rules.d/72-voice-vibecoding.rules
HWDB_SRC="$HERE/72-voice-vibecoding.hwdb"
HWDB_DST=/etc/udev/hwdb.d/72-voice-vibecoding.hwdb

PKGS=(
  pkg-config
  libwebkit2gtk-4.1-dev
  libssl-dev
  libayatana-appindicator3-dev
  librsvg2-dev
  libxdo-dev
  libdbus-1-dev
  libhivex-bin
)

echo ">>> [1/3] 安装编译依赖：${PKGS[*]}"
export DEBIAN_FRONTEND=noninteractive

# 本机软件源没有启用 noble-updates，但已装的 GTK 运行库（libgtk-3-0t64 3.24.41-4ubuntu1.3）
# 来自 noble-updates，所以 libgtk-3-dev 只能从 noble-updates 取同版本。
# 这里临时加上 noble-updates 并把优先级压到 100（不会借机升级任何其它包），装完即删除。
TMP_SRC=/etc/apt/sources.list.d/zz-voice-vibecoding-tmp.sources
TMP_PREF=/etc/apt/preferences.d/zz-voice-vibecoding-tmp.pref
cleanup_tmp_source() {
  rm -f "$TMP_SRC" "$TMP_PREF"
}
trap cleanup_tmp_source EXIT
NEED_UPDATES=0
if ! apt-cache policy libgtk-3-dev 2>/dev/null | grep -q "$(dpkg-query -W -f='${Version}' libgtk-3-0t64 2>/dev/null)"; then
  NEED_UPDATES=1
  cat > "$TMP_SRC" <<'EOF'
Types: deb
URIs: http://cn.archive.ubuntu.com/ubuntu/
Suites: noble-updates
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF
  cat > "$TMP_PREF" <<'EOF'
Package: *
Pin: release a=noble-updates
Pin-Priority: 100
EOF
fi

apt-get update -q
if [ "$NEED_UPDATES" = 1 ]; then
  apt-get install -y --no-install-recommends "${PKGS[@]}" libgtk-3-dev/noble-updates
  cleanup_tmp_source
  apt-get update -q >/dev/null 2>&1 || true
else
  apt-get install -y --no-install-recommends "${PKGS[@]}"
fi

if dpkg-query -W -f='${Status}' voice-vibe-coding 2>/dev/null | grep -q "install ok installed"; then
  # .deb 自带 /usr/lib/udev 下的规则与 hwdb，再往 /etc 放一份会盖住包里的新版本
  echo ">>> [2/3] 已安装 voice-vibe-coding 软件包（自带 udev 规则与 hwdb），跳过"
else
  echo ">>> [2/3] 安装 udev 规则 -> $RULES_DST"
  install -m 0644 "$RULES_SRC" "$RULES_DST"
  echo "          安装 hwdb -> $HWDB_DST"
  install -D -m 0644 "$HWDB_SRC" "$HWDB_DST"
fi
systemd-hwdb update

echo ">>> [3/3] 重新加载 udev 规则"
udevadm control --reload-rules
udevadm trigger --action=change --subsystem-match=misc --sysname-match=uinput
udevadm trigger --action=change --subsystem-match=input
udevadm trigger --action=change --subsystem-match=hidraw
udevadm settle || true

echo
echo ">>> 完成。"
getfacl -p /dev/uinput 2>/dev/null | sed -n '1,8p' || ls -l /dev/uinput
echo
echo ">>> 下一步：bash linux/package-deb.sh && bash linux/install-deb.sh（或开发用 bash linux/install.sh）"
