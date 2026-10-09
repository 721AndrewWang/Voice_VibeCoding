/**
 * 后端发来的中文文字 → 英文。后端逻辑和前端判断（比如 label === "语音识别"）仍用中文原文，
 * 这里只在显示时翻译。
 *
 * EXACT：整句完全相同时替换；RULES：带变量的句子按正则替换，按顺序尝试，第一条命中即返回。
 * RULES 里用函数的条目会对捕获到的子句再翻译一次（后端常把一个中文错误嵌进另一句里）。
 *
 * 来源：src-tauri/src/linux/*.rs、ipc/commands.rs、ipc/platform_cmds.rs、bridges/mod.rs、
 * bridges/xiaomi/{key_log,input_session,hid_report_tap,connect,conflict_guard}.rs、
 * app_update.rs、file_download.rs、startup_env.rs、config/manager.rs。
 */
export const EXACT: Record<string, string> = {
  // ── 主机状态栏：列名（Linux host_status.rs / Windows commands.rs）──
  "语音识别": "Speech",
  "虚拟麦克风": "Virtual mic",
  "虚拟键盘": "Keyboard",
  "遥控按键": "Remote keys",
  "蓝牙桥接": "Bluetooth",
  "虚拟声卡": "Audio cable",
  "语音路由": "Voice routing",
  "按键桥接": "Key bridge",

  // ── 主机状态栏：状态值 ──
  "已加载": "Loaded",
  "下载中": "Downloading",
  "加载中": "Loading",
  "待加载": "Not loaded",
  "未下载模型": "No model",
  "已创建": "Created",
  "未创建": "Not created",
  "已就绪": "Ready",
  "未就绪": "Not ready",
  "无权限": "No permission",
  "已接管": "Captured",
  "未连接": "Disconnected",
  "已连接": "Connected",
  "连接中...": "Connecting...",
  "等待遥控器": "Waiting for remote",
  "未启动": "Not started",
  "已安装": "Installed",
  "未检测到": "Not detected",
  "运行中": "Running",
  "已停止": "Stopped",
  "监听中": "Listening",

  // ── 主机状态栏：总状态 status_text ──
  "运行正常": "All systems normal",
  "桥接未运行": "Bridge not running",
  "未配对遥控器": "Remote not paired",
  "虚拟键盘不可用": "Virtual keyboard unavailable",
  "虚拟键盘未就绪": "Virtual keyboard not ready",
  "等待遥控器连接": "Waiting for remote",
  "按键未接管": "Keys not captured",
  "ATVV 未连接": "ATVV not connected",
  "语音识别模型未下载": "Speech model not downloaded",
  "语音输出未就绪": "Voice output not ready",
  "语音环境未就绪": "Voice environment not ready",
  "语音路由未就绪": "Voice routing not ready",
  "部分服务异常": "Some services have problems",
  // 前端拼的 ATVV 修复状态
  "ATVV 已修复": "ATVV repaired",
  "ATVV 修复未完成": "ATVV repair incomplete",
  "ATVV 修复已取消": "ATVV repair cancelled",
  "已取消修复": "Repair cancelled",

  // ── 主机状态栏：detail ──
  "可点「重启桥接」或打开日志检查。": "Click “Restart bridge” or open the log to check.",
  "请在系统「设置 → 蓝牙」里配对「MI RC」：同时长按遥控器「主页」+「菜单」键约 3 秒进入配对模式。":
    "Pair “MI RC” in system Settings → Bluetooth: hold Home + Menu on the remote for about 3 seconds to enter pairing mode.",
  "遥控器闲置会休眠断开，按一下任意键即可唤醒回连。":
    "The remote sleeps when idle. Press any key to wake it and reconnect.",
  "语音专用通道未就绪，按住语音键收不到声音。可点「修复 ATVV 连接」。":
    "The voice channel is not ready, so holding the voice key records nothing. Click “Repair ATVV connection”.",
  "本地识别需要 SenseVoice 模型（约 240MB）。请在下方「语音输出」里点「下载模型」。":
    "Local recognition needs the SenseVoice model (about 240 MB). Click “Download model” under “Voice output” below.",
  "请稍候或点「重启桥接」。": "Please wait, or click “Restart bridge”.",
  "无法打开 /dev/uinput": "Cannot open /dev/uinput",
  "语音唤醒需要 WinUHid（硬件级按键）。可点「修复虚拟键盘」自动安装内嵌驱动（需管理员确认）。":
    "Voice wake needs WinUHid (hardware-level keys). Click “Repair virtual keyboard” to install the bundled driver (requires admin approval).",
  "语音专用通道未就绪。按住语音键时「音频信号」可能无绿色波动，并可能触发系统 F5。可点「修复 ATVV 连接」。":
    "The voice channel is not ready. Holding the voice key may show no audio signal and may trigger F5. Click “Repair ATVV connection”.",
  "未检测到 VB-CABLE。可点「修复虚拟声卡」安装或修复。":
    "VB-CABLE not detected. Click “Repair virtual audio cable” to install or repair it.",
  "可点「重启桥接」或「修复虚拟声卡」后重试。":
    "Click “Restart bridge” or “Repair virtual audio cable”, then try again.",
  "可点「重启桥接」或「修复虚拟声卡」。": "Click “Restart bridge” or “Repair virtual audio cable”.",

  // ── 设备类型 / 按键名（bridges/mod.rs、key_log.rs、config）──
  "小米遥控器": "Xiaomi remote",
  "T1 遥控器": "T1 remote",
  "汉王 V60 语音笔": "Hanvon V60 voice pen",
  "电源": "Power",
  "音量+": "Vol+",
  "音量-": "Vol−",
  "上": "Up",
  "下": "Down",
  "左": "Left",
  "右": "Right",
  "确定": "OK",
  "返回": "Back",
  "主页": "Home",
  "菜单": "Menu",
  "语音": "Voice",
  "静音": "Mute",
  "未知": "Unknown",
  "删除": "Delete",
  "鼠标": "Mouse",
  "麦克风": "Microphone",
  "上翻页": "Page up",
  "下翻页": "Page down",

  // ── 状态日志（xiaomi-key 事件的 message）──
  "ATVV 语音通道已订阅": "ATVV voice channel subscribed",
  "ATVV 语音通道已恢复": "ATVV voice channel restored",
  "ATVV 语音键/音频已订阅": "ATVV voice key/audio subscribed",
  "ATVV 语音键/音频已订阅（FromId）": "ATVV voice key/audio subscribed (FromId)",
  "ATVV 语音键/音频已订阅（后台重试成功）": "ATVV voice key/audio subscribed (background retry succeeded)",
  "ATVV 麦克风音频已订阅 → VB-CABLE": "ATVV mic audio subscribed → VB-CABLE",
  "重连后仍无 ATVV，且仍有桥接占用进程。请结束占用后再点「修复 ATVV 连接」。":
    "Still no ATVV after reconnecting, and another bridge process is still holding it. End that process, then click “Repair ATVV connection” again.",
  "已重连但仍未订阅 ATVV（未见端口占用）。可再试一次，或检查蓝牙配对后重试。":
    "Reconnected but ATVV is still not subscribed (no port conflict found). Try again, or check Bluetooth pairing and retry.",
  "修复 ATVV 前检测到其它遥控桥接进程占用端口或 BLE，请先结束后再继续。":
    "Another remote bridge process is holding the port or BLE. End it before repairing ATVV.",
  "按键监听已启动（HID-Tap 返回/音量 + ATVV 语音/音频）":
    "Key listener started (HID Tap Back/Volume + ATVV voice/audio)",
  "HID Tap 已按配置禁用": "HID Tap disabled by config",
  "HID Tap 未启动：返回/音量键不可用（请确认 Frida Gadget 资源）":
    "HID Tap not started: Back/Volume keys unavailable (check the Frida Gadget files)",
  "HID Tap 不可用：缺少 Frida Gadget（返回/音量键依赖此通道）":
    "HID Tap unavailable: Frida Gadget missing (Back/Volume keys need it)",
  "HID Tap 已启动（首次需允许 UAC，以捕获返回/音量键）":
    "HID Tap started (allow UAC the first time to capture Back/Volume keys)",
  "HID Tap 等待 RC003 WUDFHost（请确认遥控器已配对且开机）":
    "HID Tap waiting for RC003 WUDFHost (make sure the remote is paired and on)",
  "HID Tap Gadget 已就绪（等待按键 IO）": "HID Tap Gadget ready (waiting for key IO)",
  "HID Tap 就绪：返回/音量信号可捕获": "HID Tap ready: Back/Volume signals can be captured",
  "HID Tap 心跳超时，重新注入": "HID Tap heartbeat timed out, re-injecting",
  "Raw Input 旁路已启动（HID Tap 未附着时的按键兜底）":
    "Raw Input fallback started (handles keys while HID Tap is not attached)",
  "跳过 GATT HID（避免抢占 Windows 音量；返回/音量走 HID Tap）":
    "Skipping GATT HID (avoids taking over Windows volume; Back/Volume go through HID Tap)",
  "GATT HID 无法 SharedReadOnly（Windows HID 可能独占）":
    "GATT HID cannot open SharedReadOnly (Windows HID may hold it exclusively)",
  "注入成功": "Injected",
  "UAC被拒": "UAC denied",
  "注入失败": "Injection failed",

  // ── 语音识别（linux/asr.rs、voice_sink.rs、linux-asr-result 的 error）──
  "Qwen3-ASR 0.6B（推荐：中英混说准）": "Qwen3-ASR 0.6B (recommended: accurate for mixed Chinese/English)",
  "中英混说、技术词准确率最高；松手后约 1 秒出字，占内存约 1.9GB":
    "Best accuracy for mixed Chinese/English and technical terms; text appears about 1 s after release; uses about 1.9 GB RAM",
  "SenseVoice Small（极速）": "SenseVoice Small (fastest)",
  "几乎瞬间出字、内存小；英文技术词容易错（如 cargo build 识别成 cargobi）":
    "Near-instant text, low memory; often gets English technical terms wrong (e.g. 'cargo build' → 'cargobi')",
  "无法确定模型目录": "Cannot determine the model folder",
  "sherpa-onnx 创建识别器失败（模型文件损坏？）": "sherpa-onnx failed to create the recognizer (model files damaged?)",
  "未知的语音识别模型": "Unknown speech model",
  "语音识别模型加载失败": "Failed to load the speech model",
  "语音识别未就绪": "Speech recognition not ready",
  "识别无结果": "No recognition result",
  "没有听到声音": "No sound heard",
  "没有识别出文字": "No text recognized",
  "文字上屏失败": "Failed to type the text",
  "语音识别不可用": "Speech recognition unavailable",
  "已取消": "Cancelled",
  "已取消下载": "Download cancelled",
  "没有可用的下载源": "No download source available",
  "模型目录未初始化": "Model folder not initialized",
  "虚拟键盘不可用，无法粘贴": "Virtual keyboard unavailable, cannot paste",
  "uinput 虚拟键盘不可用": "uinput virtual keyboard unavailable",

  // ── 蓝牙 / 连接（linux/bluez.rs、connect.rs）──
  "蓝牙已关闭：请在系统「设置 → 蓝牙」里打开": "Bluetooth is off: turn it on in system Settings → Bluetooth",
  "遥控器未连接：按一下遥控器任意键唤醒它": "Remote not connected: press any key on the remote to wake it",
  "ATVV 服务缺少 TX/CONTROL 特征": "ATVV service is missing the TX/CONTROL characteristics",
  "遥控器上没有找到 ATVV 语音服务": "No ATVV voice service found on the remote",
  "订阅 ATVV 音频通道失败": "Failed to subscribe to the ATVV audio channel",
  "ATVV CONTROL 通知结束（遥控器断开）": "ATVV CONTROL notifications ended (remote disconnected)",
  "ATVV 音频通知结束（遥控器断开）": "ATVV audio notifications ended (remote disconnected)",
  "遥控器已断开（休眠或超出范围）": "Remote disconnected (asleep or out of range)",
  "遥控器已断开": "Remote disconnected",
  "遥控器已断开连接": "Remote disconnected",
  "BlueZ 设备事件流结束": "BlueZ device event stream ended",
  "未找到已配对的小米遥控器 2 Pro：请先在系统「设置 → 蓝牙」里配对「MI RC」（同时按住遥控器「主页」+「菜单」键约 3 秒进入配对模式）":
    "No paired Xiaomi Remote 2 Pro found: pair “MI RC” in system Settings → Bluetooth first (hold Home + Menu on the remote for about 3 seconds to enter pairing mode)",
  "未找到已配对的小米遥控器 2 Pro。请先在 Windows 蓝牙设置中配对「MI RC」":
    "No paired Xiaomi Remote 2 Pro found. Pair “MI RC” in Windows Bluetooth settings first",
  "已打开蓝牙设备，但未找到 ATVV 语音服务。请确认是小米遥控器 2 Pro (MI RC)":
    "Bluetooth device opened, but no ATVV voice service found. Make sure it is a Xiaomi Remote 2 Pro (MI RC)",
  "小米遥控器连接仅支持 Windows / Linux": "Xiaomi remote connection only supports Windows / Linux",
  "遥控器语音通道（ATVV）未连接": "Remote voice channel (ATVV) not connected",
  "语音键暂时只能触发快捷键，收不到声音。请打开 Voice VibeCoding，点「修复 ATVV 连接」。":
    "The voice key can only trigger the shortcut for now; no audio is received. Open Voice VibeCoding and click “Repair ATVV connection”.",
  "没有找到系统蓝牙设置程序": "System Bluetooth settings app not found",
  "BLE 桥接已在运行": "BLE bridge is already running",
  "注册BLE事件处理器失败": "Failed to register the BLE event handler",

  // ── 命令错误（ipc/*.rs、startup_env.rs、app_update.rs、file_download.rs）──
  "小米桥接已在运行": "Xiaomi bridge is already running",
  "旧桥接尚未退出，请稍后再试": "The old bridge has not exited yet. Try again shortly",
  "保存路径为空": "Save path is empty",
  "下载地址为空": "Download URL is empty",
  "仅支持 Windows / Linux": "Only Windows / Linux are supported",
  "仅 Linux 版可用": "Only available in the Linux version",
  "Linux 版请在源码目录执行 git pull 后重新运行 linux/install.sh 升级":
    "On Linux, run git pull in the source folder and then run linux/install.sh again to upgrade",
  "获取键盘独占超时": "Timed out getting exclusive keyboard access",
  "不是 WAV 文件": "Not a WAV file",
  "WAV 缺少 fmt 块": "WAV is missing the fmt chunk",
  "WAV 缺少 data 块": "WAV is missing the data chunk",
  "无法确定 ~/.config/autostart 目录": "Cannot determine the ~/.config/autostart folder",
  "无法确定程序路径": "Cannot determine the program path",
  "VB-CABLE 未就绪且内嵌驱动包不可用": "VB-CABLE is not ready and the bundled driver package is unavailable",
  "等待语音路由超时": "Timed out waiting for voice routing",
  "BridgeState 不可用": "BridgeState unavailable",
  "ConfigManager 不可用": "ConfigManager unavailable",
  "主动重置后桥接仍未启动（将依赖后续重连）": "Bridge still not started after reset (will rely on later reconnects)",
  "桥接未在时限内启动（将依赖后续重连）": "Bridge did not start in time (will rely on later reconnects)",
  "仅支持 Windows 安装包": "Only the Windows installer is supported",
  "安装包地址为空": "Installer URL is empty",
  "已有下载任务进行中": "A download is already in progress",
  "已有 WinUHid 下载任务进行中": "A WinUHid download is already in progress",
  "已有 VB-CABLE 下载任务进行中": "A VB-CABLE download is already in progress",
  "下载失败": "Download failed",
  "语音路由已重新启动": "Voice routing restarted",
  "已尝试重启语音路由（仍在启动中）": "Tried to restart voice routing (still starting)",
  "重启语音路由失败：端口可能仍被占用": "Failed to restart voice routing: the port may still be in use",
  "仅 Windows 支持结束进程": "Ending processes is only supported on Windows",
  "monitor_connection: 缺少 AppHandle": "monitor_connection: missing AppHandle",

  // ── 系统通知（Windows key_mapping.rs）──
  "遥控器 ATVV 未连接": "Remote ATVV not connected",
  "语音键可能触发系统 F5（如记事本插入日期）。请打开本软件，在小米设置中点击「修复 ATVV 连接」。":
    "The voice key may trigger F5 (e.g. Notepad inserts the date). Open this app and click “Repair ATVV connection” in the Xiaomi settings.",
  "虚拟键盘不可用，已降级": "Virtual keyboard unavailable, fell back",
  "语音键暂用 SendInput（类似旧版）。微信或可用；豆包/千问常无效。请点「修复虚拟键盘」。":
    "The voice key uses SendInput for now (like older versions). WeChat may work; Doubao/Qwen often do not. Click “Repair virtual keyboard”.",
};

