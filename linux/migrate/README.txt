Voice VibeCoding（Linux 版）迁移包 @VERSION@
==========================================

小米蓝牙遥控器 2 Pro：按键映射成任意快捷键；按住语音键说话，松手后在本机离线识别
（中英混说），文字直接粘贴到当前输入框。

一、对新电脑的要求
  - Ubuntu 24.04 LTS，64 位 x86。22.04 装不上（要在那台电脑上从源码编译）；
    25.10 及以后的 GNOME 已没有 Xorg 会话，没有验证过。
  - 建议用「Ubuntu on Xorg」会话登录：登录界面点用户名后，右下角齿轮里选。
    Wayland 下按键映射一般能用，但录快捷键、文字上屏没有验证过。
  - 内存 16GB 用默认的 Qwen3-ASR 没问题；8GB 建议装好后在软件里切到 SenseVoice。
  - 安装时要联网：apt 会自动补齐 WebKit 等依赖。

二、安装（两条命令，过程中会要一次 sudo 密码）
  tar -xf @NAME@.tar
  bash @NAME@/install.sh

  安装脚本只会新装软件包，不会删除任何已装的包（apt 带 --no-remove）。
  新电脑上已有的设置会先备份成 .bak-时间 再覆盖。

三、包里有什么
  @DEB@
                        软件本体（含 udev 规则、hwdb、双系统共用蓝牙配对工具）
  install.sh            一键安装
  data/settings.json    全局设置（开机自启、启动进托盘等）
  data/xiaomi.json      按键映射
  data/models/          语音识别模型（没有这个目录的话，装好后在软件里下载）

四、装好以后
  1. 配对遥控器：设置 → 蓝牙，同时长按遥控器「主页」+「菜单」约 3 秒，
     指示灯闪烁后点列表里的「MI RC」。
     遥控器很可能只记得一台电脑：在新电脑配对后，回原来那台可能要重新配对。
  2. 软件在顶栏右上角的托盘里；在应用列表里点「Voice VibeCoding」可打开窗口。
  3. 松手后出字太慢：在小米页「语音输出」里把识别模型切到 SenseVoice（几乎瞬间出字，
     英文技术词会差一些）。
  4. 双系统（同一台电脑上的 Windows 和 Ubuntu）共用一套配对：
     sudo python3 /usr/share/voice-vibecoding/bt-dualboot.py inspect

五、卸载
  sudo apt remove voice-vibe-coding
  设置、识别模型、日志在 ~/.local/share/com.remote-bridge-hub.app，卸载不会删除。

详细说明：/usr/share/doc/voice-vibe-coding/LINUX.md（装好后）
