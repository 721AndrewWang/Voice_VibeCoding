//! 小米遥控器按键采集（evdev）
//!
//! BlueZ 的 HOG 插件把遥控器的 HID 报告交给内核 uhid，内核生成普通的
//! `/dev/input/eventN`。本模块独占（EVIOCGRAB）这些节点：
//! - 遥控器原始按键不会再到达桌面（没有 Windows 版那一整套 F5/Home 泄漏问题）
//! - 读到的键翻译成 button_id，交给与 Windows 共用的映射逻辑（key_mapping）
//!
//! 只 grab 遥控器自己的节点，真实键盘完全不受影响。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use evdev::{BusType, Device, EventSummary, KeyCode, MiscCode};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::bridges::xiaomi::connect::XiaomiRuntime;
use crate::bridges::xiaomi::key_log::{button_label, emit_key_and_map, emit_key_phase};

pub const XIAOMI_VENDOR: u16 = 0x2717;
pub const XIAOMI_2_PRO_PRODUCT: u16 = 0x32B8;

/// 正在读取的节点 → 设备名
static ACTIVE: Mutex<Option<HashMap<PathBuf, String>>> = Mutex::new(None);
/// 权限不足等问题（给主机状态栏展示）
static LAST_PROBLEM: Mutex<Option<String>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);
static LOGGED_UNKNOWN: Mutex<Option<HashSet<u32>>> = Mutex::new(None);
/// 遥控器语音键（固件 F5）当前是否按下 —— 仅用于 ATVV 不可用时的兜底
static FIRMWARE_VOICE_DOWN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInputStatus {
    pub devices: Vec<String>,
    pub problem: Option<String>,
}

pub fn status() -> RemoteInputStatus {
    let devices = ACTIVE
        .lock()
        .as_ref()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();
    RemoteInputStatus {
        devices,
        problem: LAST_PROBLEM.lock().clone(),
    }
}

pub fn has_devices() -> bool {
    ACTIVE.lock().as_ref().map(|m| !m.is_empty()).unwrap_or(false)
}

/// 是否为小米遥控器（按 VID/PID；名字兜底）
pub fn is_remote_identity(bus: BusType, vendor: u16, product: u16, name: &str) -> bool {
    if bus == BusType::BUS_VIRTUAL {
        return false;
    }
    if vendor == XIAOMI_VENDOR && product == XIAOMI_2_PRO_PRODUCT {
        return true;
    }
    let n = name.trim().to_ascii_lowercase();
    bus == BusType::BUS_BLUETOOTH
        && (n.starts_with("mi rc") || n.starts_with("xiaomi bluetooth remote"))
}

/// HID usage（MSC_SCAN，高 16 位 usage page）→ button_id
pub fn button_for_usage(scan: u32) -> Option<&'static str> {
    let page = scan >> 16;
    let usage = scan & 0xFFFF;
    match (page, usage) {
        (0x07, 0x3E) => Some("mic"),
        (0x07, 0x66) => Some("power"),
        (0x07, 0x52) => Some("up"),
        (0x07, 0x51) => Some("down"),
        (0x07, 0x50) => Some("left"),
        (0x07, 0x4F) => Some("right"),
        (0x07, 0x28) => Some("ok"),
        (0x07, 0xF1) => Some("back"),
        (0x07, 0x4A) => Some("home"),
        (0x07, 0x65) => Some("menu"),
        (0x07, 0x35) => Some("tv"),
        (0x07, 0x80) | (0x0C, 0xE9) => Some("volume_up"),
        (0x07, 0x81) | (0x0C, 0xEA) => Some("volume_down"),
        (0x07, 0x7F) | (0x0C, 0xE2) => Some("volume_mute"),
        (0x0C, 0x223) => Some("home"),
        (0x0C, 0x224) => Some("back"),
        (0x0C, 0x40) => Some("menu"),
        (0x0C, 0x41) => Some("ok"),
        (0x0C, 0x30) => Some("power"),
        (0x0C, 0x221) | (0x0C, 0xCF) => Some("mic"),
        _ => None,
    }
}

