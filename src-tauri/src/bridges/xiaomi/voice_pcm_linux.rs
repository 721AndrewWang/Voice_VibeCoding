//! `voice_pcm` 的 Linux 实现：接口与 Windows 版一致，内部转给 `crate::linux::voice_sink`
//!
//! - `clear()`       → 一句话开始（清缓冲、清虚拟麦克风管道里的旧数据）
//! - `push_raw_16k`  → 增益前 PCM，攒给本地识别
//! - `push_16k`      → 增益后 PCM，写虚拟麦克风 + 界面波形
//! - `end_session()` → 一句话结束，本地识别模式下提交识别并上屏

use crate::linux::voice_sink;

pub const PING_RETRY_INTERVAL_MS: u64 = 15;
pub const PING_DEADLINE_SECS: u64 = 4;

pub fn ping_retry_interval_ms() -> u64 {
    PING_RETRY_INTERVAL_MS
}

pub fn ping_deadline_secs() -> u64 {
    PING_DEADLINE_SECS
}

pub fn ensure_pcm_ready_on_press() {
    if let Err(e) = voice_sink::ensure_ready() {
        log::warn!("LINUX VOICE output not ready on press: {e}");
    }
}

pub fn ensure_started() -> Result<(), String> {
    voice_sink::ensure_ready()
}

pub fn warmup_async() {
    std::thread::Builder::new()
        .name("linux-voice-warmup".into())
        .spawn(|| {
            if let Err(e) = ensure_started() {
                log::debug!("LINUX VOICE warmup: {e}");
            }
        })
        .ok();
}

pub fn is_ready() -> bool {
    voice_sink::is_ready()
}

pub fn clear() {
    voice_sink::begin_utterance();
}

pub fn end_session() {
    voice_sink::end_utterance();
}

pub fn push_16k(samples: &[i16]) {
    if samples.is_empty() {
        return;
    }
    voice_sink::push_gained(samples);
}

pub fn push_raw_16k(samples: &[i16]) {
    voice_sink::push_raw(samples);
}

pub fn stop() {
    voice_sink::stop();
    crate::bridges::xiaomi::voice_meter::set_session(false);
}

pub fn stats() -> (u64, u64) {
    crate::linux::virtual_mic::stats()
}
