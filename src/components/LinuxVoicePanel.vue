<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t, tb, isEn } from "../i18n";

type VoiceMode = "asr" | "virtual_mic";
type PasteMethod = "shift_insert" | "ctrl_v" | "ctrl_shift_v";

interface LinuxVoiceSettings {
  voice_mode: VoiceMode;
  asr_model: string;
  asr_language: string;
  paste_method: PasteMethod;
  restore_clipboard: boolean;
  strip_trailing_period: boolean;
  asr_threads: number;
  min_utterance_ms: number;
  asr_hotwords: string;
  asr_replacements: string;
  asr_idle_unload_minutes: number;
}

interface AsrStatus {
  modelId: string;
  modelTitle: string;
  installed: boolean;
  loaded: boolean;
  loading: boolean;
  downloading: boolean;
  downloadingModel: string | null;
  error: string | null;
  modelsDir: string | null;
}

interface AsrModel {
  id: string;
  title: string;
  note: string;
  installed: boolean;
  sizeMb: number;
  ramMb: number;
  supportsHotwords: boolean;
  supportsLanguage: boolean;
}

interface AsrResult {
  ok: boolean;
  text: string;
  audioMs: number;
  asrMs: number;
  error: string | null;
}

interface BtDevice {
  address: string;
  name: string;
  paired: boolean;
  connected: boolean;
  trusted: boolean;
  hasAtvv: boolean;
  modalias: string | null;
  battery: number | null;
}

interface BtDiagnostics {
  adapter: string | null;
  powered: boolean;
  devices: BtDevice[];
  error: string | null;
}

const settings = ref<LinuxVoiceSettings | null>(null);
const asr = ref<AsrStatus | null>(null);
const models = ref<AsrModel[]>([]);
const saveError = ref("");
const download = ref<{ percent: number; speed: number; source: string } | null>(null);
const downloadError = ref("");
const lastResult = ref<AsrResult | null>(null);
const diag = ref<BtDiagnostics | null>(null);
const diagLoading = ref(false);
const actionError = ref("");

const unlisteners: UnlistenFn[] = [];
let statusTimer: ReturnType<typeof setInterval> | null = null;

const languages = computed(() => [
  { id: "auto", label: t("自动（中英混说）", "Auto (mixed Chinese/English)") },
  { id: "zh", label: t("普通话", "Mandarin") },
  { id: "en", label: t("英语", "English") },
  { id: "yue", label: t("粤语", "Cantonese") },
  { id: "ja", label: t("日语", "Japanese") },
  { id: "ko", label: t("韩语", "Korean") },
]);

/** 空闲多久释放：label 是下拉里的完整文字 */
const idleOptions = computed(() => [
  { minutes: 5, label: t("5 分钟没说话就释放", "After 5 min idle") },
  { minutes: 10, label: t("10 分钟没说话就释放", "After 10 min idle") },
  { minutes: 30, label: t("30 分钟没说话就释放", "After 30 min idle") },
  { minutes: 60, label: t("1 小时没说话就释放", "After 1 hour idle") },
  { minutes: 0, label: t("不释放（常驻内存）", "Never (keep loaded)") },
]);

const pasteMethods = computed<{ id: PasteMethod; label: string }[]>(() => [
  { id: "shift_insert", label: t("Shift+Insert（推荐，终端也能用）", "Shift+Insert (recommended, works in terminals)") },
  { id: "ctrl_v", label: "Ctrl+V" },
  { id: "ctrl_shift_v", label: t("Ctrl+Shift+V（部分终端）", "Ctrl+Shift+V (some terminals)") },
]);

const currentModel = computed(() =>
  models.value.find((m) => m.id === settings.value?.asr_model)
);

const downloadingTitle = computed(() => {
  const id = asr.value?.downloadingModel;
  if (!id) return "";
  return tb(models.value.find((m) => m.id === id)?.title ?? id);
});