/// evdev 键码 → button_id（没有 MSC_SCAN 时兜底）
pub fn button_for_key(code: KeyCode) -> Option<&'static str> {
    use KeyCode as K;
    let id = match code {
        K::KEY_F5 | K::KEY_VOICECOMMAND | K::KEY_SEARCH | K::KEY_ASSISTANT => "mic",
        K::KEY_POWER => "power",
        K::KEY_UP => "up",
        K::KEY_DOWN => "down",
        K::KEY_LEFT => "left",
        K::KEY_RIGHT => "right",
        K::KEY_ENTER | K::KEY_KPENTER | K::KEY_SELECT | K::KEY_OK => "ok",
        K::KEY_BACK | K::KEY_ESC => "back",
        K::KEY_HOME | K::KEY_HOMEPAGE => "home",
        K::KEY_COMPOSE | K::KEY_MENU | K::KEY_CONTEXT_MENU => "menu",
        K::KEY_GRAVE | K::KEY_TV => "tv",
        K::KEY_VOLUMEUP => "volume_up",
        K::KEY_VOLUMEDOWN => "volume_down",
        K::KEY_MUTE => "volume_mute",
        _ => return None,
    };
    Some(id)
}

fn remote_node_paths() -> Vec<(PathBuf, Result<Device, std::io::Error>)> {
    let Ok(dir) = std::fs::read_dir("/dev/input") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        let is_event = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("event"))
            .unwrap_or(false);
        if !is_event {
            continue;
        }
        // 先用 sysfs 判断身份，避免去 open 没权限的真实键盘
        if !sysfs_looks_like_remote(&path) {
            continue;
        }
        out.push((path.clone(), Device::open(&path)));
    }
    out
}

fn read_sys(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn sysfs_looks_like_remote(dev_path: &Path) -> bool {
    let Some(name) = dev_path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let base = PathBuf::from("/sys/class/input").join(name).join("device");
    let hex = |f: &str| {
        read_sys(&base.join("id").join(f)).and_then(|s| u16::from_str_radix(&s, 16).ok())
    };
    let (Some(bus), Some(vendor), Some(product)) = (hex("bustype"), hex("vendor"), hex("product"))
    else {
        return false;
    };
    let dev_name = read_sys(&base.join("name")).unwrap_or_default();
    is_remote_identity(BusType(bus), vendor, product, &dev_name)
}

fn bridge_enabled(app: &AppHandle) -> bool {
    app.try_state::<Arc<XiaomiRuntime>>()
        .map(|rt| !rt.should_stop())
        .unwrap_or(true)
}

/// 启动常驻扫描线程（整个应用生命周期一次）
pub fn start(app: AppHandle) {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    *ACTIVE.lock() = Some(HashMap::new());
    std::thread::Builder::new()
        .name("linux-remote-scan".into())
        .spawn(move || scan_loop(app))
        .ok();
}

/// /dev/input 出现新节点、或 udev 刚给节点加上 ACL（IN_ATTRIB）时唤醒扫描线程。
///
/// 遥控器每次断线重连，内核都会重建它的输入节点；唤醒遥控器的那一下按键紧随其后就到。
/// 只靠定时扫描会来不及独占，按键就漏给了桌面（语音键在 HID 层是 F5，会刷新网页）。
fn spawn_dev_input_watcher(tx: std::sync::mpsc::Sender<()>) {
    std::thread::Builder::new()
        .name("linux-remote-inotify".into())
        .spawn(move || {
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd < 0 {
                log::warn!("LINUX remote input: inotify_init1 failed; falling back to polling");
                return;
            }
            let dir = std::ffi::CString::new("/dev/input").expect("static path");
            let wd = unsafe {
                libc::inotify_add_watch(fd, dir.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB)
            };
            if wd < 0 {
                log::warn!("LINUX remote input: inotify watch /dev/input failed; polling only");
                unsafe { libc::close(fd) };
                return;
            }
            let mut buf = [0u8; 4096];
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
                if tx.send(()).is_err() {
                    break;
                }
            }
            unsafe { libc::close(fd) };
        })
        .ok();
}

/// 新节点出现后的快速接管窗口：期间每 10ms 扫一次（udev 加上 ACL 之前 open 会被拒）
const FAST_GRAB_WINDOW: Duration = Duration::from_millis(1500);
const FAST_SCAN_INTERVAL: Duration = Duration::from_millis(10);
const IDLE_SCAN_INTERVAL: Duration = Duration::from_millis(700);

