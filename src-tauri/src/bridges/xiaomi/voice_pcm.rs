//! 语音 PCM 出口
//! - Windows：16k→48k 后 UDP 送独立 audio_router 进程 → VB-CABLE（`voice_pcm_udp.rs`）
//! - Linux：进程内处理 → PipeWire 虚拟麦克风 / 本地识别（`voice_pcm_linux.rs`）

#[cfg(not(target_os = "linux"))]
#[path = "voice_pcm_udp.rs"]
mod imp;
#[cfg(target_os = "linux")]
#[path = "voice_pcm_linux.rs"]
mod imp;

pub use imp::*;