const asrStateText = computed(() => {
  const s = asr.value;
  if (!s) return t("检测中…", "Checking…");
  if (s.downloading) {
    const p = download.value?.percent ?? 0;
    return t(`下载中 ${p.toFixed(0)}%`, `Downloading ${p.toFixed(0)}%`);
  }
  if (s.loaded) return t("已就绪", "Ready");
  if (s.loading) return t("加载中…", "Loading…");
  if (s.installed) return t("已下载（按语音键时自动加载）", "Downloaded (loads when you press the voice key)");
  return t("未下载", "Not downloaded");
});

const asrStateTone = computed(() => {
  const s = asr.value;
  if (!s) return "";
  if (s.loaded) return "ok";
  if (s.downloading || s.loading || s.installed) return "warn";
  return "error";
});

async function refreshAsr() {
  try {
    asr.value = await invoke<AsrStatus>("get_asr_status");
  } catch (e) {
    asr.value = null;
    actionError.value = String(e);
  }
}

async function refreshModels() {
  try {
    models.value = await invoke<AsrModel[]>("get_asr_models");
  } catch (e) {
    actionError.value = String(e);
  }
}

async function loadSettings() {
  try {
    settings.value = await invoke<LinuxVoiceSettings>("get_linux_voice_settings");
  } catch (e) {
    saveError.value = String(e);
  }
}

async function save(patch: Partial<LinuxVoiceSettings>) {
  if (!settings.value) return;
  const next = { ...settings.value, ...patch };
  saveError.value = "";
  try {
    settings.value = await invoke<LinuxVoiceSettings>("save_linux_voice_settings", {
      settings: next,
    });
    await refreshAsr();
  } catch (e) {
    saveError.value = t(`保存失败：${e}`, `Save failed: ${tb(String(e))}`);
  }
}

function onTextSetting(key: "asr_hotwords" | "asr_replacements", ev: Event) {
  const value = (ev.target as HTMLInputElement | HTMLTextAreaElement).value;
  if (settings.value && settings.value[key] !== value) save({ [key]: value });
}

async function startDownload(modelId: string) {
  downloadError.value = "";
  download.value = { percent: 0, speed: 0, source: "" };
  try {
    await invoke("download_asr_model", { modelId });
    await refreshAsr();
  } catch (e) {
    downloadError.value = String(e);
    download.value = null;
  }
}

async function cancelDownload() {
  await invoke("cancel_asr_model_download").catch(() => {});
}

async function openModelsFolder() {
  actionError.value = "";
  await invoke("open_models_folder").catch((e) => (actionError.value = String(e)));
}

async function openBluetoothSettings() {
  actionError.value = "";
  await invoke("open_bluetooth_settings").catch((e) => (actionError.value = String(e)));
}

async function runDiagnostics() {
  diagLoading.value = true;
  try {
    diag.value = await invoke<BtDiagnostics>("get_bluetooth_diagnostics");
  } catch (e) {
    diag.value = { adapter: null, powered: false, devices: [], error: String(e) };
  } finally {
    diagLoading.value = false;
  }
}

function formatSpeed(bps: number): string {
  if (bps > 1024 * 1024) return `${(bps / 1024 / 1024).toFixed(1)} MB/s`;
  return `${Math.round(bps / 1024)} KB/s`;
}

onMounted(async () => {
  await loadSettings();
  await Promise.all([refreshAsr(), refreshModels()]);
  statusTimer = setInterval(refreshAsr, 2000);
  unlisteners.push(
    await listen<{ percent: number; speedBps: number; source: string }>(
      "asr-model-download-progress",
      (e) => {
        download.value = {
          percent: e.payload.percent,
          speed: e.payload.speedBps,
          source: e.payload.source,
        };
      }
    ),
    await listen("asr-model-download-complete", () => {
      download.value = null;
      refreshAsr();
      refreshModels();
    }),
    await listen<string>("asr-model-download-error", (e) => {
      download.value = null;
      downloadError.value = e.payload;
      refreshAsr();
    }),
    await listen<AsrResult>("linux-asr-result", (e) => {
      lastResult.value = e.payload;
    })
  );
});

onUnmounted(() => {
  if (statusTimer) clearInterval(statusTimer);
  unlisteners.forEach((u) => u());
});
</script>

