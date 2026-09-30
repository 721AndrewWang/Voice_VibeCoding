//! Linux 虚拟键盘（/dev/uinput）
//!
//! 配置里的映射一律是 Windows VK 码（与 Windows 版共用 xiaomi.json），
//! 这里在注入时翻译成 evdev `KEY_*`。组合键按下顺序与 Windows 版一致：
//! Ctrl → Shift → Super → Alt → 主键；抬起逆序。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, InputEvent, InputId, KeyCode, KeyEvent};
use parking_lot::Mutex;

/// 虚拟键盘的 input_id（BUS_VIRTUAL，避免被自己的遥控器扫描误认）
pub const VIRTUAL_KBD_NAME: &str = "Voice VibeCoding Virtual Keyboard";
const VIRTUAL_VENDOR: u16 = 0x1d6b;
const VIRTUAL_PRODUCT: u16 = 0x5643; // "VC"

static DEVICE: Mutex<Option<VirtualDevice>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static READY: AtomicBool = AtomicBool::new(false);

/// Windows VK → evdev KEY_*（覆盖界面可录入/可选的全部键）
pub fn vk_to_key(vk: u16) -> Option<KeyCode> {
    use KeyCode as K;
    let k = match vk {
        0x08 => K::KEY_BACKSPACE,
        0x09 => K::KEY_TAB,
        0x0D => K::KEY_ENTER,
        0x10 | 0xA0 => K::KEY_LEFTSHIFT,
        0xA1 => K::KEY_RIGHTSHIFT,
        0x11 | 0xA2 => K::KEY_LEFTCTRL,
        0xA3 => K::KEY_RIGHTCTRL,
        0x12 | 0xA4 => K::KEY_LEFTALT,
        0xA5 => K::KEY_RIGHTALT,
        0x13 => K::KEY_PAUSE,
        0x14 => K::KEY_CAPSLOCK,
        0x1B => K::KEY_ESC,
        0x20 => K::KEY_SPACE,
        0x21 => K::KEY_PAGEUP,
        0x22 => K::KEY_PAGEDOWN,
        0x23 => K::KEY_END,
        0x24 => K::KEY_HOME,
        0x25 => K::KEY_LEFT,
        0x26 => K::KEY_UP,
        0x27 => K::KEY_RIGHT,
        0x28 => K::KEY_DOWN,
        0x2C => K::KEY_SYSRQ,
        0x2D => K::KEY_INSERT,
        0x2E => K::KEY_DELETE,
        0x30 => K::KEY_0,
        0x31 => K::KEY_1,
        0x32 => K::KEY_2,
        0x33 => K::KEY_3,
        0x34 => K::KEY_4,
        0x35 => K::KEY_5,
        0x36 => K::KEY_6,
        0x37 => K::KEY_7,
        0x38 => K::KEY_8,
        0x39 => K::KEY_9,
        0x41 => K::KEY_A,
        0x42 => K::KEY_B,
        0x43 => K::KEY_C,
        0x44 => K::KEY_D,
        0x45 => K::KEY_E,
        0x46 => K::KEY_F,
        0x47 => K::KEY_G,
        0x48 => K::KEY_H,
        0x49 => K::KEY_I,
        0x4A => K::KEY_J,
        0x4B => K::KEY_K,
        0x4C => K::KEY_L,
        0x4D => K::KEY_M,
        0x4E => K::KEY_N,
        0x4F => K::KEY_O,
        0x50 => K::KEY_P,
        0x51 => K::KEY_Q,
        0x52 => K::KEY_R,
        0x53 => K::KEY_S,
        0x54 => K::KEY_T,
        0x55 => K::KEY_U,
        0x56 => K::KEY_V,
        0x57 => K::KEY_W,
        0x58 => K::KEY_X,
        0x59 => K::KEY_Y,
        0x5A => K::KEY_Z,
        0x5B => K::KEY_LEFTMETA,
        0x5C => K::KEY_RIGHTMETA,
        0x5D => K::KEY_COMPOSE,
        0x5F => K::KEY_SLEEP,
        0x60 => K::KEY_KP0,
        0x61 => K::KEY_KP1,
        0x62 => K::KEY_KP2,
        0x63 => K::KEY_KP3,
        0x64 => K::KEY_KP4,
        0x65 => K::KEY_KP5,
        0x66 => K::KEY_KP6,
        0x67 => K::KEY_KP7,
        0x68 => K::KEY_KP8,
        0x69 => K::KEY_KP9,
        0x6A => K::KEY_KPASTERISK,
        0x6B => K::KEY_KPPLUS,
        0x6D => K::KEY_KPMINUS,
        0x6E => K::KEY_KPDOT,
        0x6F => K::KEY_KPSLASH,
        0x70 => K::KEY_F1,
        0x71 => K::KEY_F2,
        0x72 => K::KEY_F3,
        0x73 => K::KEY_F4,
        0x74 => K::KEY_F5,
        0x75 => K::KEY_F6,
        0x76 => K::KEY_F7,
        0x77 => K::KEY_F8,
        0x78 => K::KEY_F9,
        0x79 => K::KEY_F10,
        0x7A => K::KEY_F11,
        0x7B => K::KEY_F12,
        0x7C => K::KEY_F13,
        0x7D => K::KEY_F14,
        0x7E => K::KEY_F15,
        0x7F => K::KEY_F16,
        0x80 => K::KEY_F17,
        0x81 => K::KEY_F18,
        0x82 => K::KEY_F19,
        0x83 => K::KEY_F20,
        0x84 => K::KEY_F21,
        0x85 => K::KEY_F22,
        0x86 => K::KEY_F23,
        0x87 => K::KEY_F24,
        0x90 => K::KEY_NUMLOCK,
        0x91 => K::KEY_SCROLLLOCK,
        0xA6 => K::KEY_BACK,
        0xA7 => K::KEY_FORWARD,
        0xA8 => K::KEY_REFRESH,
        0xAC => K::KEY_HOMEPAGE,
        0xAD => K::KEY_MUTE,
        0xAE => K::KEY_VOLUMEDOWN,
        0xAF => K::KEY_VOLUMEUP,
        0xB0 => K::KEY_NEXTSONG,
        0xB1 => K::KEY_PREVIOUSSONG,
        0xB2 => K::KEY_STOPCD,
        0xB3 => K::KEY_PLAYPAUSE,
        0xB7 => K::KEY_CALC,
        0xBA => K::KEY_SEMICOLON,
        0xBB => K::KEY_EQUAL,
        0xBC => K::KEY_COMMA,
        0xBD => K::KEY_MINUS,
        0xBE => K::KEY_DOT,
        0xBF => K::KEY_SLASH,
        0xC0 => K::KEY_GRAVE,
        0xDB => K::KEY_LEFTBRACE,
        0xDC => K::KEY_BACKSLASH,
        0xDD => K::KEY_RIGHTBRACE,
        0xDE => K::KEY_APOSTROPHE,
        _ => return None,
    };
    Some(k)
}

