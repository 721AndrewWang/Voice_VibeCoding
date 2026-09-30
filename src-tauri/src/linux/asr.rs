//! 本地离线语音识别（sherpa-onnx）
//!
//! Linux 上没有微信/豆包输入法的语音听写，所以语音键默认走本机识别：
//! 按住说话 → 松手后在本机 CPU 上识别 → 文字粘贴到当前输入框。
//!
//! 可选模型（本机中英混说实测，16 句 vibe coding 口述，模拟遥控器 ADPCM 通道）：
//! - Qwen3-ASR 0.6B：字错误率 5.7%、英文术语命中 86%，单句约 0.9 s，常驻内存约 1.9 GB
//! - SenseVoice Small：字错误率 14%、英文术语命中 64%，单句约 0.07 s，内存约 330 MB
//!
//! 模型放在 `<app_data_dir>/models/<id>/`，可在界面里一键下载；
//! 一段时间没说话会释放内存，按下语音键时后台重新加载（与说话时间重叠）。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    OfflineQwen3ASRModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};
use tauri::{AppHandle, Emitter, Manager};

pub struct ModelFile {
    /// 相对模型目录的路径（可含子目录，如 tokenizer/vocab.json）
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    SenseVoice,
    Qwen3Asr,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub kind: ModelKind,
    pub title: &'static str,
    /// 界面上的一句话说明
    pub note: &'static str,
    pub ram_mb: u32,
    /// HuggingFace 仓库（也走 hf-mirror.com 镜像）
    pub repo: &'static str,
    pub files: &'static [ModelFile],
}

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "qwen3-asr-0.6b-int8-2026-03-25",
        kind: ModelKind::Qwen3Asr,
        title: "Qwen3-ASR 0.6B（推荐：中英混说准）",
        note: "中英混说、技术词准确率最高；松手后约 1 秒出字，占内存约 1.9GB",
        ram_mb: 1900,
        repo: "csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25",
        files: &[
            ModelFile {
                name: "conv_frontend.onnx",
                size: 44_148_281,
                sha256: "d22dc4423e0940e49884e903d2ea2f7e5567c14fc1aed97e4e26d6b8f208ef9e",
            },
            ModelFile {
                name: "encoder.int8.onnx",
                size: 182_491_662,
                sha256: "60748d3e6744a57c9c91e1b17424a6c2990567e8adceb0783940c03ed98fa9d9",
            },
            ModelFile {
                name: "decoder.int8.onnx",
                size: 755_914_231,
                sha256: "4f6885be5959ae26af3089d38ee7972c5fafbeeb1cf8d5e76eab6d8b61ca5771",
            },
            ModelFile {
                name: "tokenizer/merges.txt",
                size: 1_671_853,
                sha256: "8831e4f1a044471340f7c0a83d7bd71306a5b867e95fd870f74d0c5308a904d5",
            },
            ModelFile {
                name: "tokenizer/tokenizer_config.json",
                size: 12_487,
                sha256: "4942d005604266809309cabc9f4e9cb89ce855d59b14681fdc0e1cc62ea26c4c",
            },
            ModelFile {
                name: "tokenizer/vocab.json",
                size: 2_776_833,
                sha256: "ca10d7e9fb3ed18575dd1e277a2579c16d108e32f27439684afa0e10b1440910",
            },
        ],
    },
    ModelSpec {
        id: "sense-voice-zh-en-ja-ko-yue-int8-2024-07-17",
        kind: ModelKind::SenseVoice,
        title: "SenseVoice Small（极速）",
        note: "几乎瞬间出字、内存小；英文技术词容易错（如 cargo build 识别成 cargobi）",
        ram_mb: 330,
        repo: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        files: &[
            ModelFile {
                name: "model.int8.onnx",
                size: 239_233_841,
                sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
            },
            ModelFile {
                name: "tokens.txt",
                size: 315_894,
                sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
            },
        ],
    },
];

pub fn model_spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// 识别器与构建它时用的参数（参数变了就重建）
struct Engine {
    model_id: String,
    language: String,
    threads: u32,
    hotwords: String,
    recognizer: Arc<OfflineRecognizer>,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static LOADING: AtomicBool = AtomicBool::new(false);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static DOWNLOADING: Mutex<Option<String>> = Mutex::new(None);
static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);
static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static LAST_USED: Mutex<Option<Instant>> = Mutex::new(None);

