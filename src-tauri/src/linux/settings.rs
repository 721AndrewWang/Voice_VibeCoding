//! Linux 专属设置（linux.json，与 Windows 共用的 xiaomi.json 分开存放）

use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 语音键说话的去向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    /// 本地离线识别（Qwen3-ASR / SenseVoice），松手后把文字粘贴到当前输入框
    Asr,
    /// 与 Windows 版一致：语音送进虚拟麦克风，同时按住映射的快捷键
    VirtualMic,
}

/// 识别结果上屏方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    /// Shift+Insert：GTK/Qt/Electron/终端都认（同时写 CLIPBOARD 与 PRIMARY）
    ShiftInsert,
    CtrlV,
    CtrlShiftV,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinuxVoiceSettings {
    pub voice_mode: VoiceMode,
    pub asr_model: String,
    /// 识别语言（仅 SenseVoice 使用）：auto / zh / en / yue / ja / ko
    pub asr_language: String,
    pub paste_method: PasteMethod,
    /// 粘贴后把剪贴板恢复成原来的文字
    pub restore_clipboard: bool,
    /// 去掉句末的「。」或「.」（在聊天框里续写时更自然）
    pub strip_trailing_period: bool,
    /// 识别线程数（本机实测 8 线程最快，再多反而变慢）
    pub asr_threads: u32,
    /// 录音时长下限（毫秒），更短视为误触
    pub min_utterance_ms: u32,
    /// 热词（逗号分隔，仅 Qwen3-ASR）：能纠正专有名词，但会让个别英文词被意译成中文
    pub asr_hotwords: String,
    /// 识别后替换，每行「原文 => 替换为」（不区分大小写），用来固定纠正常错的专有名词
    pub asr_replacements: String,
    /// 多少分钟没说话就释放识别模型的内存（0 = 常驻）；再按语音键时后台重新加载
    pub asr_idle_unload_minutes: u32,
}

pub const DEFAULT_ASR_MODEL: &str = "qwen3-asr-0.6b-int8-2026-03-25";
pub const DEFAULT_REPLACEMENTS: &str = "Cloud Code => Claude Code\n";

impl Default for LinuxVoiceSettings {
    fn default() -> Self {
        Self {
            voice_mode: VoiceMode::Asr,
            asr_model: DEFAULT_ASR_MODEL.into(),
            asr_language: "auto".into(),
            paste_method: PasteMethod::ShiftInsert,
            restore_clipboard: true,
            strip_trailing_period: false,
            asr_threads: 8,
            min_utterance_ms: 250,
            asr_hotwords: String::new(),
            asr_replacements: DEFAULT_REPLACEMENTS.into(),
            asr_idle_unload_minutes: 10,
        }
    }
}

impl LinuxVoiceSettings {
    fn normalized(mut self) -> Self {
        self.asr_threads = self.asr_threads.clamp(1, 16);
        self.min_utterance_ms = self.min_utterance_ms.min(3000);
        let lang = self.asr_language.trim().to_ascii_lowercase();
        self.asr_language = match lang.as_str() {
            "zh" | "en" | "yue" | "ja" | "ko" | "auto" => lang,
            _ => "auto".into(),
        };
        if super::asr::model_spec(self.asr_model.trim()).is_none() {
            self.asr_model = DEFAULT_ASR_MODEL.into();
        }
        self.asr_idle_unload_minutes = self.asr_idle_unload_minutes.min(24 * 60);
        self.asr_hotwords = self.asr_hotwords.trim().to_string();
        self
    }
}

static CURRENT: RwLock<Option<LinuxVoiceSettings>> = RwLock::new(None);
static PATH: RwLock<Option<PathBuf>> = RwLock::new(None);

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("linux.json"))
}

/// 启动时加载（损坏文件备份成 .bak 后用默认值）
pub fn init(app: &AppHandle) {
    let path = settings_path(app);
    let loaded = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|raw| match serde_json::from_str::<LinuxVoiceSettings>(&raw) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("LINUX settings parse failed ({e}); using defaults");
                if let Some(p) = &path {
                    let _ = std::fs::rename(p, p.with_extension("json.bak"));
                }
                LinuxVoiceSettings::default()
            }
        })
        .unwrap_or_default()
        .normalized();
    *PATH.write() = path;
    *CURRENT.write() = Some(loaded);
}

pub fn get() -> LinuxVoiceSettings {
    CURRENT.read().clone().unwrap_or_default()
}

pub fn save(settings: LinuxVoiceSettings) -> Result<LinuxVoiceSettings, String> {
    let settings = settings.normalized();
    if let Some(path) = PATH.read().clone() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
        }
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body).map_err(|e| format!("写入 linux.json 失败: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("保存 linux.json 失败: {e}"))?;
    }
    *CURRENT.write() = Some(settings.clone());
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_asr_with_shift_insert() {
        let s = LinuxVoiceSettings::default();
        assert_eq!(s.voice_mode, VoiceMode::Asr);
        assert_eq!(s.paste_method, PasteMethod::ShiftInsert);
        assert!(s.restore_clipboard);
    }

    #[test]
    fn partial_json_fills_defaults_and_normalizes() {
        let s: LinuxVoiceSettings =
            serde_json::from_str(r#"{"voice_mode":"virtual_mic","asr_threads":99,"asr_language":"XX"}"#)
                .unwrap();
        let s = s.normalized();
        assert_eq!(s.voice_mode, VoiceMode::VirtualMic);
        assert_eq!(s.asr_threads, 16);
        assert_eq!(s.asr_language, "auto");
        assert_eq!(s.asr_model, DEFAULT_ASR_MODEL);
        assert!(s.asr_replacements.contains("Claude Code"));
        assert_eq!(s.asr_idle_unload_minutes, 10);
    }
}
