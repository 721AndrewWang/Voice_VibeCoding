//! Linux 主机状态栏（与 Windows 共用 `XiaomiHostStatus` 结构，四列换成 Linux 组件）

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use super::settings::VoiceMode;
use crate::bridges::xiaomi::connect::XiaomiRuntime;
use crate::bridges::{BridgeState, BridgeStatus, BridgeType};
use crate::ipc::commands::{XiaomiHostStatus, XiaomiHostStatusItem};

fn item(id: &str, label: &str, state: &str, tone: &str) -> XiaomiHostStatusItem {
    XiaomiHostStatusItem {
        id: id.into(),
        label: label.into(),
        state_label: state.into(),
        tone: tone.into(),
    }
}

/// 虚拟麦克风是否还在（起 pactl，5 秒节流）
fn mic_verified() -> bool {
    static LAST: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    if let Some((t, v)) = *LAST.lock() {
        if t.elapsed() < Duration::from_secs(5) {
            return v;
        }
    }
    let v = super::virtual_mic::verify();
    *LAST.lock() = Some((Instant::now(), v));
    v
}

pub fn build(app: &AppHandle) -> XiaomiHostStatus {
    let settings = super::settings::get();
    let worker_alive = app
        .try_state::<Arc<XiaomiRuntime>>()
        .map(|r| r.running.load(Ordering::SeqCst))
        .unwrap_or(false);
    let connected = app
        .try_state::<BridgeState>()
        .map(|s| s.get_info(BridgeType::Xiaomi).status == BridgeStatus::Connected)
        .unwrap_or(false);
    let atvv_ok = crate::bridges::xiaomi::connect::atvv_subscribed();
    let uinput_ready = super::uinput_kbd::is_ready() || super::uinput_kbd::ensure_init();
    let input = super::remote_input::status();
    let keys_grabbed = !input.devices.is_empty();
    let asr = super::asr::status();

    let (voice_label, voice_ok, voice_state) = match settings.voice_mode {
        VoiceMode::Asr => {
            let state = if asr.loaded {
                "已加载"
            } else if asr.downloading {
                "下载中"
            } else if asr.loading {
                "加载中"
            } else if asr.installed {
                "待加载"
            } else {
                "未下载模型"
            };
            ("语音识别", asr.loaded || (asr.installed && !asr.error.is_some()), state)
        }
        VoiceMode::VirtualMic => {
            let ok = mic_verified();
            ("虚拟麦克风", ok, if ok { "已创建" } else { "未创建" })
        }
    };

    let items = vec![
        item(
            "cable",
            voice_label,
            voice_state,
            if voice_ok { "ok" } else { "error" },
        ),
        item(
            "winuhid",
            "虚拟键盘",
            if uinput_ready { "已就绪" } else { "无权限" },
            if uinput_ready { "ok" } else { "error" },
        ),
        item(
            "audio",
            "遥控按键",
            if keys_grabbed { "已接管" } else { "未连接" },
            if keys_grabbed { "ok" } else { "error" },
        ),
        item(
            "bridge",
            "蓝牙桥接",
            if connected {
                "已连接"
            } else if worker_alive {
                "等待遥控器"
            } else {
                "未启动"
            },
            if connected { "ok" } else { "error" },
        ),
    ];

    let paired = if connected {
        Some(true)
    } else {
        super::bluez::paired_remote_known()
    };
    let (status_text, detail, tone): (&str, String, &str) = if !worker_alive {
        ("桥接未运行", "可点「重启桥接」或打开日志检查。".into(), "error")
    } else if paired == Some(false) {
        (
            "未配对遥控器",
            "请在系统「设置 → 蓝牙」里配对「MI RC」：同时长按遥控器「主页」+「菜单」键约 3 秒进入配对模式。".into(),
            "warn",
        )
    } else if !uinput_ready {
        (
            "虚拟键盘不可用",
            format!(
                "{}。请在终端运行 linux/setup-system.sh（安装 udev 规则后需重新登录或重新插拔）。",
                super::uinput_kbd::last_error().unwrap_or_else(|| "无法打开 /dev/uinput".into())
            ),
            "warn",
        )
    } else if !connected {
        (
            "等待遥控器连接",
            "遥控器闲置会休眠断开，按一下任意键即可唤醒回连。".into(),
            "warn",
        )
    } else if let Some(problem) = input.problem.clone().filter(|_| !keys_grabbed) {
        ("按键未接管", problem, "warn")
    } else if !atvv_ok {
        (
            "ATVV 未连接",
            "语音专用通道未就绪，按住语音键收不到声音。可点「修复 ATVV 连接」。".into(),
            "warn",
        )
    } else if settings.voice_mode == VoiceMode::Asr && !asr.installed {
        (
            "语音识别模型未下载",
            "本地识别需要 SenseVoice 模型（约 240MB）。请在下方「语音输出」里点「下载模型」。".into(),
            "warn",
        )
    } else if !voice_ok {
        (
            "语音输出未就绪",
            asr.error.clone().unwrap_or_else(|| {
                super::virtual_mic::last_error().unwrap_or_else(|| "请稍候或点「重启桥接」。".into())
            }),
            "warn",
        )
    } else {
        ("运行正常", String::new(), "ok")
    };

    let voice_ready = connected && atvv_ok && uinput_ready && voice_ok;
    let tray_kind = if voice_ready {
        crate::ipc::tray::TrayIconKind::Ready
    } else if worker_alive && connected {
        crate::ipc::tray::TrayIconKind::Error
    } else {
        crate::ipc::tray::TrayIconKind::Init
    };
    crate::ipc::tray::sync_runtime_icons(app, tray_kind);

    XiaomiHostStatus {
        bridge_alive: worker_alive,
        audio_alive: keys_grabbed,
        cable_ready: settings.voice_mode == VoiceMode::VirtualMic && voice_ok,
        winuhid_ready: uinput_ready,
        atvv_ok,
        status_text: status_text.into(),
        detail,
        tone: tone.into(),
        items,
    }
}
