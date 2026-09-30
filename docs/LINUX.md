# Voice VibeCoding · Linux 版

在 Linux（已在 Ubuntu 24.04 / GNOME / X11 上验证）运行本软件：小米蓝牙遥控器 2 Pro 的按键映射 + 语音输入。

界面、按键映射配置（`xiaomi.json`）与 Windows 版完全相同；底层换成 Linux 原生组件：

| 功能 | Windows 版 | Linux 版 |
|---|---|---|
| 蓝牙 / ATVV 语音通道 | WinRT BLE | BlueZ（D-Bus） |
| 读取遥控器按键 | HID Tap（Frida 注入 WUDFHost）+ 低级键盘钩子 | evdev 独占遥控器输入节点（EVIOCGRAB） |
| 注入快捷键 | WinUHid 虚拟键盘 / SendInput | uinput 虚拟键盘 |
| 语音送到哪里 | VB-CABLE 虚拟声卡 → 输入法听写 | **本地离线识别直接上屏**（默认），或 PipeWire 虚拟麦克风 |
| 录快捷键 | 低级键盘钩子 | X11 键盘独占（XGrabKeyboard） |
| 开机自启 | 注册表 Run | `~/.config/autostart` |

> 为什么默认「本地识别上屏」：Linux 上没有微信/豆包/千问输入法的「按住说话」，
> 所以改为在本机离线识别（sherpa-onnx，CPU 运行），松手后把文字粘贴到当前输入框。
> 也可以切回与 Windows 相同的「虚拟麦克风 + 快捷键」模式。

### 识别模型

| 模型 | 适合 | 出字速度（5 秒语音） | 内存 | 下载 |
|---|---|---|---|---|
| **Qwen3-ASR 0.6B**（默认） | 中英混说、技术词（Claude Code、cargo build、PR、tokio…） | 约 1 秒 | 约 1.9GB | 约 990MB |
| SenseVoice Small | 纯中文 / 追求瞬间出字、内存紧张 | 约 0.1 秒 | 约 330MB | 约 240MB |

在 16 句中英混说的编程口述上实测（遥控器音质模拟）：Qwen3-ASR 字错误率 5.7%、英文术语认对 86%；
SenseVoice 字错误率 14.3%、英文术语认对 64%（常见错误如 cargo build → cargobi、PR → p2）。

- **空闲释放内存**：默认 10 分钟没说话就释放 Qwen3-ASR 占的内存；再按语音键时在你说话的同时后台加载（约 2～3 秒），
  说得很短时第一句会稍慢一点。可改成「不释放（常驻内存）」。
- **识别后替换**：每行一条 `原文 => 替换为`（英文不区分大小写），固定纠正总是认错的词，默认带一条 `Cloud Code => Claude Code`。
- **热词**（仅 Qwen3-ASR，可选）：逗号分隔的专有名词能提高命中，但写得多了会让 container、server 这类普通英文词被意译成中文，
  所以默认留空，只建议写几个总认错的名字；能用「识别后替换」解决的优先用替换。

## 一、安装

推荐打成 .deb 安装（装在 /usr，apt 管理升级与卸载，自带 udev 规则与 hwdb）：

```bash
# 1. 一次性：编译依赖（需要 sudo）
sudo bash linux/setup-system.sh

# 2. 构建并打包（不需要 sudo）→ dist-linux/voice-vibe-coding_<版本>_amd64.deb
bash linux/package-deb.sh

# 3. 安装 / 升级（会要 sudo 密码）；软件正在运行的话会换成新版本重启
bash linux/install-deb.sh
```

装好后在应用列表里搜索「Voice VibeCoding」。构建需要 Rust（rustup）与 Node.js 18+；
打好的 .deb 可以拷到其它 Ubuntu 24.04 机器上直接 `bash linux/install-deb.sh xxx.deb`（或 `sudo apt install ./xxx.deb`）。

包里装了什么：

| 位置 | 内容 |
|---|---|
| `/usr/bin/voice-vibecoding` | 程序本体（识别引擎静态链接在内） |
| `/usr/share/applications/Voice VibeCoding.desktop` | 应用列表入口 |
| `/usr/lib/udev/rules.d/72-voice-vibecoding.rules` | 当前登录用户可访问 /dev/uinput 与遥控器节点；遥控器电源键不触发关机 |
| `/usr/lib/udev/hwdb.d/72-voice-vibecoding.hwdb` | 遥控器语音键/电源键的原生键码改成无效键，断线重连瞬间漏给桌面也不会刷新网页/弹关机菜单 |
| `/usr/share/voice-vibecoding/bt-dualboot.py` | 双系统共用蓝牙配对的工具（见下文） |

卸载：`sudo apt remove voice-vibe-coding`（包名由 Tauri 按产品名生成，程序本体叫 `voice-vibecoding`）。设置、识别模型与日志在 `~/.local/share/com.remote-bridge-hub.app/`，卸载不会删除。

开发调试时也可以不打包，直接装到 `~/.local`：`bash linux/install.sh`（与 .deb 二选一，
`install-deb.sh` 会自动清掉 `~/.local` 这份）。