<template>
  <section class="card linux-voice">
    <div class="lv-head">
      <h3>{{ t("语音输出", "Voice output") }}</h3>
      <span class="lv-sub">{{
        t(
          "Linux 上没有微信/豆包输入法的语音听写，语音键说的话由这里处理",
          "Linux has no WeChat/Doubao IME voice dictation, so speech from the voice key is handled here"
        )
      }}</span>
    </div>

    <div v-if="settings" class="lv-modes" role="radiogroup" :aria-label="t('语音输出方式', 'Voice output mode')">
      <label class="lv-mode" :class="{ 'is-active': settings.voice_mode === 'asr' }">
        <input
          type="radio"
          name="voice-mode"
          value="asr"
          :checked="settings.voice_mode === 'asr'"
          @change="save({ voice_mode: 'asr' })"
        />
        <span class="lv-mode-title">{{ t("本地识别上屏（推荐）", "Local recognition, paste at cursor (recommended)") }}</span>
        <span class="lv-mode-desc">
          {{
            t(
              "按住语音键说话，松手后在本机离线识别，文字直接粘贴到当前输入框；中英混说也能认。",
              "Hold the voice key and speak; on release it is recognized offline on this machine and pasted into the focused field. Mixed Chinese/English works too."
            )
          }}
        </span>
      </label>
      <label class="lv-mode" :class="{ 'is-active': settings.voice_mode === 'virtual_mic' }">
        <input
          type="radio"
          name="voice-mode"
          value="virtual_mic"
          :checked="settings.voice_mode === 'virtual_mic'"
          @change="save({ voice_mode: 'virtual_mic' })"
        />
        <span class="lv-mode-title">{{ t("虚拟麦克风 + 快捷键", "Virtual mic + shortcut") }}</span>
        <span class="lv-mode-desc">
          {{
            t(
              "与 Windows 版相同：语音送进「Voice VibeCoding 遥控器麦克风」，同时按住下方映射的语音快捷键。",
              "Same as the Windows version: audio goes to the \"Voice VibeCoding 遥控器麦克风\" mic (the remote mic) while the voice shortcut mapped below is held."
            )
          }}
        </span>
      </label>
    </div>

    <div v-if="settings && settings.voice_mode === 'asr'" class="lv-block">
      <div class="lv-row">
        <label class="lv-field lv-model">
          <span class="lv-label">{{ t("识别模型", "Model") }}</span>
          <select
            :value="settings.asr_model"
            @change="save({ asr_model: ($event.target as HTMLSelectElement).value })"
          >
            <option v-for="m in models" :key="m.id" :value="m.id">
              {{ tb(m.title) }}{{ m.installed ? "" : t("（未下载）", " (not downloaded)") }}
            </option>
          </select>
        </label>
        <span class="lv-state" :class="`tone-${asrStateTone}`">{{ asrStateText }}</span>
        <button
          v-if="asr && !asr.installed && !asr.downloading"
          class="btn btn-primary btn-small"
          type="button"
          :disabled="!!asr.downloadingModel"
          @click="startDownload(settings.asr_model)"
        >
          {{ t("下载模型", "Download model")
          }}{{ currentModel ? t(`（${currentModel.sizeMb} MB）`, ` (${currentModel.sizeMb} MB)`) : "" }}
        </button>
        <button
          v-if="asr?.downloading"
          class="btn btn-secondary btn-small"
          type="button"
          @click="cancelDownload"
        >
          {{ t("取消", "Cancel") }}
        </button>
        <button class="btn btn-secondary btn-small" type="button" @click="openModelsFolder">
          {{ t("打开模型目录", "Open models folder") }}
        </button>
      </div>
      <p v-if="currentModel" class="lv-note">
        {{ tb(currentModel.note) }}
        <template v-if="!currentModel.installed">
          · {{ t(`需下载约 ${currentModel.sizeMb} MB`, `~${currentModel.sizeMb} MB download`) }}</template
        >
      </p>
      <div v-if="asr?.downloadingModel && download" class="lv-progress">
        <div class="lv-progress-bar" :style="{ width: `${download.percent.toFixed(1)}%` }" />
        <span class="lv-progress-text">
          <template v-if="!asr.downloading">{{ downloadingTitle }} · </template>
          {{ download.percent.toFixed(1) }}% · {{ formatSpeed(download.speed) }} · {{ tb(download.source) }}
        </span>
      </div>
      <p v-if="downloadError" class="lv-error">{{ t("下载失败：", "Download failed: ") }}{{ tb(downloadError) }}</p>
      <p v-if="asr?.error && !asr.downloading" class="lv-error">{{ tb(asr.error) }}</p>

      <div class="lv-grid">
        <label v-if="currentModel?.supportsLanguage" class="lv-field">
          <span class="lv-label">{{ t("识别语言", "Language") }}</span>
          <select
            :value="settings.asr_language"
            @change="save({ asr_language: ($event.target as HTMLSelectElement).value })"
          >
            <option v-for="l in languages" :key="l.id" :value="l.id">{{ l.label }}</option>
          </select>
        </label>
        <label class="lv-field">
          <span class="lv-label">{{ t("上屏方式", "Paste method") }}</span>
          <select
            :value="settings.paste_method"
            @change="
              save({ paste_method: ($event.target as HTMLSelectElement).value as PasteMethod })
            "
          >
            <option v-for="p in pasteMethods" :key="p.id" :value="p.id">{{ p.label }}</option>
          </select>
        </label>
        <label class="lv-check">
          <input
            type="checkbox"
            :checked="settings.restore_clipboard"
            @change="save({ restore_clipboard: ($event.target as HTMLInputElement).checked })"
          />
          {{ t("粘贴后恢复原剪贴板", "Restore clipboard after paste") }}
        </label>
        <label class="lv-check">
          <input
            type="checkbox"
            :checked="settings.strip_trailing_period"
            @change="save({ strip_trailing_period: ($event.target as HTMLInputElement).checked })"
          />
          {{ t("去掉句末句号", "Strip trailing period") }}
        </label>
        <label class="lv-field">
          <span class="lv-label">{{ t("空闲释放内存", "Free memory when idle") }}</span>
          <select
            :value="settings.asr_idle_unload_minutes"
            @change="
              save({
                asr_idle_unload_minutes: Number(($event.target as HTMLSelectElement).value),
              })
            "
          >
            <option v-for="o in idleOptions" :key="o.minutes" :value="o.minutes">
              {{ o.label }}
            </option>
          </select>
        </label>
        <label v-if="currentModel?.supportsHotwords" class="lv-field lv-span">
          <span class="lv-label">{{ t("热词", "Hotwords") }}</span>
          <input
            type="text"
            :value="settings.asr_hotwords"
            :placeholder="t('可选，逗号分隔，如：Claude, tokio, BlueZ', 'Optional, comma-separated, e.g. Claude, tokio, BlueZ')"
            spellcheck="false"
            @change="onTextSetting('asr_hotwords', $event)"
          />
        </label>
        <p v-if="currentModel?.supportsHotwords" class="lv-note lv-span">
          {{
            t(
              "热词能把常错的专有名词认对，但写得越多，container、server 这类普通英文词越容易被意译成中文；只写几个总认错的名字即可。",
              "Hotwords fix names that are often misheard, but the more you add, the more plain English words like container or server get translated into Chinese. List only a few names that keep going wrong."
            )
          }}
        </p>
        <label class="lv-field lv-span lv-field-top">
          <span class="lv-label">{{ t("识别后替换", "Replacements") }}</span>
          <textarea
            rows="3"
            :value="settings.asr_replacements"
            :placeholder="t('每行一条：原文 => 替换为', 'One per line: original => replacement')"
            spellcheck="false"
            @change="onTextSetting('asr_replacements', $event)"
          />
        </label>
        <p class="lv-note lv-span">
          {{
            t(
              "每行一条「原文 => 替换为」，英文不区分大小写；用来固定纠正总是认错的词。",
              "One rule per line: \"original => replacement\". English is case-insensitive. Use it to fix words that are always misrecognized."
            )
          }}
        </p>
      </div>

      <div class="lv-result" :class="{ 'is-empty': !lastResult }">
        <span class="lv-label">{{ t("最近一次识别", "Last result") }}</span>
        <template v-if="lastResult">
          <span v-if="lastResult.text" class="lv-result-text">{{ lastResult.text }}</span>
          <span v-else class="lv-result-empty">{{ lastResult.error ? tb(lastResult.error) : t("（无内容）", "(empty)") }}</span>
          <span class="lv-result-meta">
            {{ t("语音", "Audio") }} {{ (lastResult.audioMs / 1000).toFixed(1) }}s ·
            {{ t("识别", "ASR") }} {{ lastResult.asrMs }}ms
            <template v-if="lastResult.text && !lastResult.ok">
              · {{ t("上屏失败：", "Paste failed: ") }}{{ tb(lastResult.error) }}</template
            >
          </span>
        </template>
        <span v-else class="lv-result-empty">{{ t("按住遥控器语音键说一句话试试", "Hold the remote's voice key and say something") }}</span>
      </div>
    </div>

    <div v-if="settings && settings.voice_mode === 'virtual_mic'" class="lv-block">
      <p class="lv-hint">
        <template v-if="isEn">
          In the app where you want voice input, choose <b>Voice VibeCoding 遥控器麦克风</b> (the remote mic) as the
          microphone (or make it the default under Settings → Sound → Input). While the remote's voice key is held, this
          app also holds the shortcut mapped to the "Voice" key below.
        </template>
        <template v-else>
          在需要语音输入的软件里，把麦克风选成 <b>Voice VibeCoding 遥控器麦克风</b>
          （也可在「设置 → 声音 → 输入」里设为默认）。按住遥控器语音键时，本软件会同时按住下方「语音」键映射的快捷键。
        </template>
      </p>
    </div>

    <div class="lv-block lv-bt">
      <div class="lv-row">
        <span class="lv-label">{{ t("蓝牙配对", "Bluetooth pairing") }}</span>
        <span class="lv-value">
          {{
            t(
              "在系统蓝牙设置里添加「MI RC」：同时长按遥控器「主页」+「菜单」键约 3 秒进入配对模式。",
              "Add \"MI RC\" in the system Bluetooth settings: hold the remote's Home + Menu keys for about 3 s to enter pairing mode."
            )
          }}
        </span>
      </div>
      <div class="lv-row">
        <button class="btn btn-secondary btn-small" type="button" @click="openBluetoothSettings">
          {{ t("打开蓝牙设置", "Open Bluetooth settings") }}
        </button>
        <button
          class="btn btn-secondary btn-small"
          type="button"
          :disabled="diagLoading"
          @click="runDiagnostics"
        >
          {{ diagLoading ? t("检测中…", "Checking…") : t("蓝牙诊断", "Bluetooth diagnostics") }}
        </button>
      </div>
      <div v-if="diag" class="lv-diag">
        <p v-if="diag.error" class="lv-error">{{ tb(diag.error) }}</p>
        <p v-else>
          {{ t("适配器", "Adapter") }} {{ diag.adapter }} ·
          {{ diag.powered ? t("已开启", "On") : t("已关闭", "Off") }} ·
          {{ t(`已知设备 ${diag.devices.length} 个`, `${diag.devices.length} known device(s)`) }}
        </p>
        <ul v-if="diag.devices.length">
          <li v-for="d in diag.devices" :key="d.address" :class="{ 'is-remote': d.hasAtvv }">
            <b>{{ d.name || t("(无名称)", "(no name)") }}</b> {{ d.address }} ·
            {{ d.paired ? t("已配对", "Paired") : t("未配对", "Not paired") }} ·
            {{ d.connected ? t("已连接", "Connected") : t("未连接", "Disconnected") }}
            <template v-if="d.hasAtvv"> · {{ t("有 ATVV 语音服务", "ATVV voice service") }}</template>
            <template v-if="d.battery != null"> · {{ t("电量", "Battery") }} {{ d.battery }}%</template>
          </li>
        </ul>
      </div>
    </div>

    <p v-if="saveError" class="lv-error">{{ tb(saveError) }}</p>
    <p v-if="actionError" class="lv-error">{{ tb(actionError) }}</p>
  </section>