fn scan_loop(app: AppHandle) {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    spawn_dev_input_watcher(tx);
    let mut permission_warned: HashSet<PathBuf> = HashSet::new();
    let mut fast_until: Option<Instant> = None;
    loop {
        let fast = fast_until.is_some_and(|t| Instant::now() < t);
        if bridge_enabled(&app) {
            for (path, opened) in remote_node_paths() {
                let already = ACTIVE
                    .lock()
                    .as_ref()
                    .map(|m| m.contains_key(&path))
                    .unwrap_or(false);
                if already {
                    continue;
                }
                match opened {
                    Ok(dev) => {
                        permission_warned.remove(&path);
                        spawn_reader(app.clone(), path, dev);
                    }
                    // 快速窗口里的 EACCES 多半是 udev 还没来得及加 ACL：不报警，继续重试
                    Err(e) if fast && e.kind() == std::io::ErrorKind::PermissionDenied => {}
                    Err(e) => {
                        if permission_warned.insert(path.clone()) {
                            let msg = if e.kind() == std::io::ErrorKind::PermissionDenied {
                                format!(
                                    "无权读取遥控器按键 {}：请先运行 linux/setup-system.sh 安装 udev 规则",
                                    path.display()
                                )
                            } else {
                                format!("打开 {} 失败: {e}", path.display())
                            };
                            log::warn!("LINUX remote input: {msg}");
                            *LAST_PROBLEM.lock() = Some(msg);
                        }
                    }
                }
            }
        }
        let wait = if fast { FAST_SCAN_INTERVAL } else { IDLE_SCAN_INTERVAL };
        match rx.recv_timeout(wait) {
            Ok(()) => {
                while rx.try_recv().is_ok() {}
                fast_until = Some(Instant::now() + FAST_GRAB_WINDOW);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(wait),
        }
    }
}

fn spawn_reader(app: AppHandle, path: PathBuf, mut dev: Device) {
    let name = dev.name().unwrap_or("MI RC").to_string();
    if let Err(e) = dev.grab() {
        // 别的程序（如旧实例）已经 grab：不抢，稍后重试
        log::warn!("LINUX remote input: grab {} failed: {e}", path.display());
        *LAST_PROBLEM.lock() = Some(format!("遥控器按键被其它程序占用（{e}），请关闭重复运行的实例"));
        return;
    }
    *LAST_PROBLEM.lock() = None;
    if let Some(m) = ACTIVE.lock().as_mut() {
        m.insert(path.clone(), name.clone());
    }
    log::info!("LINUX remote input: grabbed {} ({name})", path.display());
    on_remote_nodes_changed(&app);

    std::thread::Builder::new()
        .name(format!(
            "linux-remote-{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("ev")
        ))
        .spawn(move || {
            let reason = read_loop(&app, &mut dev);
            // 抬起所有仍按着的键，防止映射卡键
            release_all_pressed(&app);
            let _ = dev.ungrab();
            if let Some(m) = ACTIVE.lock().as_mut() {
                m.remove(&path);
            }
            log::info!("LINUX remote input: released {} ({reason})", path.display());
            on_remote_nodes_changed(&app);
        })
        .ok();
}

static PRESSED: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

fn mark_pressed(id: &'static str, down: bool) -> bool {
    let mut g = PRESSED.lock();
    let set = g.get_or_insert_with(HashSet::new);
    if down {
        set.insert(id)
    } else {
        set.remove(id)
    }
}

fn release_all_pressed(app: &AppHandle) {
    let ids: Vec<&'static str> = PRESSED
        .lock()
        .as_mut()
        .map(|s| s.drain().collect())
        .unwrap_or_default();
    for id in ids {
        dispatch(app, id, false);
    }
}

fn read_loop(app: &AppHandle, dev: &mut Device) -> String {
    use std::os::fd::AsRawFd;
    let mut pending_scan: Option<u32> = None;
    if let Err(e) = dev.set_nonblocking(true) {
        return format!("set_nonblocking: {e}");
    }
    loop {
        if !bridge_enabled(app) {
            return "bridge stopped".into();
        }
        // poll 带超时：用户点「断开」后 300ms 内释放 grab，遥控器恢复成普通键盘
        let mut pfd = libc::pollfd {
            fd: dev.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let r = unsafe { libc::poll(&mut pfd, 1, 300) };
        if r < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return format!("poll error: {err}");
        }
        if r == 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return "device removed".into();
        }
        let events = match dev.fetch_events() {
            Ok(ev) => ev.collect::<Vec<_>>(),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return format!("read error: {e}"),
        };
        for ev in events {
            match ev.destructure() {
                EventSummary::Misc(_, MiscCode::MSC_SCAN, value) => {
                    pending_scan = Some(value as u32);
                }
                EventSummary::Key(_, code, value) => {
                    let scan = pending_scan.take();
                    if value == 2 {
                        continue; // 内核自动重复：映射层有自己的连发节奏
                    }
                    let id = scan
                        .and_then(button_for_usage)
                        .or_else(|| button_for_key(code));
                    match id {
                        Some(id) => {
                            if mark_pressed(id, value == 1) {
                                dispatch(app, id, value == 1);
                            }
                        }
                        None => log_unknown(code, scan),
                    }
                }
                EventSummary::Synchronization(..) => pending_scan = None,
                _ => {}
            }
        }
    }
}