pub fn init(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        *MODELS_DIR.lock() = Some(dir.join("models"));
    }
    start_idle_reaper();
}

pub fn models_dir() -> Option<PathBuf> {
    MODELS_DIR.lock().clone()
}

fn model_dir(spec: &ModelSpec) -> Option<PathBuf> {
    models_dir().map(|d| d.join(spec.id))
}

/// 文件齐全且大小对得上即视为已安装（sha256 在下载时校验过）
pub fn is_installed(spec: &ModelSpec) -> bool {
    let Some(dir) = model_dir(spec) else {
        return false;
    };
    spec.files.iter().all(|f| {
        std::fs::metadata(dir.join(f.name))
            .map(|m| m.len() == f.size)
            .unwrap_or(false)
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrStatus {
    pub model_id: String,
    pub model_title: String,
    pub installed: bool,
    pub loaded: bool,
    pub loading: bool,
    pub downloading: bool,
    /// 正在下载的模型 id（可能不是当前选中的）
    pub downloading_model: Option<String>,
    pub error: Option<String>,
    pub models_dir: Option<String>,
}

pub fn status() -> AsrStatus {
    let settings = super::settings::get();
    let spec = model_spec(&settings.asr_model).unwrap_or(&MODELS[0]);
    let loaded = ENGINE
        .lock()
        .as_ref()
        .map(|e| e.model_id == spec.id)
        .unwrap_or(false);
    let downloading_model = DOWNLOADING.lock().clone();
    AsrStatus {
        model_id: spec.id.to_string(),
        model_title: spec.title.to_string(),
        installed: is_installed(spec),
        loaded,
        loading: LOADING.load(Ordering::Acquire),
        downloading: downloading_model.as_deref() == Some(spec.id),
        downloading_model,
        error: LAST_ERROR.lock().clone(),
        models_dir: models_dir().map(|d| d.display().to_string()),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub title: String,
    pub note: String,
    pub installed: bool,
    pub size_mb: u64,
    pub ram_mb: u32,
    pub supports_hotwords: bool,
    pub supports_language: bool,
}

/// 界面上的模型列表
pub fn models_overview() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|m| ModelInfo {
            id: m.id.into(),
            title: m.title.into(),
            note: m.note.into(),
            installed: is_installed(m),
            size_mb: m.files.iter().map(|f| f.size).sum::<u64>() / 1_000_000,
            ram_mb: m.ram_mb,
            supports_hotwords: m.kind == ModelKind::Qwen3Asr,
            supports_language: m.kind == ModelKind::SenseVoice,
        })
        .collect()
}

pub fn is_loaded() -> bool {
    ENGINE.lock().is_some()
}

/// 记一次「正在用」：推迟空闲释放
pub fn touch() {
    *LAST_USED.lock() = Some(Instant::now());
}

/// 空闲超过设定分钟数就释放识别器（Qwen3-ASR 常驻约 1.9GB）
fn start_idle_reaper() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::Builder::new()
        .name("linux-asr-idle".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            let minutes = super::settings::get().asr_idle_unload_minutes;
            if minutes == 0 || LOADING.load(Ordering::Acquire) {
                continue;
            }
            let idle = LAST_USED
                .lock()
                .map(|t| t.elapsed() >= Duration::from_secs(minutes as u64 * 60))
                .unwrap_or(true);
            if idle {
                let engine = ENGINE.lock().take();
                if let Some(e) = engine {
                    let id = e.model_id.clone();
                    drop(e);
                    release_memory();
                    log::info!("LINUX ASR unloaded {id} after {minutes} min idle");
                }
            }
        })
        .ok();
}

/// 释放识别器后把 glibc 堆里空出来的页还给系统：
/// 加载线程每次都是新线程、落在不同的 malloc arena，不 trim 的话每释放/重载一轮 RSS 就涨几百 MB
fn release_memory() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::malloc_trim(0);
    }
}

