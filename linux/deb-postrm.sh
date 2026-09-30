#!/bin/sh
# .deb 卸载后：udev 规则 / hwdb 已随包删除，重新加载让遥控器恢复系统默认行为。
# 用户数据（~/.local/share/com.remote-bridge-hub.app：设置、识别模型、日志）不动。
set -e
case "$1" in
  remove|purge)
    if command -v systemd-hwdb >/dev/null 2>&1; then
      systemd-hwdb update || true
    fi
    if command -v udevadm >/dev/null 2>&1; then
      udevadm control --reload-rules || true
    fi
    ;;
esac
exit 0