/// evdev KEY_* → Windows VK（快捷键录入用；左右修饰键保持区分）
pub fn key_to_vk(code: u16) -> Option<u16> {
    // 反查表：遍历 VK 空间即可（0..=0xFE，调用频率极低）
    let preferred_modifier = match code {
        c if c == KeyCode::KEY_LEFTSHIFT.code() => Some(0xA0),
        c if c == KeyCode::KEY_LEFTCTRL.code() => Some(0xA2),
        c if c == KeyCode::KEY_LEFTALT.code() => Some(0xA4),
        c if c == KeyCode::KEY_KPENTER.code() => Some(0x0D),
        c if c == KeyCode::KEY_PRINT.code() => Some(0x2C),
        c if c == KeyCode::KEY_MENU.code() => Some(0x5D),
        _ => None,
    };
    if preferred_modifier.is_some() {
        return preferred_modifier;
    }
    (0u16..=0xFE).find(|&vk| vk_to_key(vk).map(|k| k.code()) == Some(code))
}

fn modifier_rank(vk: u16) -> Option<u8> {
    match vk {
        0x11 | 0xA2 | 0xA3 => Some(0),
        0x10 | 0xA0 | 0xA1 => Some(1),
        0x5B | 0x5C => Some(2),
        0x12 | 0xA4 | 0xA5 => Some(3),
        _ => None,
    }
}