fn build_recognizer(
    spec: &ModelSpec,
    language: &str,
    threads: u32,
    hotwords: &str,
) -> Result<OfflineRecognizer, String> {
    let dir = model_dir(spec).ok_or("无法确定模型目录")?;
    if !is_installed(spec) {
        return Err(format!("语音识别模型未安装：{}", spec.title));
    }
    let path = |f: &str| Some(dir.join(f).display().to_string());
    let mut config = OfflineRecognizerConfig::default();
    match spec.kind {
        ModelKind::SenseVoice => {
            config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
                model: path("model.int8.onnx"),
                language: Some(language.to_string()),
                use_itn: true,
            };
            config.model_config.tokens = path("tokens.txt");
        }
        ModelKind::Qwen3Asr => {
            config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
                conv_frontend: path("conv_frontend.onnx"),
                encoder: path("encoder.int8.onnx"),
                decoder: path("decoder.int8.onnx"),
                tokenizer: path("tokenizer"),
                hotwords: Some(hotwords.to_string()).filter(|h| !h.is_empty()),
                ..Default::default()
            };
        }
    }
    config.model_config.num_threads = threads as i32;
    config.model_config.provider = Some("cpu".into());
    OfflineRecognizer::create(&config).ok_or_else(|| "sherpa-onnx 创建识别器失败（模型文件损坏？）".into())
}

/// 按当前设置加载识别器（已加载同配置时立即返回）
pub fn ensure_loaded() -> Result<(), String> {
    let settings = super::settings::get();
    let spec = model_spec(&settings.asr_model).ok_or("未知的语音识别模型")?;
    let hotwords = if spec.kind == ModelKind::Qwen3Asr {
        settings.asr_hotwords.clone()
    } else {
        String::new()
    };
    {
        let guard = ENGINE.lock();
        if let Some(e) = guard.as_ref() {
            if e.model_id == spec.id
                && e.language == settings.asr_language
                && e.threads == settings.asr_threads
                && e.hotwords == hotwords
            {
                return Ok(());
            }
        }
    }
    if LOADING.swap(true, Ordering::AcqRel) {
        // 另一个线程正在加载：等它完成
        let start = Instant::now();
        while LOADING.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(50));
        }
        return if is_loaded() {
            Ok(())
        } else {
            Err(LAST_ERROR.lock().clone().unwrap_or_else(|| "语音识别模型加载失败".into()))
        };
    }
    // 先释放旧识别器，避免新旧两个大模型同时占内存
    let old = ENGINE.lock().take();
    if old.is_some() {
        drop(old);
        release_memory();
    }
    let t0 = Instant::now();
    let result = build_recognizer(spec, &settings.asr_language, settings.asr_threads, &hotwords);
    let out = match result {
        Ok(recognizer) => {
            *ENGINE.lock() = Some(Engine {
                model_id: spec.id.to_string(),
                language: settings.asr_language.clone(),
                threads: settings.asr_threads,
                hotwords,
                recognizer: Arc::new(recognizer),
            });
            *LAST_ERROR.lock() = None;
            touch();
            log::info!(
                "LINUX ASR loaded model={} lang={} threads={} in {:?}",
                spec.id,
                settings.asr_language,
                settings.asr_threads,
                t0.elapsed()
            );
            Ok(())
        }
        Err(e) => {
            log::warn!("LINUX ASR load failed: {e}");
            *LAST_ERROR.lock() = Some(e.clone());
            Err(e)
        }
    };
    LOADING.store(false, Ordering::Release);
    out
}

pub fn ensure_loaded_async() {
    std::thread::Builder::new()
        .name("linux-asr-load".into())
        .spawn(|| {
            let _ = ensure_loaded();
        })
        .ok();
}

/// 设置变更（模型/语言/线程/热词）后丢弃旧识别器
pub fn invalidate() {
    let old = ENGINE.lock().take();
    if old.is_some() {
        drop(old);
        release_memory();
    }
}

/// 把遥控器 PCM 归一化到 [-1, 1] 的 f32，并把峰值拉到约 -3 dBFS（遥控器原始电平偏低）
pub fn normalize_for_asr(samples: &[i16]) -> Vec<f32> {
    let peak = samples
        .iter()
        .map(|s| (*s as i32).unsigned_abs())
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    let target = 0.7 * 32768.0;
    // 最多放大 30 dB，避免把底噪放成一片
    let gain = (target / peak).clamp(1.0, 31.6);
    samples
        .iter()
        .map(|s| ((*s as f32) * gain / 32768.0).clamp(-1.0, 1.0))
        .collect()
}

pub fn rms_dbfs(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -120.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sum / samples.len() as f64).sqrt() / 32768.0;
    if rms <= 0.0 {
        -120.0
    } else {
        20.0 * (rms as f32).log10()
    }
}