type Rep = string | ((...m: string[]) => string);

/** 对捕获的子句再翻译（子句本身也可能是后端中文错误） */
const tb = (s: string | undefined): string => (s === undefined ? "" : translateBackend(s));

function esc(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** 「<前缀>失败: <错误>」→「<英文>: <错误>」 */
function fail(zh: string, en: string): [RegExp, Rep] {
  return [new RegExp(`^${esc(zh)}失败: ([\\s\\S]*)$`), (_m, e) => `${en}: ${tb(e)}`];
}

/** 「<前缀>: <错误>」→「<英文>: <错误>」 */
function colon(zh: string, en: string): [RegExp, Rep] {
  return [new RegExp(`^${esc(zh)}: ([\\s\\S]*)$`), (_m, e) => `${en}: ${tb(e)}`];
}

export const RULES: Array<[RegExp, Rep]> = [
  // ── 电量 ──
  [/^电量 (\d+)%$/, "Battery $1%"],
  [/^电量读取失败: ([\s\S]*)$/, (_m, e) => `Battery read failed: ${tb(e)}`],

  // ── 包装别的句子的外层 ──
  [/^Error\|([\s\S]*)$/, (_m, e) => `Error: ${tb(e)}`],
  [/^错误: ([\s\S]*)$/, (_m, e) => `Error: ${tb(e)}`],
  [/^([\s\S]+)（将自动重试）$/, (_m, e) => `${tb(e)} (retrying automatically)`],
  [/^ATVV 语音通道不可用: ([\s\S]*)$/, (_m, e) => `ATVV voice channel unavailable: ${tb(e)}`],
  [
    /^([\s\S]+)。请在终端运行 linux\/setup-system\.sh（安装 udev 规则后需重新登录或重新插拔）。$/,
    (_m, e) =>
      `${tb(e)}. Run linux/setup-system.sh in a terminal (log out and back in, or replug, after installing the udev rules).`,
  ],
  [
    /^([\s\S]+)。请打开 Voice VibeCoding，在小米页面下载语音识别模型。$/,
    (_m, e) => `${tb(e)}. Open Voice VibeCoding and download a speech model on the Xiaomi page.`,
  ],
  [/^(.+) 连接逻辑尚未接入$/, (_m, d) => `${tb(d)} connection is not implemented yet`],

  // ── Linux 权限 / 设备 ──
  [
    /^打开 \/dev\/uinput 失败: ([\s\S]*)（请先运行 linux\/setup-system\.sh 安装 udev 规则）$/,
    "Failed to open /dev/uinput: $1 (run linux/setup-system.sh first to install the udev rules)",
  ],
  [
    /^无权读取遥控器按键 (.+)：请先运行 linux\/setup-system\.sh 安装 udev 规则$/,
    "No permission to read remote keys $1: run linux/setup-system.sh first to install the udev rules",
  ],
  [
    /^遥控器按键被其它程序占用（([\s\S]*)），请关闭重复运行的实例$/,
    "Remote keys are held by another program ($1). Close any duplicate running instance",
  ],
  [
    /^无法运行 pactl: ([\s\S]*)（需要 pipewire-pulse 或 pulseaudio-utils）$/,
    "Cannot run pactl: $1 (needs pipewire-pulse or pulseaudio-utils)",
  ],
  [/^pactl load-module 返回异常: ([\s\S]*)$/, "pactl load-module returned unexpected output: $1"],
  [/^pactl (\S*) 失败: ([\s\S]*)$/, "pactl $1 failed: $2"],
  [
    /^无法连接 BlueZ（bluetooth 服务是否在运行？）: ([\s\S]*)$/,
    "Cannot connect to BlueZ (is the bluetooth service running?): $1",
  ],
  [
    /^无法连接 X11 显示（录入快捷键目前只支持 X11 会话）: ([\s\S]*)$/,
    "Cannot connect to the X11 display (shortcut capture only supports X11 sessions for now): $1",
  ],
  [
    /^无法独占键盘（([\s\S]*)），请关闭弹出菜单后重试$/,
    "Cannot get exclusive keyboard access ($1). Close any open menu and try again",
  ],
  [
    /^只支持 16kHz\/单声道\/16bit PCM（当前 ([\s\S]*)）$/,
    "Only 16 kHz / mono / 16-bit PCM is supported (current $1)",
  ],

  // ── 语音识别模型 ──
  [/^语音识别模型未安装：([\s\S]*)$/, (_m, t) => `Speech model not installed: ${tb(t)}`],
  [/^(.+) 校验失败（sha256 不匹配）$/, "$1 failed verification (sha256 mismatch)"],
  [/^正在下载 (.+)，请等它完成$/, "Already downloading $1. Wait for it to finish"],
  [/^分段长度不符 ([\s\S]*)$/, "Chunk length mismatch $1"],
  [/^(hf-mirror\.com|huggingface\.co): ([\s\S]*)$/, (_m, s, e) => `${s}: ${tb(e)}`],

  // ── 冲突 / 修复 ──
  [
    /^发现占用进程：([\s\S]*)。请在弹窗中结束后，将自动继续修复。$/,
    (_m, p) => `Conflicting processes found: ${p.replace(/、/g, ", ")}. End them in the dialog and the repair will continue automatically.`,
  ],
  [/^拒绝结束非白名单进程 (\S+) \(pid=(\d+)\)$/, "Refusing to end non-whitelisted process $1 (pid=$2)"],
  [
    /^语音路由端口 (\d+) 可能被占用（WinError 10048）或路由未就绪$/,
    "Voice routing port $1 may be in use (WinError 10048) or routing is not ready",
  ],
  [
    /^HID Tap 端口 (\d+) 被其它进程占用（请关闭其它 RemoteBridge 实例）: ([\s\S]*)$/,
    "HID Tap port $1 is used by another process (close other RemoteBridge instances): $2",
  ],
  [/^HID Tap 端口 (\d+) 绑定失败: ([\s\S]*)$/, "HID Tap port $1 bind failed: $2"],

  // ── Windows 状态日志（带变量）──
  [/^已找到 RC003 WUDFHost pid=(\d+)，准备注入…$/, "Found RC003 WUDFHost pid=$1, preparing to inject…"],
  [/^已请求注入 WUDFHost pid=(\d+)（若弹出 UAC 请允许）$/, "Requested injection into WUDFHost pid=$1 (allow UAC if prompted)"],
  [/^HID Tap 已附着 pid=(\d+)，请按返回\/音量键验证$/, "HID Tap attached pid=$1. Press Back/Volume to verify"],
  [
    /^UAC 注入被拒绝；(\d+) 秒后重试（源头清除暂停，仅钩子兜底）$/,
    "UAC injection denied; retrying in $1 s (source clearing paused, hook fallback only)",
  ],
  [/^HID Tap 钩子错误: ([\s\S]*)$/, "HID Tap hook error: $1"],
  [/^输入会话已启动 \(([\s\S]*)\)$/, "Input session started ($1)"],
  [/^ATVV FromId 失败，回退地址打开: ([\s\S]*)$/, "ATVV FromId failed, falling back to opening by address: $1"],
  [
    /^语音音频：VB-CABLE 未就绪（([\s\S]*)）；快捷键仍可用$/,
    (_m, e) => `Voice audio: VB-CABLE not ready (${tb(e)}); shortcuts still work`,
  ],
  [
    /^发现 (\d+) 个小米候选设备，请仅保留当前 2 Pro 配对后重试，或在设置中填写蓝牙地址$/,
    "Found $1 Xiaomi candidate devices. Keep only the current 2 Pro paired and retry, or enter the Bluetooth address in settings",
  ],
  [
    /^未找到已配对 BLE 设备: (.*)（请确认 Windows 已配对且设备开机）$/,
    "No paired BLE device found: $1 (make sure it is paired in Windows and turned on)",
  ],

  // ── 前端拼的下载失败日志 ──
  [/^WinUHid 驱动包下载失败: ([\s\S]*)$/, (_m, e) => `WinUHid driver package download failed: ${tb(e)}`],
  [/^VB-CABLE 驱动包下载失败: ([\s\S]*)$/, (_m, e) => `VB-CABLE driver package download failed: ${tb(e)}`],

  // ── 带路径 / 名字的失败 ──
  [/^保存 (\S+) 失败: ([\s\S]*)$/, (_m, f, e) => `Failed to save ${f}: ${tb(e)}`],
  [/^读取 (\S+) 失败: ([\s\S]*)$/, (_m, f, e) => `Failed to read ${f}: ${tb(e)}`],
  [/^打开 (\S+) 失败: ([\s\S]*)$/, (_m, f, e) => `Failed to open ${f}: ${tb(e)}`],
  [/^无法写入 (.+?): ([\s\S]*)$/, "Cannot write $1: $2"],
  [/^无法创建目录 (.+?): ([\s\S]*)$/, "Cannot create folder $1: $2"],
  [/^无效蓝牙地址 (\S+): ([\s\S]*)$/, "Invalid Bluetooth address $1: $2"],
  [/^蓝牙地址格式无效：([\s\S]*)$/, "Invalid Bluetooth address format: $1"],
  [/^未知设备类型: ([\s\S]*)$/, "Unknown device type: $1"],
  [/^未知来源: (\S+)，可选 ([\s\S]*)$/, "Unknown source: $1. Options: $2"],
  [/^打开 ATVV 服务状态异常: ([\s\S]*)$/, "Unexpected status opening the ATVV service: $1"],
  [/^GATT 服务发现状态异常: ([\s\S]*)$/, "Unexpected GATT service discovery status: $1"],

  // ── 「X失败: 错误」──
  fail("创建临时文件", "Failed to create temp file"),
  fail("写文件", "Failed to write file"),
  fail("创建模型目录", "Failed to create model folder"),
  fail("启动下载线程", "Failed to start download thread"),
  fail("创建 autostart 目录", "Failed to create autostart folder"),
  fail("写入自启项", "Failed to write autostart entry"),
  fail("删除自启项", "Failed to remove autostart entry"),
  fail("枚举蓝牙设备", "Failed to list Bluetooth devices"),
  fail("打开蓝牙设备", "Failed to open Bluetooth device"),
  fail("读取 GATT 服务", "Failed to read GATT services"),
  fail("读取 ATVV 特征", "Failed to read ATVV characteristics"),
  fail("订阅 ATVV CONTROL ", "Failed to subscribe to ATVV CONTROL"),
  fail("监听设备状态", "Failed to watch device state"),
  fail("启动 ATVV 线程", "Failed to start ATVV thread"),
  fail("创建配置目录", "Failed to create config folder"),
  fail("写入 linux.json ", "Failed to write linux.json"),
  fail("启动录入线程", "Failed to start capture thread"),
  fail("写剪贴板", "Failed to write clipboard"),
  fail("写 PRIMARY 选区", "Failed to write PRIMARY selection"),
  fail("创建 uinput 虚拟键盘", "Failed to create uinput virtual keyboard"),
  fail("uinput 写入", "uinput write failed"),
  fail("打开虚拟麦克风管道", "Failed to open virtual mic pipe"),
  fail("启动小米 worker ", "Failed to start Xiaomi worker"),
  fail("重启 worker ", "Failed to restart worker"),
  fail("打开日志目录", "Failed to open log folder"),
  fail("打开模型目录", "Failed to open models folder"),
  fail("请求", "Request failed"),
  fail("解析 latest.json ", "Failed to parse latest.json"),
  fail("下载请求", "Download request failed"),
  fail("下载", "Download failed"),
  fail("读取下载数据", "Failed to read download data"),
  fail("写入安装包", "Failed to write installer"),
  fail("写入升级脚本", "Failed to write update script"),
  fail("启动静默升级", "Failed to start silent update"),
  fail("创建下载目录", "Failed to create download folder"),
  fail("写入文件", "Failed to write file"),
  fail("桥接主动重置", "Bridge reset failed"),
  fail("序列化配置", "Failed to serialize config"),
  fail("写入临时文件", "Failed to write temp file"),
  fail("同步临时文件", "Failed to sync temp file"),
  fail("替换配置文件", "Failed to replace config file"),
  fail("读取设置", "Failed to read settings"),
  fail("解析设置", "Failed to parse settings"),
  fail("序列化设置", "Failed to serialize settings"),
  fail("替换设置文件", "Failed to replace settings file"),
  fail("HID Tap 注入", "HID Tap injection failed"),
  fail("Raw Input 启动", "Raw Input failed to start"),
  fail("语音路由启动", "Voice routing failed to start"),
  fail("创建BLE扫描器", "Failed to create BLE scanner"),
  fail("启动BLE扫描", "Failed to start BLE scan"),
  fail("枚举 GATT 接口", "Failed to list GATT interfaces"),
  fail("GATT 服务发现", "GATT service discovery failed"),
  colon("无法访问剪贴板", "Cannot access clipboard"),
  colon("无法获取应用数据目录", "Cannot get the app data folder"),
  colon("没有找到蓝牙适配器", "No Bluetooth adapter found"),
  colon("没有蓝牙适配器", "No Bluetooth adapter"),
  colon("地址无效", "Invalid address"),
];

export function translateBackend(text: string): string {
  if (!text) return text;
  const exact = EXACT[text];
  if (exact !== undefined) return exact;
  for (const [re, rep] of RULES) {
    if (re.test(text)) {
      return typeof rep === "string"
        ? text.replace(re, rep)
        : text.replace(re, (...m) => rep(...(m as string[])));
    }
  }
  return text;
}