pub fn is_modifier_vk(vk: u16) -> bool {
    modifier_rank(vk).is_some()
}

/// 组合键按下顺序：修饰键（Ctrl→Shift→Super→Alt）在前，主键按原顺序在后
pub fn chord_down_order(vks: &[u16]) -> Vec<u16> {
    let mut mods: Vec<u16> = vks.iter().copied().filter(|v| is_modifier_vk(*v)).collect();
    mods.sort_by_key(|v| modifier_rank(*v).unwrap_or(9));
    let mains = vks.iter().copied().filter(|v| !is_modifier_vk(*v));
    let mut out = Vec::with_capacity(vks.len());
    for v in mods.into_iter().chain(mains) {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

fn all_supported_keys() -> AttributeSet<KeyCode> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for vk in 0u16..=0xFE {
        if let Some(k) = vk_to_key(vk) {
            keys.insert(k);
        }
    }
    for k in [
        KeyCode::KEY_KPENTER,
        KeyCode::KEY_PRINT,
        KeyCode::KEY_MENU,
        KeyCode::KEY_SEARCH,
    ] {
        keys.insert(k);
    }
    keys
}

fn create_device() -> Result<VirtualDevice, String> {
    let keys = all_supported_keys();
    VirtualDevice::builder()
        .map_err(|e| format!("打开 /dev/uinput 失败: {e}（请先运行 linux/setup-system.sh 安装 udev 规则）"))?
        .name(VIRTUAL_KBD_NAME)
        .input_id(InputId::new(BusType::BUS_VIRTUAL, VIRTUAL_VENDOR, VIRTUAL_PRODUCT, 1))
        .with_keys(&keys)
        .map_err(|e| format!("uinput with_keys: {e}"))?
        .build()
        .map_err(|e| format!("创建 uinput 虚拟键盘失败: {e}"))
}

/// 创建（或确认已创建）虚拟键盘。X11/GNOME 需要数百毫秒识别新设备，所以启动时就建好常驻。
pub fn ensure_init() -> bool {
    if READY.load(Ordering::Acquire) {
        return true;
    }
    let mut guard = DEVICE.lock();
    if guard.is_some() {
        READY.store(true, Ordering::Release);
        return true;
    }
    match create_device() {
        Ok(dev) => {
            *guard = Some(dev);
            *LAST_ERROR.lock() = None;
            READY.store(true, Ordering::Release);
            log::info!("LINUX uinput virtual keyboard created");
            true
        }
        Err(e) => {
            // 状态栏每秒会重试一次：同样的错误只记一次日志
            let mut last = LAST_ERROR.lock();
            if last.as_deref() != Some(e.as_str()) {
                log::warn!("LINUX uinput: {e}");
            }
            *last = Some(e);
            false
        }
    }
}

pub fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn last_error() -> Option<String> {
    LAST_ERROR.lock().clone()
}

fn emit_keys(keys: &[(KeyCode, i32)]) -> bool {
    if !ensure_init() {
        return false;
    }
    let events: Vec<InputEvent> = keys
        .iter()
        .map(|(k, v)| *KeyEvent::new(*k, *v))
        .collect();
    let mut guard = DEVICE.lock();
    let Some(dev) = guard.as_mut() else {
        return false;
    };
    match dev.emit(&events) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("LINUX uinput emit failed: {e}");
            *LAST_ERROR.lock() = Some(format!("uinput 写入失败: {e}"));
            false
        }
    }
}

