//! PipeWire 虚拟麦克风（替代 Windows 的 VB-CABLE）
//!
//! 用 PipeWire 自带的 pulse 兼容模块 `module-pipe-source` 建一个音源：
//! 本进程往 FIFO 写 16 kHz / 单声道 / s16le PCM，任何录音软件把麦克风选成
//! 「Voice VibeCoding 遥控器麦克风」即可收到遥控器语音。无需 root，不装驱动。
//!
//! 注意：音源空闲（没人录音）时 PipeWire 不读 FIFO，写入的数据会堆在管道里；
//! 所以每次说话开始前先把管道里的旧数据读空（FIFO 以 O_RDWR 打开，自己能读）。

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

pub const SOURCE_NAME: &str = "voice_vibecoding_mic";
pub const SOURCE_DESCRIPTION: &str = "Voice VibeCoding 遥控器麦克风";
pub const SAMPLE_RATE: u32 = 16_000;

struct MicState {
    fifo: File,
    module_id: Option<u32>,
}

static MIC: Mutex<Option<MicState>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static WRITTEN: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

fn fifo_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("voice-vibecoding-mic.fifo")
}

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| format!("无法运行 pactl: {e}（需要 pipewire-pulse 或 pulseaudio-utils）"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "pactl {} 失败: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// 找到本软件以前加载过、仍然存在的 pipe-source 模块（上次异常退出残留）
fn existing_modules() -> Vec<u32> {
    let Ok(list) = pactl(&["list", "short", "modules"]) else {
        return Vec::new();
    };
    list.lines()
        .filter(|l| l.contains("module-pipe-source") && l.contains(SOURCE_NAME))
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

fn source_exists() -> bool {
    pactl(&["list", "short", "sources"])
        .map(|s| s.lines().any(|l| l.split_whitespace().nth(1) == Some(SOURCE_NAME)))
        .unwrap_or(false)
}

fn load_module() -> Result<u32, String> {
    let fifo = fifo_path();
    let _ = std::fs::remove_file(&fifo);
    let file_arg = format!("file={}", fifo.display());
    let props = format!("source_properties=device.description='{SOURCE_DESCRIPTION}'");
    let rate = format!("rate={SAMPLE_RATE}");
    let out = pactl(&[
        "load-module",
        "module-pipe-source",
        &format!("source_name={SOURCE_NAME}"),
        &file_arg,
        "format=s16le",
        &rate,
        "channels=1",
        &props,
    ])?;
    out.trim()
        .parse::<u32>()
        .map_err(|_| format!("pactl load-module 返回异常: {out}"))
}

fn open_fifo() -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(fifo_path())
        .map_err(|e| format!("打开虚拟麦克风管道失败: {e}"))
}

/// 确保虚拟麦克风存在并已打开管道（可重复调用；热路径只看内存状态）
pub fn ensure() -> Result<(), String> {
    let mut guard = MIC.lock();
    if guard.is_some() {
        return Ok(());
    }

    let result: Result<MicState, String> = (|| {
        // 残留模块：管道文件可能已被删，直接卸掉重建最稳
        for id in existing_modules() {
            let _ = pactl(&["unload-module", &id.to_string()]);
        }
        let module_id = load_module()?;
        let fifo = open_fifo()?;
        log::info!("LINUX virtual mic ready source={SOURCE_NAME} module={module_id}");
        Ok(MicState {
            fifo,
            module_id: Some(module_id),
        })
    })();

    match result {
        Ok(state) => {
            *guard = Some(state);
            *LAST_ERROR.lock() = None;
            Ok(())
        }
        Err(e) => {
            log::warn!("LINUX virtual mic: {e}");
            *LAST_ERROR.lock() = Some(e.clone());
            Err(e)
        }
    }
}

pub fn is_ready() -> bool {
    MIC.lock().is_some()
}

/// 核对 PipeWire 里音源是否还在（PipeWire 重启后模块会消失）；不在则丢弃状态，
/// 下次 `ensure()` 重建。调用方自行节流（会起 pactl 子进程）。
pub fn verify() -> bool {
    if MIC.lock().is_none() {
        return false;
    }
    if source_exists() {
        return true;
    }
    log::warn!("LINUX virtual mic source vanished (PipeWire restarted?) — will recreate");
    *MIC.lock() = None;
    false
}

pub fn last_error() -> Option<String> {
    LAST_ERROR.lock().clone()
}

/// 说话开始前清掉管道里堆积的旧数据
pub fn drain() {
    let mut guard = MIC.lock();
    let Some(state) = guard.as_mut() else {
        return;
    };
    let mut buf = [0u8; 16 * 1024];
    let mut drained = 0usize;
    loop {
        match state.fifo.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => drained += n,
            Err(_) => break,
        }
        if drained > 1 << 20 {
            break;
        }
    }
    if drained > 0 {
        log::debug!("LINUX virtual mic drained {drained} stale bytes");
    }
}

/// 写入 16 kHz 单声道 PCM；管道满（没人读）时直接丢弃，不阻塞语音线程
pub fn write(samples: &[i16]) -> bool {
    let mut guard = MIC.lock();
    let Some(state) = guard.as_mut() else {
        return false;
    };
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    match state.fifo.write(&bytes) {
        Ok(n) if n == bytes.len() => {
            WRITTEN.fetch_add(1, Ordering::Relaxed);
            true
        }
        Ok(_) | Err(_) => {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            false
        }
    }
}

pub fn stats() -> (u64, u64) {
    (WRITTEN.load(Ordering::Relaxed), DROPPED.load(Ordering::Relaxed))
}

/// 清掉上次异常退出残留的音源（本地识别模式用不到虚拟麦克风）
pub fn cleanup_stale() {
    if MIC.lock().is_some() {
        return;
    }
    for id in existing_modules() {
        let _ = pactl(&["unload-module", &id.to_string()]);
        log::info!("LINUX virtual mic: removed stale module {id}");
    }
    let _ = std::fs::remove_file(fifo_path());
}

/// 退出时卸载模块（不影响其它音频设备）
pub fn remove() {
    let state = MIC.lock().take();
    if let Some(state) = state {
        drop(state.fifo);
        if let Some(id) = state.module_id {
            let _ = pactl(&["unload-module", &id.to_string()]);
        }
        let _ = std::fs::remove_file(fifo_path());
        log::info!("LINUX virtual mic removed");
    }
}