</template>

<style scoped>
.linux-voice {
  background: var(--card-bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  margin-bottom: 10px;
  font-size: 13px;
}
.lv-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin-bottom: 8px;
}
.lv-head h3 {
  font-size: 15px;
  font-weight: 600;
}
.lv-sub {
  color: var(--text-secondary);
  font-size: 12px;
}
.lv-modes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-bottom: 8px;
}
.lv-mode {
  display: grid;
  grid-template-columns: auto 1fr;
  column-gap: 8px;
  row-gap: 2px;
  align-items: start;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  cursor: pointer;
}
.lv-mode.is-active {
  border-color: var(--primary);
  background: #eff6ff;
}
.lv-mode input {
  grid-row: span 2;
  margin-top: 3px;
}
.lv-mode-title {
  font-weight: 600;
}
.lv-mode-desc {
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.5;
}
.lv-block {
  border-top: 1px dashed var(--border);
  padding-top: 8px;
  margin-top: 8px;
}
.lv-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 6px;
}
.lv-label {
  color: var(--text-secondary);
  font-size: 12px;
  white-space: nowrap;
}
.lv-value {
  flex: 0 1 auto;
}
.lv-state {
  font-size: 12px;
  padding: 1px 8px;
  border-radius: 10px;
  background: #f1f5f9;
}
.tone-ok {
  color: #15803d;
  background: #dcfce7;
}
.tone-warn {
  color: #b45309;
  background: #fef3c7;
}
.tone-error {
  color: #b91c1c;
  background: #fee2e2;
}
.lv-progress {
  position: relative;
  height: 18px;
  background: #f1f5f9;
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 6px;
}
.lv-progress-bar {
  position: absolute;
  inset: 0 auto 0 0;
  background: #bfdbfe;
  transition: width 0.3s;
}
.lv-progress-text {
  position: relative;
  font-size: 11px;
  line-height: 18px;
  padding-left: 8px;
}
.lv-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 16px;
  margin: 6px 0;
}
.lv-field {
  display: flex;
  align-items: center;
  gap: 8px;
}
.lv-field select,
.lv-field input[type="text"],
.lv-field textarea {
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: #fff;
  font-size: 12px;
  font-family: inherit;
}
.lv-field textarea {
  resize: vertical;
  line-height: 1.5;
}
.lv-field-top {
  align-items: flex-start;
}
.lv-field-top .lv-label {
  padding-top: 4px;
}
.lv-model {
  flex: 1 1 280px;
  min-width: 0;
}
.lv-span {
  grid-column: 1 / -1;
}
.lv-note {
  color: var(--text-secondary);
  font-size: 11px;
  line-height: 1.5;
  margin: -2px 0 4px;
}
.lv-check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  cursor: pointer;
}
.lv-result {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  padding: 6px 8px;
  background: #f8fafc;
  border-radius: 6px;
}
.lv-result-text {
  font-weight: 500;
}
.lv-result-empty {
  color: var(--text-secondary);
}
.lv-result-meta {
  color: var(--text-secondary);
  font-size: 11px;
}
.lv-hint {
  line-height: 1.6;
}
.lv-diag {
  font-size: 12px;
  line-height: 1.6;
}
.lv-diag ul {
  padding-left: 18px;
}
.lv-diag li.is-remote {
  color: #15803d;
}
.lv-error {
  color: var(--danger);
  font-size: 12px;
  margin-top: 4px;
}
.btn {
  border: none;
  border-radius: 5px;
  cursor: pointer;
}
.btn-small {
  padding: 3px 10px;
  font-size: 12px;
}
.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.btn-secondary {
  background: #f1f5f9;
  color: var(--text);
  border: 1px solid var(--border);
}
.btn-secondary:hover:not(:disabled) {
  background: #e2e8f0;
}
.btn-primary {
  background: var(--primary, #2563eb);
  color: #fff;
  border: 1px solid transparent;
}
.btn-primary:hover:not(:disabled) {
  filter: brightness(0.95);
}
</style>