语音识别模型在小米页面「语音输出」卡片里选择并点「下载模型」（Qwen3-ASR 约 990MB，SenseVoice 约 240MB），
来源为 hf-mirror.com / huggingface.co（多线程下载，自动校验 sha256），存放在
`~/.local/share/com.remote-bridge-hub.app/models/`。

## 二、配对遥控器

1. 打开系统「设置 → 蓝牙」。
2. 同时长按遥控器 **主页** + **菜单** 键约 3 秒，指示灯闪烁即进入配对模式。
3. 在列表里点「MI RC」完成配对。
4. 回到 Voice VibeCoding：状态栏「蓝牙桥接」显示「已连接」、「遥控按键」显示「已接管」即可。

遥控器休眠或超出范围会断开，按任意键即自动回连：按键节点在重建的瞬间就会被重新接管，
ATVV 语音通道约 1～2 秒内恢复（界面短暂显示「等待遥控器连接」属正常现象）。

### 双系统（Windows + Linux）共用一套配对

蓝牙遥控器对同一台电脑只记得一套配对密钥：在一个系统里重新配对，另一个系统就连不上。
解决办法是**最后在 Windows 里配对**，再把 Windows 的密钥导入 Linux，两边就共用同一套：

1. 重启进 Windows，在「设置 → 蓝牙和其他设备 → 添加设备」里配对遥控器
   （同时长按「主页」+「菜单」约 3 秒），确认在 Windows 下能用。
2. 在 Windows 里选**重启**（不要休眠 / 快速启动关机），从开机菜单进 Ubuntu。
3. 先别按遥控器，在终端运行：
   ```bash
   sudo python3 /usr/share/voice-vibecoding/bt-dualboot.py import-windows
   ```
   （没装 .deb 时用源码里的 `linux/bt-dualboot.py`；需要 `sudo apt install libhivex-bin`）
   它只读挂载 Windows 分区，读出注册表里的配对密钥写进 BlueZ（原文件自动备份，可一键还原）。
4. 按遥控器任意键，Linux 下即可直接连上；之后两个系统切换都不用再配对。

`sudo python3 /usr/share/voice-vibecoding/bt-dualboot.py inspect` 可只读对比两边的配对记录（不显示密钥原文）。

## 三、使用

- **开机自启**：「设置 → 开机自启」写入 `~/.config/autostart/`；配合「启动后最小化到托盘」可登录后静默运行，
  从顶栏托盘图标菜单「打开状态」、或在应用列表里再点一次「Voice VibeCoding」打开窗口。
- **按键**：与 Windows 版相同，在「按键映射」里点遥控器上的键即可改映射；「录入」快捷键时会独占键盘，
  系统快捷键（如 Super）不会被触发。
- **语音（本地识别模式）**：按住遥控器语音键说话，松开后文字出现在当前光标处。
  「语音输出」卡片可调：识别模型、上屏方式（默认 Shift+Insert，终端也能用）、粘贴后恢复剪贴板、去掉句末句号、
  空闲释放内存、热词、识别后替换（SenseVoice 另有识别语言）。
- **语音（虚拟麦克风模式）**：在需要语音输入的软件里把麦克风选为「Voice VibeCoding 遥控器麦克风」；
  按住语音键时软件会同时按住「语音」键映射的快捷键。

## 四、排障

| 现象 | 处理 |
|---|---|
| 状态栏「虚拟键盘：无权限」 | udev 规则没生效：装 .deb（或从源码安装时跑 `sudo bash linux/setup-system.sh`）后仍不行就重新登录一次 |
| 「未配对遥控器」 | 见上文「配对遥控器」；也可在「语音输出」卡片点「蓝牙诊断」查看 BlueZ 看到的设备 |
| 「ATVV 未连接」 | 点「修复 ATVV 连接」（会软重启蓝牙会话并重新订阅语音通道） |
| 按键没反应 | 查看日志（小米页右侧「日志」）；`voice-vibecoding diag-bluetooth` 可在终端列出蓝牙设备 |
| 识别出的文字没粘贴进去 | 换一种「上屏方式」（部分终端需要 Ctrl+Shift+V）；Wayland 会话下剪贴板/按键注入受限，建议用 X11 会话 |
| 某个词总是认错 | 在「识别后替换」里加一行 `错的 => 对的`；仍不行再把它加进「热词」（仅 Qwen3-ASR） |
| 内存紧张 / 想要瞬间出字 | 「识别模型」切到 SenseVoice Small |

日志：`~/.local/share/com.remote-bridge-hub.app/logs/app.log`

### 无实物联调

```bash
# 伪造一只「MI RC」发按键（走与真遥控器相同的 evdev → 映射 → uinput 链路）
voice-vibecoding simulate-remote up down ok back

# 把一段 16kHz 单声道 WAV 当作「按住语音键说话」喂给正在运行的实例（ATVV 编解码 → 识别 → 上屏）
voice-vibecoding --simulate-voice /path/to/16k-mono.wav
```

## 五、实现位置

Linux 后端都在 `src-tauri/src/linux/`；与 Windows 共用的状态机（ATVV 协议、ADPCM 解码、按键映射、
快捷键录入引擎、配置）保持原样复用。Windows 版行为未改动。