/// 逐键按下（每个键一次 SYN，修饰键之间 4ms，避免桌面把整组当成同一帧而漏判）
pub fn press_chord(vks: &[u16]) -> bool {
    let order = chord_down_order(vks);
    let keys: Vec<KeyCode> = order.iter().filter_map(|vk| vk_to_key(*vk)).collect();
    if keys.is_empty() {
        log::warn!("LINUX uinput: no evdev keys for vks={vks:?}");
        return false;
    }
    let mut ok = true;
    for (i, k) in keys.iter().enumerate() {
        ok &= emit_keys(&[(*k, 1)]);
        if i + 1 < keys.len() {
            std::thread::sleep(Duration::from_millis(4));
        }
    }
    ok
}

/// 逆序抬起
pub fn release_chord(vks: &[u16]) -> bool {
    let order = chord_down_order(vks);
    let keys: Vec<KeyCode> = order.iter().filter_map(|vk| vk_to_key(*vk)).collect();
    let mut ok = true;
    for k in keys.iter().rev() {
        ok &= emit_keys(&[(*k, 0)]);
    }
    ok
}

/// 单击：按下 → 保持 hold_ms → 抬起
pub fn tap_chord(vks: &[u16], hold_ms: u64) -> bool {
    if !press_chord(vks) {
        release_chord(vks);
        return false;
    }
    std::thread::sleep(Duration::from_millis(hold_ms.max(1)));
    release_chord(vks)
}

/// 直接按 evdev 键码单击（粘贴用 Shift+Insert 等）
pub fn tap_keys(keys: &[KeyCode], hold_ms: u64) -> bool {
    let mut ok = true;
    for k in keys {
        ok &= emit_keys(&[(*k, 1)]);
    }
    std::thread::sleep(Duration::from_millis(hold_ms.max(1)));
    for k in keys.iter().rev() {
        ok &= emit_keys(&[(*k, 0)]);
    }
    ok
}

/// 松开虚拟键盘上可能残留按下的全部修饰键（断线/异常兜底）
pub fn release_all_modifiers() {
    if !is_ready() {
        return;
    }
    let mods = [
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_LEFTMETA,
        KeyCode::KEY_RIGHTMETA,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_RIGHTALT,
    ];
    let events: Vec<(KeyCode, i32)> = mods.iter().map(|k| (*k, 0)).collect();
    let _ = emit_keys(&events);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_first_in_ctrl_shift_super_alt_order() {
        assert_eq!(chord_down_order(&[0x5B, 0xA2]), vec![0xA2, 0x5B]);
        assert_eq!(
            chord_down_order(&[0x41, 0xA4, 0xA0, 0x5B, 0xA2]),
            vec![0xA2, 0xA0, 0x5B, 0xA4, 0x41]
        );
        assert_eq!(chord_down_order(&[0xA0, 0x79]), vec![0xA0, 0x79]);
    }

    #[test]
    fn duplicate_vks_collapse() {
        assert_eq!(chord_down_order(&[0xA2, 0xA2, 0x56]), vec![0xA2, 0x56]);
    }

    #[test]
    fn every_ui_vk_maps_to_a_key() {
        let mut ui: Vec<u16> = vec![
            0x08, 0x09, 0x0D, 0x13, 0x14, 0x1B, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
            0x28, 0x2C, 0x2D, 0x2E, 0x5D, 0x90, 0x91, 0x6A, 0x6B, 0x6D, 0x6E, 0x6F, 0x10, 0xA0,
            0xA1, 0x11, 0xA2, 0xA3, 0x12, 0xA4, 0xA5, 0x5B, 0x5C, 0xAD, 0xAE, 0xAF, 0xB0, 0xB1,
            0xB2, 0xB3, 0xB7, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE,
        ];
        ui.extend(0x41..=0x5A);
        ui.extend(0x30..=0x39);
        ui.extend(0x60..=0x69);
        ui.extend(0x70..=0x87);
        for vk in ui {
            assert!(vk_to_key(vk).is_some(), "vk 0x{vk:02X} has no evdev key");
        }
    }

    #[test]
    fn reverse_map_round_trips_sided_modifiers() {
        for vk in [0xA0u16, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C, 0x41, 0x74, 0x0D, 0xAF] {
            let code = vk_to_key(vk).unwrap().code();
            assert_eq!(key_to_vk(code), Some(vk), "vk 0x{vk:02X}");
        }
    }
}
