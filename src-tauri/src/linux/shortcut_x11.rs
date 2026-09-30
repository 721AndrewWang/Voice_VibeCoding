//! 快捷键录入（X11）
//!
//! Windows 版靠低级键盘钩子「吞键 + 识别」。X11 上的对等做法是在根窗口上
//! `XGrabKeyboard`：录入期间所有按键只发给本进程，系统快捷键（Super 打开
//! 活动概览等）不会被触发，也不会漏到其它窗口。每个按键翻译成 Windows VK
//! 后交给与 Windows 共用的录入引擎（`shortcut_capture::try_swallow_capture_key`），
//! 录入结果、进度事件、轮询接口完全不变。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, GrabStatus};
use x11rb::protocol::Event;
use x11rb::CURRENT_TIME;

use crate::bridges::shared::shortcut_capture;

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;

static STOP: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// 录入进行中（遥控器按键此时只高亮不注入）
pub fn is_capture_active() -> bool {
    shortcut_capture::is_swallow_active()
}

/// X keycode → Windows VK（X keycode = evdev 键码 + 8）
fn x_keycode_to_vk(detail: u8) -> Option<u32> {
    let evdev = (detail as u16).checked_sub(8)?;
    super::uinput_kbd::key_to_vk(evdev).map(u32::from)
}

pub fn start() -> Result<(), String> {
    stop();
    STOP.store(false, Ordering::Release);
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let handle = std::thread::Builder::new()
        .name("linux-shortcut-grab".into())
        .spawn(move || {
            RUNNING.store(true, Ordering::Release);
            grab_loop(ready_tx);
            RUNNING.store(false, Ordering::Release);
        })
        .map_err(|e| format!("启动录入线程失败: {e}"))?;
    *THREAD.lock() = Some(handle);
    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(r) => r,
        Err(_) => Err("获取键盘独占超时".into()),
    }
}

pub fn stop() {
    STOP.store(true, Ordering::Release);
    if let Some(h) = THREAD.lock().take() {
        let deadline = Instant::now() + Duration::from_millis(800);
        while RUNNING.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !RUNNING.load(Ordering::Acquire) {
            let _ = h.join();
        }
    }
}

fn grab_loop(ready: mpsc::Sender<Result<(), String>>) {
    let (conn, screen_num) = match x11rb::connect(None) {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!(
                "无法连接 X11 显示（录入快捷键目前只支持 X11 会话）: {e}"
            )));
            return;
        }
    };
    let root = conn.setup().roots[screen_num].root;

    // 其它程序（如打开的菜单）正持有独占时会返回 AlreadyGrabbed，稍等重试
    let mut grabbed = false;
    let mut last_status = String::new();
    for _ in 0..40 {
        let reply = conn
            .grab_keyboard(false, root, CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)
            .ok()
            .and_then(|c| c.reply().ok());
        match reply.map(|r| r.status) {
            Some(GrabStatus::SUCCESS) => {
                grabbed = true;
                break;
            }
            other => last_status = format!("{other:?}"),
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !grabbed {
        let _ = ready.send(Err(format!("无法独占键盘（{last_status}），请关闭弹出菜单后重试")));
        return;
    }
    let _ = ready.send(Ok(()));
    log::info!("LINUX shortcut capture: keyboard grabbed");

    let mut pending: Option<Event> = None;
    loop {
        if STOP.load(Ordering::Acquire) || !shortcut_capture::is_swallow_active() {
            break;
        }
        let ev = match pending.take() {
            Some(e) => Some(e),
            None => match conn.poll_for_event() {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("LINUX shortcut capture: X connection error: {e}");
                    break;
                }
            },
        };
        let Some(ev) = ev else {
            std::thread::sleep(Duration::from_millis(4));
            continue;
        };
        match ev {
            Event::KeyPress(k) => {
                if let Some(vk) = x_keycode_to_vk(k.detail) {
                    shortcut_capture::try_swallow_capture_key(vk, WM_KEYDOWN, false);
                }
            }
            Event::KeyRelease(k) => {
                // X 自动重复 = 紧挨着的一对 Release+Press（同键码同时间戳），整对忽略
                let next = conn.poll_for_event().ok().flatten();
                if let Some(Event::KeyPress(n)) = &next {
                    if n.detail == k.detail && n.time == k.time {
                        continue;
                    }
                }
                pending = next;
                if let Some(vk) = x_keycode_to_vk(k.detail) {
                    shortcut_capture::try_swallow_capture_key(vk, WM_KEYUP, false);
                }
            }
            _ => {}
        }
    }

    let _ = conn.ungrab_keyboard(CURRENT_TIME);
    let _ = conn.flush();
    log::info!("LINUX shortcut capture: keyboard released");
}