fn log_unknown(code: KeyCode, scan: Option<u32>) {
    let key = ((code.code() as u32) << 16) ^ scan.unwrap_or(0);
    let mut g = LOGGED_UNKNOWN.lock();
    if g.get_or_insert_with(HashSet::new).insert(key) {
        log::info!(
            "LINUX remote input: unmapped key code={:?}({}) scan={:?}",
            code,
            code.code(),
            scan.map(|s| format!("0x{s:X}"))
        );
    }
}

/// 一次按键（down/up）交给共用的映射逻辑
fn dispatch(app: &AppHandle, id: &'static str, down: bool) {
    if crate::linux::shortcut_x11::is_capture_active() {
        // 录快捷键期间只高亮，不注入
        emit_key_phase(app, id, button_label(id), down);
        return;
    }
    if id == "mic" {
        on_firmware_voice_key(app, down);
        return;
    }
    emit_key_and_map(app, id, button_label(id), down);
}

/// 语音键的 HID 部分（固件 F5）。正常情况下按住/松开由 ATVV 的
/// AUDIO_START/AUDIO_STOP 驱动（见 linux::bluez），这里只在 ATVV 没连上时兜底：
/// 仍然按住/松开映射的快捷键（虚拟麦克风模式），并提示用户修复 ATVV。
fn on_firmware_voice_key(app: &AppHandle, down: bool) {
    if crate::bridges::xiaomi::connect::atvv_subscribed() {
        FIRMWARE_VOICE_DOWN.store(down, Ordering::Release);
        return;
    }
    let was = FIRMWARE_VOICE_DOWN.swap(down, Ordering::AcqRel);
    if was == down {
        return;
    }
    emit_key_and_map(app, "mic", button_label("mic"), down);
    if down {
        crate::linux::bluez::notify_atvv_unavailable(app);
    }
}

fn on_remote_nodes_changed(app: &AppHandle) {
    if has_devices() {
        crate::bridges::xiaomi::tv_gate::mark_ready(tv_ready_delay(app));
    }
}

fn tv_ready_delay(app: &AppHandle) -> Duration {
    app.try_state::<crate::config::manager::ConfigManager>()
        .and_then(|m| m.get_device_config("xiaomi").ok())
        .map(|c| Duration::from_secs_f32(c.tv_action_ready_delay.clamp(0.0, 10.0)))
        .unwrap_or(Duration::from_secs(2))
}

/// 等待遥控器的输入节点出现（配对后刚连上时用）
pub fn wait_for_devices(timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if has_devices() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    has_devices()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_by_vid_pid_or_bt_name() {
        assert!(is_remote_identity(BusType::BUS_BLUETOOTH, 0x2717, 0x32B8, "whatever"));
        assert!(is_remote_identity(BusType::BUS_BLUETOOTH, 0x0001, 0x0002, "MI RC"));
        assert!(is_remote_identity(BusType::BUS_BLUETOOTH, 0, 0, "Xiaomi Bluetooth Remote 2 Pro Keyboard"));
        assert!(!is_remote_identity(BusType::BUS_USB, 0x046d, 0xc52b, "MI RC"));
        assert!(!is_remote_identity(BusType::BUS_VIRTUAL, 0x2717, 0x32B8, "MI RC"));
        assert!(!is_remote_identity(BusType::BUS_BLUETOOTH, 0x1234, 0x5678, "WBT22"));
    }

    #[test]
    fn rc003_keyboard_page_usages() {
        let k = |u: u32| button_for_usage(0x0007_0000 | u);
        assert_eq!(k(0x3E), Some("mic"));
        assert_eq!(k(0xF1), Some("back"));
        assert_eq!(k(0x35), Some("tv"));
        assert_eq!(k(0x65), Some("menu"));
        assert_eq!(k(0x4A), Some("home"));
        assert_eq!(k(0x80), Some("volume_up"));
        assert_eq!(k(0x04), None);
        assert_eq!(button_for_usage(0x000C_00E9), Some("volume_up"));
    }

    #[test]
    fn keycode_fallback_matches_linux_hid_keyboard_table() {
        // hid-input 把 RC003 的键盘页 usage 翻成这些键码
        assert_eq!(button_for_key(KeyCode::KEY_F5), Some("mic"));
        assert_eq!(button_for_key(KeyCode::KEY_BACK), Some("back"));
        assert_eq!(button_for_key(KeyCode::KEY_COMPOSE), Some("menu"));
        assert_eq!(button_for_key(KeyCode::KEY_GRAVE), Some("tv"));
        assert_eq!(button_for_key(KeyCode::KEY_HOME), Some("home"));
        assert_eq!(button_for_key(KeyCode::KEY_A), None);
    }
}
