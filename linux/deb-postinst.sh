#!/bin/sh
# .deb 安装/升级后：让 udev 规则（/dev/uinput 与遥控器节点的访问权限）和 hwdb（语音键/电源键
# 漏给桌面时不触发动作）立即生效，不用重启或重新插拔
set -e
if command -v systemd-hwdb >/dev/null 2>&1; then
  systemd-hwdb update || true
fi
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules || true
  udevadm trigger --action=change --subsystem-match=misc --sysname-match=uinput || true
  udevadm trigger --action=change --subsystem-match=input || true
  udevadm trigger --action=change --subsystem-match=hidraw || true
fi
exit 0