/// 识别 16 kHz 单声道 PCM
pub fn recognize(samples: &[i16]) -> Result<String, String> {
    touch();
    ensure_loaded()?;
    let recognizer = ENGINE
        .lock()
        .as_ref()
        .map(|e| Arc::clone(&e.recognizer))
        .ok_or("语音识别未就绪")?;
    let audio = normalize_for_asr(samples);
    let stream = recognizer.create_stream();
    stream.accept_waveform(16_000, &audio);
    recognizer.decode(&stream);
    touch();
    let text = stream
        .get_result()
        .map(|r| r.text)
        .ok_or("识别无结果")?;
    Ok(text.trim().to_string())
}

/// 「P R」「S Q L」这种逐个念出来的大写字母合并成「PR」「SQL」
pub fn collapse_spelled_letters(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let single = |i: usize| {
        c[i].is_ascii_uppercase()
            && (i == 0 || !c[i - 1].is_ascii_alphanumeric())
            && (i + 1 >= c.len() || !c[i + 1].is_ascii_alphanumeric())
    };
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if single(i) {
            let mut j = i;
            while j + 2 < c.len() && c[j + 1] == ' ' && single(j + 2) {
                j += 2;
            }
            if j > i {
                for k in (i..=j).step_by(2) {
                    out.push(c[k]);
                }
                i = j + 1;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// 按「原文 => 替换为」逐行替换（ASCII 部分不区分大小写）
pub fn apply_replacements(text: &str, rules: &str) -> String {
    let mut out = text.to_string();
    for line in rules.lines() {
        let Some((from, to)) = line.split_once("=>") else {
            continue;
        };
        let (from, to) = (from.trim(), to.trim());
        if from.is_empty() {
            continue;
        }
        let hay = out.to_ascii_lowercase();
        let needle = from.to_ascii_lowercase();
        let mut result = String::with_capacity(out.len());
        let mut last = 0;
        for (pos, _) in hay.match_indices(&needle) {
            result.push_str(&out[last..pos]);
            result.push_str(to);
            last = pos + needle.len();
        }
        result.push_str(&out[last..]);
        out = result;
    }
    out
}

/// 识别结果后处理：合并拆开的字母 → 替换表 → 句末句号
pub fn postprocess_text(text: &str, strip_trailing_period: bool, replacements: &str) -> String {
    let mut t = collapse_spelled_letters(text.trim());
    t = apply_replacements(&t, replacements);
    if strip_trailing_period {
        while t.ends_with('。') || t.ends_with('.') {
            t.pop();
        }
    }
    t
}

// ---------------------------------------------------------------------------
// 模型下载（多线程分段，走 hf-mirror.com / huggingface.co，校验 sha256）
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
    percent: f64,
    speed_bps: u64,
    source: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build()
}

fn agent_direct() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build()
}

/// 下载源：(显示名, URL 前缀, 是否走代理环境变量)
fn sources(repo: &str) -> Vec<(&'static str, String, bool)> {
    vec![
        ("hf-mirror.com", format!("https://hf-mirror.com/{repo}/resolve/main"), false),
        ("huggingface.co", format!("https://huggingface.co/{repo}/resolve/main"), true),
    ]
}

fn fetch_range(
    agent: &ureq::Agent,
    url: &str,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let resp = agent
        .get(url)
        .set("Range", &format!("bytes={start}-{end}"))
        .set("User-Agent", "VoiceVibeCoding-Linux")
        .call()
        .map_err(|e| e.to_string())?;
    let want = (end - start + 1) as usize;
    let mut buf = Vec::with_capacity(want);
    let mut reader = resp.into_reader().take(want as u64 + 1);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("已取消".into());
        }
        let n = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    if buf.len() != want {
        return Err(format!("分段长度不符 {} != {want}", buf.len()));
    }
    Ok(buf)
}

