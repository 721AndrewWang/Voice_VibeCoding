//! Linux 平台后端
//!
//! | Windows 版                         | Linux 版（本目录）                          |
//! |-----------------------------------|--------------------------------------------|
//! | WinRT BLE + ATVV                  | `bluez`：BlueZ D-Bus（bluer）               |
//! | HID Tap（Frida 注入 WUDFHost）+ LL 钩子 | `remote_input`：evdev 独占遥控器输入节点  |
//! | WinUHid / SendInput               | `uinput_kbd`：/dev/uinput 虚拟键盘          |
//! | VB-CABLE + audio_router 子进程     | `virtual_mic`：PipeWire pipe-source 音源    |
//! | 输入法语音听写（微信/豆包/千问）    | `asr` + `text_commit`：本地 SenseVoice 识别后粘贴 |
//! | 低级钩子录快捷键                   | `shortcut_x11`：XGrabKeyboard               |
//! | 注册表 Run 自启                    | `autostart`：XDG autostart                  |

pub mod asr;
pub mod autostart;
pub mod bluez;
pub mod host_status;
pub mod remote_input;
pub mod settings;
pub mod shortcut_x11;
pub mod simulate;
pub mod text_commit;
pub mod uinput_kbd;
pub mod virtual_mic;
pub mod voice_sink;

use tauri::AppHandle;

/// 应用启动时调用（Tauri setup 内）
pub fn init(app: &AppHandle) {
    use tauri::Manager;
    if let Some(rt) = app.try_state::<std::sync::Arc<crate::bridges::xiaomi::connect::XiaomiRuntime>>() {
        bluez::bind_runtime(std::sync::Arc::clone(&rt));
    }
    settings::init(app);
    asr::init(app);
    voice_sink::bind_app(app.clone());
    crate::bridges::xiaomi::key_mapping::bind_voice_hook_app(app.clone());
    crate::bridges::xiaomi::key_log::bind_key_output_app(app.clone());

    // 虚拟键盘要提前建好：X11 识别新设备需要几百毫秒，别等到第一次按键
    std::thread::Builder::new()
        .name("linux-init".into())
        .spawn(|| {
            uinput_kbd::ensure_init();
            if settings::get().voice_mode == settings::VoiceMode::Asr {
                virtual_mic::cleanup_stale();
            }
            if let Err(e) = voice_sink::ensure_ready() {
                log::warn!("LINUX voice output not ready: {e}");
            }
            if settings::get().voice_mode == settings::VoiceMode::Asr {
                let _ = asr::ensure_loaded();
            }
        })
        .ok();

    remote_input::start(app.clone());
    install_signal_handlers(app.clone());
    log::info!(
        "LINUX backend initialised (session={})",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into())
    );
}

static SIGNAL_PIPE_WR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn on_termination_signal(_sig: libc::c_int) {
    // 信号处理函数里只做 async-signal-safe 的 write，真正的退出交给普通线程
    let fd = SIGNAL_PIPE_WR.load(std::sync::atomic::Ordering::Relaxed);
    if fd >= 0 {
        let b = 1u8;
        unsafe { libc::write(fd, (&b as *const u8).cast(), 1) };
    }
}

/// 注销登录 / `kill` 时走正常退出流程，保证虚拟麦克风模块被卸载、快捷键被松开
pub fn install_signal_handlers(app: AppHandle) {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        log::warn!("LINUX: pipe2 for signal handling failed");
        return;
    }
    SIGNAL_PIPE_WR.store(fds[1], std::sync::atomic::Ordering::Relaxed);
    let handler = on_termination_signal as extern "C" fn(libc::c_int);
    unsafe {
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, handler as libc::sighandler_t);
        libc::signal(libc::SIGHUP, handler as libc::sighandler_t);
    }
    let read_fd = fds[0];
    std::thread::Builder::new()
        .name("linux-signals".into())
        .spawn(move || {
            let mut b = [0u8; 1];
            loop {
                let n = unsafe { libc::read(read_fd, b.as_mut_ptr().cast(), 1) };
                if n == 1 {
                    break;
                }
                if n < 0
                    && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                {
                    continue;
                }
                return;
            }
            log::info!("LINUX: termination signal received — exiting cleanly");
            crate::ipc::tray::quit_app_public(&app);
        })
        .ok();
}

/// 退出前清理
pub fn shutdown() {
    uinput_kbd::release_all_modifiers();
    virtual_mic::remove();
}
