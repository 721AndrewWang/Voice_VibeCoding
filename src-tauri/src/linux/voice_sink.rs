//! Linux 语音汇聚（`voice_pcm` 在 Linux 上的实现）
//!
//! Windows 版把解码后的 PCM 经 UDP 交给 audio_router 子进程写进 VB-CABLE。
//! Linux 版在进程内完成：
//! - 本地识别模式：一句话的原始 PCM 攒在内存里，松手后交给识别线程 → 文字上屏
//! - 虚拟麦克风模式：增益后的 PCM 实时写进 PipeWire 虚拟麦克风

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::settings::VoiceMode;

/// 单句最长 2 分钟（16 kHz）
const MAX_UTTERANCE_SAMPLES: usize = 16_000 * 120;
/// 低于该响度视为没说话（遥控器底噪约 -70 dBFS）
const SILENCE_DBFS: f32 = -58.0;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static BUF: Mutex<Vec<i16>> = Mutex::new(Vec::new());
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
static JOBS: OnceLock<mpsc::Sender<Vec<i16>>> = OnceLock::new();
static LAST_NOTIFY: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrResultEvent {
    pub ok: bool,
    pub text: String,
    pub audio_ms: u64,
    pub asr_ms: u64,
    pub error: Option<String>,
}

fn mode() -> VoiceMode {
    super::settings::get().voice_mode
}

pub fn bind_app(app: AppHandle) {
    *APP.lock() = Some(app);
    let _ = JOBS.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Vec<i16>>();
        std::thread::Builder::new()
            .name("linux-asr-worker".into())
            .spawn(move || {
                for samples in rx {
                    process_utterance(samples);
                }
            })
            .expect("spawn linux-asr-worker");
        tx
    });
}

/// 语音键按下前/启动时：确保当前模式所需的输出就绪
pub fn ensure_ready() -> Result<(), String> {
    match mode() {
        VoiceMode::VirtualMic => super::virtual_mic::ensure(),
        VoiceMode::Asr => {
            if !super::asr::is_loaded() {
                super::asr::ensure_loaded_async();
            }
            Ok(())
        }
    }
}

pub fn is_ready() -> bool {
    match mode() {
        VoiceMode::VirtualMic => super::virtual_mic::is_ready(),
        // 空闲时模型会被释放，说话时再加载：装好了就算就绪
        VoiceMode::Asr => super::asr::is_loaded() || super::asr::status().installed,
    }
}

/// 本地识别模式下不注入语音快捷键（Linux 输入法没有「按住说话」可唤醒）
pub fn hotkey_suppressed_by_mode() -> bool {
    mode() == VoiceMode::Asr
}

/// 一句话开始（对应 Windows 的 CLEAR）
pub fn begin_utterance() {
    BUF.lock().clear();
    *STARTED.lock() = Some(Instant::now());
    ACTIVE.store(true, Ordering::Release);
    if mode() == VoiceMode::Asr {
        // 模型若因空闲被释放：趁用户说话时在后台重新加载（约 2.5 秒，与说话时间重叠）
        super::asr::touch();
        if !super::asr::is_loaded() {
            super::asr::ensure_loaded_async();
        }
    }
    if mode() == VoiceMode::VirtualMic {
        super::virtual_mic::drain();
    }
}

/// 原始解码 PCM（增益前）：给本地识别用，避免 +10 dB 增益削顶
pub fn push_raw(samples: &[i16]) {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let mut buf = BUF.lock();
    if buf.len() + samples.len() <= MAX_UTTERANCE_SAMPLES {
        buf.extend_from_slice(samples);
    }
}

/// 增益后的 PCM：写虚拟麦克风 + 驱动界面波形
pub fn push_gained(samples: &[i16]) {
    let sent = if mode() == VoiceMode::VirtualMic {
        super::virtual_mic::write(samples)
    } else {
        false
    };
    crate::bridges::xiaomi::voice_meter::on_pcm(samples, sent);
}

/// 一句话结束（对应 Windows 的 END）：识别模式下提交识别
pub fn end_utterance() {
    if !ACTIVE.swap(false, Ordering::AcqRel) {
        return;
    }
    let samples = std::mem::take(&mut *BUF.lock());
    if mode() != VoiceMode::Asr {
        return;
    }
    if let Some(tx) = JOBS.get() {
        let _ = tx.send(samples);
    }
}

pub fn stop() {
    ACTIVE.store(false, Ordering::Release);
    BUF.lock().clear();
}

fn emit_result(ev: AsrResultEvent) {
    if let Some(app) = APP.lock().clone() {
        let _ = app.emit("linux-asr-result", ev);
    }
}

fn notify_throttled(title: &str, body: &str) {
    {
        let mut last = LAST_NOTIFY.lock();
        if let Some(t) = *last {
            if t.elapsed() < Duration::from_secs(20) {
                return;
            }
        }
        *last = Some(Instant::now());
    }
    let Some(app) = APP.lock().clone() else {
        return;
    };
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

fn process_utterance(samples: Vec<i16>) {
    let settings = super::settings::get();
    let audio_ms = samples.len() as u64 * 1000 / 16_000;
    if audio_ms < settings.min_utterance_ms as u64 {
        log::info!("LINUX ASR skip: too short ({audio_ms}ms)");
        return;
    }
    let level = super::asr::rms_dbfs(&samples);
    if level < SILENCE_DBFS {
        log::info!("LINUX ASR skip: silent ({level:.1} dBFS, {audio_ms}ms)");
        emit_result(AsrResultEvent {
            ok: false,
            text: String::new(),
            audio_ms,
            asr_ms: 0,
            error: Some("没有听到声音".into()),
        });
        return;
    }
    let t0 = Instant::now();
    match super::asr::recognize(&samples) {
        Ok(raw) => {
            let asr_ms = t0.elapsed().as_millis() as u64;
            let text = super::asr::postprocess_text(
                &raw,
                settings.strip_trailing_period,
                &settings.asr_replacements,
            );
            log::info!("LINUX ASR ok audio={audio_ms}ms asr={asr_ms}ms level={level:.1}dBFS chars={}", text.chars().count());
            if text.is_empty() {
                emit_result(AsrResultEvent {
                    ok: false,
                    text,
                    audio_ms,
                    asr_ms,
                    error: Some("没有识别出文字".into()),
                });
                return;
            }
            let commit = super::text_commit::commit_text(&text);
            if let Err(e) = &commit {
                log::warn!("LINUX ASR commit failed: {e}");
                notify_throttled("文字上屏失败", e);
            }
            emit_result(AsrResultEvent {
                ok: commit.is_ok(),
                text,
                audio_ms,
                asr_ms,
                error: commit.err(),
            });
        }
        Err(e) => {
            log::warn!("LINUX ASR failed: {e}");
            notify_throttled(
                "语音识别不可用",
                &format!("{e}。请打开 Voice VibeCoding，在小米页面下载语音识别模型。"),
            );
            emit_result(AsrResultEvent {
                ok: false,
                text: String::new(),
                audio_ms,
                asr_ms: 0,
                error: Some(e),
            });
        }
    }
}