fn download_file_parallel(
    app: &AppHandle,
    agent: &ureq::Agent,
    source: &str,
    url: &str,
    dest: &Path,
    file: &ModelFile,
    base_done: u64,
    grand_total: u64,
) -> Result<(), String> {
    const CHUNK: u64 = 4 * 1024 * 1024;
    const CONNS: usize = 8;
    let part = dest.with_extension("part");
    {
        let f = std::fs::File::create(&part).map_err(|e| format!("创建临时文件失败: {e}"))?;
        f.set_len(file.size).map_err(|e| e.to_string())?;
    }
    let ranges: Arc<Mutex<Vec<(u64, u64)>>> = Arc::new(Mutex::new(
        (0..file.size)
            .step_by(CHUNK as usize)
            .map(|s| (s, (s + CHUNK).min(file.size) - 1))
            .collect(),
    ));
    let done = Arc::new(AtomicU64::new(0));
    let failed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let t0 = Instant::now();

    std::thread::scope(|scope| {
        for _ in 0..CONNS {
            let ranges = Arc::clone(&ranges);
            let done = Arc::clone(&done);
            let failed = Arc::clone(&failed);
            let part = part.clone();
            scope.spawn(move || loop {
                if failed.lock().is_some() || DOWNLOAD_CANCEL.load(Ordering::Acquire) {
                    return;
                }
                let Some((a, b)) = ranges.lock().pop() else {
                    return;
                };
                let mut last_err = String::new();
                let mut ok = false;
                for attempt in 0u64..5 {
                    match fetch_range(agent, url, a, b, &DOWNLOAD_CANCEL) {
                        Ok(data) => {
                            let write = (|| -> std::io::Result<()> {
                                use std::io::{Seek, SeekFrom};
                                let mut f = std::fs::OpenOptions::new().write(true).open(&part)?;
                                f.seek(SeekFrom::Start(a))?;
                                f.write_all(&data)
                            })();
                            match write {
                                Ok(()) => {
                                    done.fetch_add(data.len() as u64, Ordering::AcqRel);
                                    ok = true;
                                }
                                Err(e) => last_err = format!("写文件失败: {e}"),
                            }
                            break;
                        }
                        Err(e) => {
                            last_err = e;
                            if DOWNLOAD_CANCEL.load(Ordering::Acquire) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(800 * (attempt + 1)));
                        }
                    }
                }
                if !ok {
                    *failed.lock() = Some(last_err);
                    return;
                }
            });
        }
        // 进度汇报
        loop {
            let d = done.load(Ordering::Acquire);
            let secs = t0.elapsed().as_secs_f64().max(0.001);
            let total_done = base_done + d;
            let _ = app.emit(
                "asr-model-download-progress",
                DownloadProgress {
                    downloaded: total_done,
                    total: grand_total,
                    percent: total_done as f64 * 100.0 / grand_total.max(1) as f64,
                    speed_bps: (d as f64 / secs) as u64,
                    source: source.to_string(),
                },
            );
            if d >= file.size || failed.lock().is_some() || DOWNLOAD_CANCEL.load(Ordering::Acquire)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });

    if DOWNLOAD_CANCEL.load(Ordering::Acquire) {
        let _ = std::fs::remove_file(&part);
        return Err("已取消下载".into());
    }
    if let Some(e) = failed.lock().take() {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    // 校验
    let mut hasher = Sha256::new();
    let mut f = std::fs::File::open(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = hex_lower(&hasher.finalize());
    if got != file.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} 校验失败（sha256 不匹配）", file.name));
    }
    std::fs::rename(&part, dest).map_err(|e| format!("保存 {} 失败: {e}", file.name))?;
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn download_model_blocking(app: &AppHandle, spec: &ModelSpec) -> Result<(), String> {
    let dir = model_dir(spec).ok_or("无法确定模型目录")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败: {e}"))?;
    let grand_total: u64 = spec.files.iter().map(|f| f.size).sum();
    let mut base_done = 0u64;
    for file in spec.files {
        let dest = dir.join(file.name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建模型目录失败: {e}"))?;
        }
        let already = std::fs::metadata(&dest)
            .map(|m| m.len() == file.size)
            .unwrap_or(false);
        if already {
            base_done += file.size;
            continue;
        }
        let mut last_err = String::from("没有可用的下载源");
        let mut ok = false;
        for (name, prefix, use_proxy) in sources(spec.repo) {
            let agent = if use_proxy { agent() } else { agent_direct() };
            let url = format!("{prefix}/{}", file.name);
            log::info!("LINUX ASR download {} from {name}", file.name);
            match download_file_parallel(app, &agent, name, &url, &dest, file, base_done, grand_total) {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) => {
                    log::warn!("LINUX ASR download {} via {name} failed: {e}", file.name);
                    last_err = format!("{name}: {e}");
                    if DOWNLOAD_CANCEL.load(Ordering::Acquire) {
                        return Err("已取消下载".into());
                    }
                }
            }
        }
        if !ok {
            return Err(last_err);
        }
        base_done += file.size;
    }
    Ok(())
}

/// 后台下载模型（不传 id 则下载当前设置里的模型）；进度事件 `asr-model-download-progress`，
/// 结束事件 `asr-model-download-complete` / `asr-model-download-error`
pub fn start_download(app: AppHandle, model_id: Option<String>) -> Result<(), String> {
    let id = model_id.unwrap_or_else(|| super::settings::get().asr_model);
    let spec = model_spec(&id).ok_or("未知的语音识别模型")?;
    {
        let mut d = DOWNLOADING.lock();
        if let Some(cur) = d.as_ref() {
            return Err(format!("正在下载 {cur}，请等它完成"));
        }
        *d = Some(spec.id.to_string());
    }
    DOWNLOAD_CANCEL.store(false, Ordering::Release);
    std::thread::Builder::new()
        .name("linux-asr-download".into())
        .spawn(move || {
            let result = download_model_blocking(&app, spec);
            *DOWNLOADING.lock() = None;
            match result {
                Ok(()) => {
                    log::info!("LINUX ASR model {} downloaded", spec.id);
                    *LAST_ERROR.lock() = None;
                    let _ = app.emit("asr-model-download-complete", spec.id);
                    if super::settings::get().asr_model == spec.id {
                        ensure_loaded_async();
                    }
                }
                Err(e) => {
                    log::warn!("LINUX ASR model download failed: {e}");
                    let _ = app.emit("asr-model-download-error", e);
                }
            }
        })
        .map_err(|e| {
            *DOWNLOADING.lock() = None;
            format!("启动下载线程失败: {e}")
        })?;
    Ok(())
}

pub fn cancel_download() {
    DOWNLOAD_CANCEL.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_boosts_quiet_audio_but_caps_gain() {
        let quiet = vec![100i16, -100, 50];
        let out = normalize_for_asr(&quiet);
        // 100 * 31.6 / 32768 ≈ 0.096（封顶 30 dB）
        assert!((out[0] - 0.0964).abs() < 0.01, "{}", out[0]);
        let loud = vec![30000i16, -30000];
        let out = normalize_for_asr(&loud);
        assert!(out[0] <= 0.92 && out[0] > 0.9);
    }

    #[test]
    fn strip_trailing_period_handles_cjk_and_ascii() {
        assert_eq!(postprocess_text("你好。", true, ""), "你好");
        assert_eq!(postprocess_text("Hello world.", true, ""), "Hello world");
        assert_eq!(postprocess_text("你好。", false, ""), "你好。");
        assert_eq!(postprocess_text("  好  ", false, ""), "好");
    }

    #[test]
    fn spelled_letters_are_joined_but_words_untouched() {
        assert_eq!(collapse_spelled_letters("这个P R先别 merge，等C I跑完"), "这个PR先别 merge，等CI跑完");
        assert_eq!(collapse_spelled_letters("用S Q L查一下"), "用SQL查一下");
        assert_eq!(collapse_spelled_letters("A I 编程"), "AI 编程");
        assert_eq!(collapse_spelled_letters("I think it is OK"), "I think it is OK");
        assert_eq!(collapse_spelled_letters("Please refactor the API"), "Please refactor the API");
    }

    #[test]
    fn replacements_are_case_insensitive_and_line_based() {
        let rules = "Cloud Code => Claude Code\n\nolama=>Ollama\n无效行\n";
        assert_eq!(
            apply_replacements("打开 cloud code，再看看 Olama 的显存", rules),
            "打开 Claude Code，再看看 Ollama 的显存"
        );
        assert_eq!(apply_replacements("没有要替换的", rules), "没有要替换的");
    }

    #[test]
    fn qwen3_is_default_and_listed_first() {
        assert_eq!(MODELS[0].id, crate::linux::settings::DEFAULT_ASR_MODEL);
        assert_eq!(MODELS[0].kind, ModelKind::Qwen3Asr);
        let o = models_overview();
        assert!(o[0].supports_hotwords && !o[1].supports_hotwords);
    }

    #[test]
    fn default_model_is_in_catalog() {
        assert!(model_spec(crate::linux::settings::DEFAULT_ASR_MODEL).is_some());
    }
}
