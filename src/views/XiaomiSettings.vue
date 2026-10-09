<script setup lang="ts">
import { onMounted, onUnmounted, computed, ref, nextTick, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useBridgeStore } from "../stores/bridge";
import { useConfigStore } from "../stores/config";
import type { DeviceConfig } from "../types";
import DeviceStatus from "../components/DeviceStatus.vue";
import BatteryLevelIcon from "../components/BatteryLevelIcon.vue";
import CableVolRuler from "../components/CableVolRuler.vue";
import KeyMappingStage from "../components/KeyMappingStage.vue";
import LinuxVoicePanel from "../components/LinuxVoicePanel.vue";
import { isLinux } from "../stores/platform";
import { cableZoneForLevel } from "../utils/cableVolMeter";
import wechatImeHotkeysImg from "../assets/guides/wechat-ime-hotkeysV3.png";
import doubaoImeHotkeysImg from "../assets/guides/doubao.png";
import { vkDisplayName } from "../utils/vkDisplay";
import { t, tb } from "../i18n";
import {
  applyImePresetConfig,
  getPresetsForTab,
  IME_FAQ,
  IME_PRESETS,
  QIANWEN_GUIDE,
  QIANWEN_PRESET_IDS,
  listImeTabs,
  type ImePresetDefinition,
  type ImePresetId,
  type ImeTabId,
} from "../utils/imePreset";

const bridge = useBridgeStore();
const configStore = useConfigStore();
const type = "xiaomi" as const;

const device = computed(() => bridge.devices[type]);
const config = computed(() => configStore.configs[type]);
const configLoadState = computed(() => configStore.loadStates[type]);
const configLoadError = computed(() => configStore.loadErrors[type]);
const configSectionLoading = computed(
  () => configLoadState.value === "pending" || configLoadState.value === "loading"
);

interface HostStatusItem {
  id: string;
  label: string;
  state_label: string;
  tone: string;
}

interface HostStatus {
  bridge_alive: boolean;
  audio_alive: boolean;
  cable_ready: boolean;
  winuhid_ready?: boolean;
  atvv_ok?: boolean;
  status_text: string;
  detail: string;
  tone: string;
  items: HostStatusItem[];
}

const restarting = ref(false);
const voiceRepairing = ref(false);
const winuhidRepairing = ref(false);
const atvvRepairing = ref(false);
const showVoiceChoice = ref(false);
const showWinuhidChoice = ref(false);
const voiceChoiceMsg = ref("");
type VoiceRepairTab = "repair" | "guide" | "faq";
const voiceRepairTab = ref<VoiceRepairTab>("repair");
const voiceRepairTabs = computed<{ id: VoiceRepairTab; label: string }[]>(() => [
  { id: "repair", label: t("1 修复", "1 Repair") },
  { id: "guide", label: t("2 说明", "2 Guide") },
  { id: "faq", label: t("3 常见问题", "3 FAQ") },
]);
const winuhidChoiceMsg = ref("");
type WinuhidDownloadPhase = "idle" | "downloading" | "complete" | "error";
type CableDownloadPhase = "idle" | "downloading" | "complete" | "error";
const winuhidDownloadPhase = ref<WinuhidDownloadPhase>("idle");
const winuhidDownloadProgress = ref<{
  downloaded: number;
  total?: number | null;
  percent?: number | null;
} | null>(null);
const winuhidDownloadMessage = ref("");
const winuhidZipDefaultName = ref("WinUHid_Manual.zip");
const cableDownloadPhase = ref<CableDownloadPhase>("idle");
const cableDownloadProgress = ref<{
  downloaded: number;
  total?: number | null;
  percent?: number | null;
} | null>(null);
const cableDownloadMessage = ref("");
const cableZipDefaultName = ref("VBCABLE_Driver_Pack45.zip");
const showVoiceReboot = ref(false);
const voiceRebootMsg = ref("");
const showLogModal = ref(false);
const showSetupTips = ref(false);
const setupApplyHint = ref("");
const setupImeTab = ref<ImeTabId>("wechat");
const imeTabs = listImeTabs();
const imeFaq = IME_FAQ;
const qianwenGuide = QIANWEN_GUIDE;
const qianwenPresets = QIANWEN_PRESET_IDS.map((id) => IME_PRESETS[id]);

/** 设为 true 可恢复「触发模式」下拉（后端当前固定为按住语义，PR #8） */
const SHOW_VOICE_TRIGGER_MODE = false;

const activeImePresets = computed(() => getPresetsForTab(setupImeTab.value));
const logText = ref("");
const logPath = ref("");
const logLoading = ref(false);
const logCopyHint = ref("");

type BleMeterState = "idle" | "session" | "receiving";
interface VoiceMeterSnapshot {
  bleState: BleMeterState;
  bleLevel: number;
  waveform: number[];
  cableActive: boolean;
  cableLevel: number;
  atvvOk: boolean;
}

const voiceMeter = ref<VoiceMeterSnapshot>({
  bleState: "idle",
  bleLevel: 0,
  waveform: Array(28).fill(0),
  cableActive: false,
  cableLevel: 0,
  atvvOk: false,
});

/** 「按键映射」标题旁：最近一次 按下/抬起 + 配置映射 */
const lastMappingFlash = ref<{
  seq: number;
  phase: "down" | "up";
  remote: string;
  mapped: string | null;
} | null>(null);
let mappingFlashSeq = 0;
let mappingFlashClearTimer: ReturnType<typeof setTimeout> | null = null;

const bleSignalLabel = computed(() => {
  switch (voiceMeter.value.bleState) {
    case "receiving":
      return t("接收中", "Receiving");
    case "session":
      return t("语音会话", "Voice session");
    default:
      return t("无信号", "No signal");
  }
});

function waveBinHeight(v: number, receiving: boolean): number {
  const clamped = Math.min(1, Math.max(0, v));
  let h = Math.pow(clamped, 0.25) * 100;
  if (receiving && h < 20) h = 20;
  return Math.max(8, h);
}

const waveAreaPath = computed(() => {
  const wf = voiceMeter.value.waveform;
  const receiving = voiceMeter.value.bleState === "receiving";
  if (!wf.length) return "M0,100 L100,100 Z";
  const top = wf
    .map((v, i) => {
      const x = wf.length === 1 ? 0 : (i / (wf.length - 1)) * 100;
      const y = 100 - waveBinHeight(v, receiving);
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  return `${top} L100,100 L0,100 Z`;
});

const waveLinePoints = computed(() => {
  const wf = voiceMeter.value.waveform;
  const receiving = voiceMeter.value.bleState === "receiving";
  if (!wf.length) return "0,100 100,100";
  return wf
    .map((v, i) => {
      const x = wf.length === 1 ? 0 : (i / (wf.length - 1)) * 100;
      const y = 100 - waveBinHeight(v, receiving);
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
});

const cableReady = computed(() => host.value.cable_ready);
/** Linux 本地识别模式：状态栏第一列是「语音识别」，虚拟麦克风不参与 */
const linuxAsrMode = computed(
  () => isLinux.value && host.value.items.some((i) => i.id === "cable" && i.label === "语音识别")
);

const cableVolZone = computed(() => {
  if (!cableReady.value) return "idle";
  if (!voiceMeter.value.cableActive) return "idle";
  return cableZoneForLevel(voiceMeter.value.cableLevel);
});

const cableVolHint = computed(() => {
  if (!cableReady.value) return "\u00a0";
  if (!voiceMeter.value.cableActive) return "\u00a0";
  switch (cableVolZone.value) {
    case "low":
      return t("偏低", "Low");
    case "high":
      return t("偏高", "High");
    case "ok":
      return t("正常", "OK");
    default:
      return t("送声", "Sending");
  }
});

function applyVoiceMeter(p: Record<string, unknown>) {
  const bleState = (p.bleState ?? p.ble_state ?? "idle") as BleMeterState;
  const waveform = p.waveform as number[] | undefined;
  voiceMeter.value = {
    bleState,
    bleLevel: Number(p.bleLevel ?? p.ble_level ?? 0),
    waveform: Array.isArray(waveform) && waveform.length ? [...waveform] : Array(28).fill(0),
    cableActive: Boolean(p.cableActive ?? p.cable_active ?? false),
    cableLevel: Number(p.cableLevel ?? p.cable_level ?? 0),
    atvvOk: Boolean(p.atvvOk ?? p.atvv_ok ?? false),
  };
}
const showVoiceShortcutTip = ref(false);
const showGainTip = ref(false);
const showTriggerTip = ref(false);
const showRepairTip = ref(false);
const showWinuhidTip = ref(false);
const showAtvvTip = ref(false);
const showRestartTip = ref(false);
const voiceInfoBtn = ref<HTMLElement | null>(null);
const gainInfoBtn = ref<HTMLElement | null>(null);
const triggerInfoBtn = ref<HTMLElement | null>(null);
const repairInfoBtn = ref<HTMLElement | null>(null);
const winuhidInfoBtn = ref<HTMLElement | null>(null);
const atvvInfoBtn = ref<HTMLElement | null>(null);
const restartInfoBtn = ref<HTMLElement | null>(null);
const voiceTipEl = ref<HTMLElement | null>(null);
const gainTipEl = ref<HTMLElement | null>(null);
const triggerTipEl = ref<HTMLElement | null>(null);
const repairTipEl = ref<HTMLElement | null>(null);
const winuhidTipEl = ref<HTMLElement | null>(null);
const atvvTipEl = ref<HTMLElement | null>(null);
const restartTipEl = ref<HTMLElement | null>(null);
const voiceTipStyle = ref<Record<string, string>>({});
const gainTipStyle = ref<Record<string, string>>({});
const triggerTipStyle = ref<Record<string, string>>({});
const repairTipStyle = ref<Record<string, string>>({});
const winuhidTipStyle = ref<Record<string, string>>({});
const atvvTipStyle = ref<Record<string, string>>({});
const restartTipStyle = ref<Record<string, string>>({});
let voiceTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let gainTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let triggerTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let repairTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let winuhidTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let atvvTipCloseTimer: ReturnType<typeof setTimeout> | null = null;
let restartTipCloseTimer: ReturnType<typeof setTimeout> | null = null;

/** 右上 / 右下自动落位，并钳制在视口内 */
function placeInfoTip(
  anchor: HTMLElement | null,
  tip: HTMLElement | null,
  styleRef: typeof voiceTipStyle
) {
  if (!anchor || !tip) return;
  const margin = 8;
  const pad = 8;
  const ar = anchor.getBoundingClientRect();
  const tw = tip.offsetWidth || Math.min(420, window.innerWidth - pad * 2);
  const th = tip.offsetHeight || 120;
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  const spaceBelow = vh - ar.bottom - margin;
  const spaceAbove = ar.top - margin;
  // 优先右下方；下方不够且上方更宽裕则改右上方
  const placeBelow = spaceBelow >= th || spaceBelow >= spaceAbove;

  let top = placeBelow ? ar.bottom + margin : ar.top - th - margin;
  // 右对齐图标右侧（右上/右下）
  let left = ar.right - tw;

  if (left < pad) left = pad;
  if (left + tw > vw - pad) left = Math.max(pad, vw - pad - tw);
  if (top < pad) top = pad;
  if (top + th > vh - pad) top = Math.max(pad, vh - pad - th);

  styleRef.value = {
    position: "fixed",
    top: `${Math.round(top)}px`,
    left: `${Math.round(left)}px`,
    right: "auto",
    bottom: "auto",
    zIndex: "2000",
    visibility: "visible",
    maxWidth: `${Math.min(420, vw - pad * 2)}px`,
  };
}

async function openVoiceTip() {
  if (voiceTipCloseTimer) {
    clearTimeout(voiceTipCloseTimer);
    voiceTipCloseTimer = null;
  }
  voiceTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showVoiceShortcutTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(voiceInfoBtn.value, voiceTipEl.value, voiceTipStyle);
  });
}

function scheduleCloseVoiceTip() {
  if (voiceTipCloseTimer) clearTimeout(voiceTipCloseTimer);
  voiceTipCloseTimer = setTimeout(() => {
    showVoiceShortcutTip.value = false;
  }, 120);
}

function toggleVoiceTip() {
  if (showVoiceShortcutTip.value) {
    showVoiceShortcutTip.value = false;
  } else {
    void openVoiceTip();
  }
}

async function openGainTip() {
  if (gainTipCloseTimer) {
    clearTimeout(gainTipCloseTimer);
    gainTipCloseTimer = null;
  }
  gainTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showGainTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(gainInfoBtn.value, gainTipEl.value, gainTipStyle);
  });
}

function scheduleCloseGainTip() {
  if (gainTipCloseTimer) clearTimeout(gainTipCloseTimer);
  gainTipCloseTimer = setTimeout(() => {
    showGainTip.value = false;
  }, 120);
}

function toggleGainTip() {
  if (showGainTip.value) {
    showGainTip.value = false;
  } else {
    void openGainTip();
  }
}

async function openTriggerTip() {
  if (triggerTipCloseTimer) {
    clearTimeout(triggerTipCloseTimer);
    triggerTipCloseTimer = null;
  }
  triggerTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showTriggerTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(triggerInfoBtn.value, triggerTipEl.value, triggerTipStyle);
  });
}

function scheduleCloseTriggerTip() {
  if (triggerTipCloseTimer) clearTimeout(triggerTipCloseTimer);
  triggerTipCloseTimer = setTimeout(() => {
    showTriggerTip.value = false;
  }, 120);
}

function toggleTriggerTip() {
  if (showTriggerTip.value) {
    showTriggerTip.value = false;
  } else {
    void openTriggerTip();
  }
}

async function openRepairTip() {
  if (repairTipCloseTimer) {
    clearTimeout(repairTipCloseTimer);
    repairTipCloseTimer = null;
  }
  repairTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showRepairTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(repairInfoBtn.value, repairTipEl.value, repairTipStyle);
  });
}

function scheduleCloseRepairTip() {
  if (repairTipCloseTimer) clearTimeout(repairTipCloseTimer);
  repairTipCloseTimer = setTimeout(() => {
    showRepairTip.value = false;
  }, 120);
}

function toggleRepairTip() {
  if (showRepairTip.value) {
    showRepairTip.value = false;
  } else {
    void openRepairTip();
  }
}

async function openWinuhidTip() {
  if (winuhidTipCloseTimer) {
    clearTimeout(winuhidTipCloseTimer);
    winuhidTipCloseTimer = null;
  }
  winuhidTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showWinuhidTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(winuhidInfoBtn.value, winuhidTipEl.value, winuhidTipStyle);
  });
}

function scheduleCloseWinuhidTip() {
  if (winuhidTipCloseTimer) clearTimeout(winuhidTipCloseTimer);
  winuhidTipCloseTimer = setTimeout(() => {
    showWinuhidTip.value = false;
  }, 120);
}

function toggleWinuhidTip() {
  if (showWinuhidTip.value) {
    showWinuhidTip.value = false;
  } else {
    void openWinuhidTip();
  }
}

async function openAtvvTip() {
  if (atvvTipCloseTimer) {
    clearTimeout(atvvTipCloseTimer);
    atvvTipCloseTimer = null;
  }
  atvvTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showAtvvTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(atvvInfoBtn.value, atvvTipEl.value, atvvTipStyle);
  });
}

function scheduleCloseAtvvTip() {
  if (atvvTipCloseTimer) clearTimeout(atvvTipCloseTimer);
  atvvTipCloseTimer = setTimeout(() => {
    showAtvvTip.value = false;
  }, 120);
}

function toggleAtvvTip() {
  if (showAtvvTip.value) {
    showAtvvTip.value = false;
  } else {
    void openAtvvTip();
  }
}

async function openRestartTip() {
  if (restartTipCloseTimer) {
    clearTimeout(restartTipCloseTimer);
    restartTipCloseTimer = null;
  }
  restartTipStyle.value = {
    position: "fixed",
    top: "0px",
    left: "0px",
    visibility: "hidden",
    zIndex: "2000",
  };
  showRestartTip.value = true;
  await nextTick();
  requestAnimationFrame(() => {
    placeInfoTip(restartInfoBtn.value, restartTipEl.value, restartTipStyle);
  });
}

function scheduleCloseRestartTip() {
  if (restartTipCloseTimer) clearTimeout(restartTipCloseTimer);
  restartTipCloseTimer = setTimeout(() => {
    showRestartTip.value = false;
  }, 120);
}

function toggleRestartTip() {
  if (showRestartTip.value) {
    showRestartTip.value = false;
  } else {
    void openRestartTip();
  }
}

let tipViewportRaf: number | null = null;

function onViewportChange() {
  const anyTip =
    showVoiceShortcutTip.value ||
    showGainTip.value ||
    showTriggerTip.value ||
    showRepairTip.value ||
    showWinuhidTip.value ||
    showAtvvTip.value ||
    showRestartTip.value;
  if (!anyTip) return;
  if (tipViewportRaf != null) return;
  tipViewportRaf = requestAnimationFrame(() => {
    tipViewportRaf = null;
    if (showVoiceShortcutTip.value) {
      placeInfoTip(voiceInfoBtn.value, voiceTipEl.value, voiceTipStyle);
    }
    if (showGainTip.value) {
      placeInfoTip(gainInfoBtn.value, gainTipEl.value, gainTipStyle);
    }
    if (showTriggerTip.value) {
      placeInfoTip(triggerInfoBtn.value, triggerTipEl.value, triggerTipStyle);
    }
    if (showRepairTip.value) {
      placeInfoTip(repairInfoBtn.value, repairTipEl.value, repairTipStyle);
    }
    if (showWinuhidTip.value) {
      placeInfoTip(winuhidInfoBtn.value, winuhidTipEl.value, winuhidTipStyle);
    }
    if (showAtvvTip.value) {
      placeInfoTip(atvvInfoBtn.value, atvvTipEl.value, atvvTipStyle);
    }
    if (showRestartTip.value) {
      placeInfoTip(restartInfoBtn.value, restartTipEl.value, restartTipStyle);
    }
  });
}
const host = ref<HostStatus>({
  bridge_alive: false,
  audio_alive: false,
  cable_ready: false,
  winuhid_ready: false,
  atvv_ok: false,
  status_text: "正在启动",
  detail: "",
  tone: "warn",
  items: [
    { id: "cable", label: "虚拟声卡", state_label: "检测中", tone: "warn" },
    { id: "winuhid", label: "虚拟键盘", state_label: "检测中", tone: "warn" },
    { id: "audio", label: "语音路由", state_label: "检测中", tone: "warn" },
    { id: "bridge", label: "按键桥接", state_label: "检测中", tone: "warn" },
  ],
});

/** C1：桥接在跑且 ATVV 未订阅 → 音频信号旁红字 */
const showAtvvFailLabel = computed(
  () => Boolean(host.value.bridge_alive) && !(voiceMeter.value.atvvOk || host.value.atvv_ok)
);

const voiceShortcutEnabled = computed({
  get: () => config.value?.voice_shortcut_enabled !== false,
  set: (v: boolean) => {
    if (!config.value) return;
    config.value.voice_shortcut_enabled = v;
    void persistVoiceSettings();
  },
});

const GAIN_MIN = -12;
const GAIN_MAX = 30;
const GAIN_STEP = 1;
/** 连点 ± 时合并为一次保存，避免并发写配置竞态 */
const GAIN_SAVE_DEBOUNCE_MS = 300;

let gainSaveTimer: ReturnType<typeof setTimeout> | null = null;
let gainSaveSeq = 0;
let voiceSettingsSaveSeq = 0;
/** 串行化 xiaomi 配置写盘，避免增益/语音设置并发覆盖 */
let configSaveQueue: Promise<void> = Promise.resolve();

function cancelGainPersistSchedule() {
  if (gainSaveTimer) {
    clearTimeout(gainSaveTimer);
    gainSaveTimer = null;
  }
}

function runSerializedConfigSave(task: () => Promise<void>): Promise<void> {
  const run = configSaveQueue.then(task);
  configSaveQueue = run.catch(() => {});
  return run;
}

/** 非增益类 xiaomi 配置保存：取消待写入的增益 debounce，并入串行队列 */
async function saveXiaomiConfig(cfg: DeviceConfig): Promise<boolean> {
  cancelGainPersistSchedule();
  let ok = false;
  await runSerializedConfigSave(async () => {
    ok = await configStore.saveConfig(type, cfg);
  });
  return ok;
}

const gainToastVisible = ref(false);
const gainToastMessage = ref("");
const gainToastError = ref(false);
let gainToastTimer: ReturnType<typeof setTimeout> | null = null;

function showGainToast(message: string, isError = false) {
  gainToastMessage.value = message;
  gainToastError.value = isError;
  gainToastVisible.value = true;
  if (gainToastTimer) clearTimeout(gainToastTimer);
  gainToastTimer = setTimeout(() => {
    gainToastVisible.value = false;
    gainToastTimer = null;
  }, 2000);
}

const gainDb = computed({
  get: () => config.value?.gain_db ?? 10,
  set: (v: number | string) => {
    if (!config.value) return;
    const n = typeof v === "number" ? v : Number(v);
    if (Number.isNaN(n)) return;
    config.value.gain_db = Math.min(GAIN_MAX, Math.max(GAIN_MIN, n));
    scheduleGainPersist();
  },
});

function stepGain(delta: number) {
  gainDb.value = Math.min(GAIN_MAX, Math.max(GAIN_MIN, gainDb.value + delta));
}

function clampGainOnBlur() {
  if (!config.value) return;
  const n = Number(config.value.gain_db);
  if (Number.isNaN(n)) {
    config.value.gain_db = 10;
  } else {
    config.value.gain_db = Math.min(GAIN_MAX, Math.max(GAIN_MIN, n));
  }
  flushGainPersist();
}

function scheduleGainPersist() {
  if (gainSaveTimer) clearTimeout(gainSaveTimer);
  gainSaveTimer = setTimeout(() => {
    gainSaveTimer = null;
    void persistGainSettings();
  }, GAIN_SAVE_DEBOUNCE_MS);
}

function flushGainPersist() {
  if (gainSaveTimer) {
    clearTimeout(gainSaveTimer);
    gainSaveTimer = null;
  }
  void persistGainSettings();
}

async function persistGainSettings() {
  if (!config.value || configLoadState.value !== "ready") return;
  await runSerializedConfigSave(async () => {
    const seq = ++gainSaveSeq;
    const ok = await configStore.saveConfig(type, { ...config.value! });
    if (seq !== gainSaveSeq) return;
    if (ok) {
      showGainToast(t("增益新数值已生效。", "Gain updated."));
      return;
    }
    showGainToast(t("增益值更新失败，请调整数值重试。", "Couldn't update gain. Adjust the value and try again."), true);
    await configStore.loadConfig(type);
    if (seq !== gainSaveSeq) return;
    if (configStore.loadStates[type] !== "ready") {
      showGainToast(t("增益值更新失败，请刷新页面后重试。", "Couldn't update gain. Refresh the page and try again."), true);
    }
  });
}

async function persistVoiceSettings() {
  if (!config.value) return;
  cancelGainPersistSchedule();
  await runSerializedConfigSave(async () => {
    const seq = ++voiceSettingsSaveSeq;
    const ok = await configStore.saveConfig(type, { ...config.value! });
    if (seq !== voiceSettingsSaveSeq) return;
    if (ok) return;
    prependLog("语音设置保存失败，请重试", "Couldn't save voice settings. Try again.");
    await configStore.loadConfig(type);
    if (seq !== voiceSettingsSaveSeq) return;
    if (configStore.loadStates[type] !== "ready") {
      prependLog("配置重新加载失败，请刷新页面", "Couldn't reload settings. Refresh the page.");
    }
  });
}

/** 输入法一键预设（微信 / 豆包 / 千问等） */
function isWechatPreset(id: ImePresetId): boolean {
  return id.startsWith("wechat-");
}

function isDoubaoHoldPreset(id: ImePresetId): boolean {
  return id === "doubao-hold";
}

function presetShortcutLabel(preset: ImePresetDefinition): string {
  return preset.shortcutVks.map((vk) => vkDisplayName(vk)).join(" + ");
}

async function applyImePreset(presetId: ImePresetId) {
  if (!config.value) return;
  const definition = IME_PRESETS[presetId];
  if (!definition) return;
  const next = applyImePresetConfig(config.value, presetId);
  config.value.button_bindings = next.button_bindings;
  config.value.voice_hotkey = next.voice_hotkey;
  config.value.voice_shortcut_enabled = true;
  config.value.trigger_mode = next.trigger_mode;
  config.value.voice_release_behavior = next.voice_release_behavior;
  const ok = await saveXiaomiConfig(next);
  if (!ok) {
    prependLog("预设应用失败，请重试", "Couldn't apply preset. Try again.");
    await configStore.loadConfig(type);
    return;
  }
  setupApplyHint.value = definition.applyHint;
  prependLog(definition.logMessage);
  window.setTimeout(() => {
    if (setupApplyHint.value.startsWith("已应用")) setupApplyHint.value = "";
  }, 4000);
}

async function onKeyMappingSave(cfg: DeviceConfig) {
  const ok = await saveXiaomiConfig(cfg);
  if (!ok) prependLog("按键映射保存失败，请重试", "Couldn't save key mapping. Try again.");
}

let hostPollTimer: ReturnType<typeof setInterval> | null = null;
let devicePollTimer: ReturnType<typeof setInterval> | null = null;

function itemToneClass(tone: string): string {
  if (tone === "ok") return "ok";
  if (tone === "warn") return "warn";
  return "error";
}

interface LogEntry {
  id: number;
  time: string;
  text: string;
  en?: string;
}

const logs = ref<LogEntry[]>([]);
const logAreaRef = ref<HTMLElement | null>(null);
let logSeq = 0;
let unlistenKey: UnlistenFn | null = null;
let unlistenMeter: UnlistenFn | null = null;
let unlistenAtvvRepair: UnlistenFn | null = null;
let unlistenAtvvCancel: UnlistenFn | null = null;
let unlistenWinuhidProgress: UnlistenFn | null = null;
let unlistenWinuhidComplete: UnlistenFn | null = null;
let unlistenWinuhidError: UnlistenFn | null = null;
let unlistenCableProgress: UnlistenFn | null = null;
let unlistenCableComplete: UnlistenFn | null = null;
let unlistenCableError: UnlistenFn | null = null;

function formatTime(d = new Date()): string {
  return d.toLocaleTimeString("zh-CN", { hour12: false });
}

/** 前端自己写的日志传中英两份（切换语言时跟着变）；后端来的只传原文，显示时用 tb() 翻译 */
function prependLog(text: string, en?: string) {
  logs.value.unshift({
    id: ++logSeq,
    time: formatTime(),
    text,
    en,
  });
  if (logs.value.length > 80) {
    logs.value.length = 80;
  }
  nextTick(() => {
    const el = logAreaRef.value;
    if (el) {
      // 最新在顶部，自动滚回顶端
      el.scrollTop = 0;
    }
  });
}

function resolveKeyLabel(buttonId: string): string {
  const aliases = config.value?.button_aliases;
  if (aliases && aliases[buttonId]) return aliases[buttonId];
  const fallback: Record<string, string> = {
    power: t("电源", "Power"),
    volume_up: t("音量+", "Vol+"),
    volume_down: t("音量-", "Vol-"),
    up: t("上", "Up"),
    down: t("下", "Down"),
    left: t("左", "Left"),
    right: t("右", "Right"),
    dpad_up: t("上", "Up"),
    dpad_down: t("下", "Down"),
    dpad_left: t("左", "Left"),
    dpad_right: t("右", "Right"),
    ok: t("确认", "OK"),
    back: t("返回", "Back"),
    home: t("主页", "Home"),
    menu: t("菜单", "Menu"),
    mic: t("语音", "Voice"),
    voice: t("语音", "Voice"),
    volume_mute: t("静音", "Mute"),
    mute: t("静音", "Mute"),
    tv: "TV",
  };
  return fallback[buttonId] || buttonId;
}

function bindingAliases(buttonId: string): string[] {
  switch (buttonId) {
    case "mic":
    case "voice":
      return ["mic", "voice"];
    case "mute":
    case "volume_mute":
      return ["mute", "volume_mute"];
    case "up":
    case "dpad_up":
      return ["up", "dpad_up"];
    case "down":
    case "dpad_down":
      return ["down", "dpad_down"];
    case "left":
    case "dpad_left":
      return ["left", "dpad_left"];
    case "right":
    case "dpad_right":
      return ["right", "dpad_right"];
    default:
      return [buttonId];
  }
}

function resolveMappedActionLabel(buttonId: string): string {
  const bindings = config.value?.button_bindings;
  if (!bindings) return t("未绑定", "Unbound");
  let action = bindings[buttonId];
  if (!action) {
    for (const alt of bindingAliases(buttonId)) {
      if (bindings[alt]) {
        action = bindings[alt];
        break;
      }
    }
  }
  if (!action || action.type === "None") return t("未绑定", "Unbound");
  if (action.type === "SingleKey") return vkDisplayName(Number(action.value));
  if (action.type === "ComboKey") {
    const arr = Array.isArray(action.value) ? action.value : [];
    return arr.map((v) => vkDisplayName(Number(v))).join(" + ");
  }
  if (action.type === "TextInput") return t(`文字: ${action.value}`, `Text: ${action.value}`);
  if (action.type === "LaunchApp") return t(`启动: ${action.value}`, `Launch: ${action.value}`);
  return "—";
}

function formatKeyEventLine(
  phase: "down" | "up",
  remoteLabel: string,
  mappedLabel: string | null,
): string {
  const phaseLabel = phase === "up" ? t("抬起", "Released") : t("按下", "Pressed");
  if (mappedLabel) {
    return `${phaseLabel} ${remoteLabel} → ${mappedLabel}`;
  }
  return `${phaseLabel} ${remoteLabel}`;
}

function showMappingFlash(
  remoteLabel: string,
  mappedLabel: string | null,
  phase: "down" | "up" = "down"
) {
  lastMappingFlash.value = {
    seq: ++mappingFlashSeq,
    phase,
    remote: remoteLabel,
    mapped: mappedLabel,
  };
  if (mappingFlashClearTimer) clearTimeout(mappingFlashClearTimer);
  mappingFlashClearTimer = setTimeout(() => {
    lastMappingFlash.value = null;
    mappingFlashClearTimer = null;
  }, 4500);
  // 状态日志只记配置映射，不再汇总漏键/吞键/真实输出
  if (phase === "down") {
    prependLog(formatKeyEventLine(phase, remoteLabel, mappedLabel));
  }
}

async function refreshHost() {
  try {
    host.value = await invoke<HostStatus>("get_xiaomi_host_status");
  } catch (e) {
    host.value = {
      bridge_alive: false,
      audio_alive: false,
      cable_ready: false,
      atvv_ok: false,
      status_text: "桥接未运行",
      detail: String(e),
      tone: "error",
      items: [
        { id: "cable", label: "虚拟声卡", state_label: "未知", tone: "error" },
        { id: "winuhid", label: "虚拟键盘", state_label: "未知", tone: "error" },
        { id: "audio", label: "语音路由", state_label: "未知", tone: "error" },
        { id: "bridge", label: "按键桥接", state_label: "未启动", tone: "error" },
      ],
    };
  }
}

async function restartBridge() {
  restarting.value = true;
  try {
    await invoke("restart_xiaomi_bridge");
    await refreshHost();
  } catch (e) {
    host.value = {
      ...host.value,
      status_text: "重启失败",
      detail: String(e),
      tone: "error",
    };
  } finally {
    restarting.value = false;
  }
}

interface AtvvRepairResult {
  phase: string;
  message: string;
  atvvOk: boolean;
  hadConflicts: boolean;
}

async function repairAtvv() {
  if (atvvRepairing.value || restarting.value || voiceRepairing.value) return;
  atvvRepairing.value = true;
  let awaitingClear = false;
  try {
    const result = await invoke<AtvvRepairResult>("repair_xiaomi_atvv", {
      force: false,
    });
    awaitingClear = result.phase === "awaiting_conflict_clear";
    host.value = {
      ...host.value,
      status_text: result.atvvOk
        ? "ATVV 已修复"
        : awaitingClear
          ? "等待清理占用"
          : "ATVV 修复未完成",
      detail: result.message,
      tone: result.atvvOk ? "ok" : awaitingClear ? "warn" : "error",
    };
    if (awaitingClear) {
      return;
    }
    await refreshHost();
  } catch (e) {
    host.value = {
      ...host.value,
      status_text: "ATVV 修复失败",
      detail: String(e),
      tone: "error",
    };
  } finally {
    if (!awaitingClear) {
      atvvRepairing.value = false;
    }
  }
}

async function openLogs() {
  showLogModal.value = true;
  logCopyHint.value = "";
  logLoading.value = true;
  try {
    const result = await invoke<{ path: string; content: string }>("get_app_log");
    logPath.value = result.path || "";
    logText.value = result.content?.trim()
      ? result.content
      : t("（暂无日志）", "(No logs yet)");
  } catch (e) {
    logText.value = t(`读取日志失败: ${e}`, `Couldn't read log: ${tb(String(e))}`);
    logPath.value = "";
  } finally {
    logLoading.value = false;
  }
}

async function copyLog() {
  try {
    await navigator.clipboard.writeText(logText.value || "");
    logCopyHint.value = t("已复制", "Copied");
    setTimeout(() => {
      logCopyHint.value = "";
    }, 1500);
  } catch (e) {
    logCopyHint.value = t(`复制失败: ${e}`, `Copy failed: ${tb(String(e))}`);
  }
}

async function openLogExternally() {
  try {
    await invoke("open_app_log");
  } catch (e) {
    logCopyHint.value = t(`打开失败: ${e}`, `Couldn't open: ${tb(String(e))}`);
  }
}

async function clearLog() {
  if (logLoading.value) return;
  logLoading.value = true;
  logCopyHint.value = "";
  try {
    await invoke("clear_app_log");
    const result = await invoke<{ path: string; content: string }>("get_app_log");
    logPath.value = result.path || "";
    logText.value = result.content?.trim() ? result.content : t("（暂无日志）", "(No logs yet)");
  } catch (e) {
    logText.value = t(`清空日志失败: ${e}`, `Couldn't clear log: ${tb(String(e))}`);
  } finally {
    logLoading.value = false;
  }
}

interface VoiceEnvActionResult {
  ok: boolean;
  ready: boolean;
  needsChoice: boolean;
  needsReboot: boolean;
  message: string;
  reportPath?: string | null;
}

function applyVoiceEnvResult(result: VoiceEnvActionResult) {
  host.value = {
    ...host.value,
    detail: result.message,
    tone: result.ready ? "ok" : result.needsReboot ? "warn" : result.ok ? "warn" : "error",
  };
  prependLog(result.message);
  if (result.needsReboot) {
    voiceRebootMsg.value = result.message;
    showVoiceReboot.value = true;
  }
}

async function voiceDetectAndRepair() {
  if (voiceRepairing.value || winuhidRepairing.value || atvvRepairing.value || restarting.value) {
    return;
  }
  openVoiceRepairChoice();
}

/** 自动检测与修复（原点击按钮的默认行为） */
async function runVoiceAutoRepair() {
  voiceRepairing.value = true;
  showVoiceChoice.value = false;
  showVoiceReboot.value = false;
  try {
    // 与「使用内置驱动安装」相同：完整官方安装包 + 安装界面
    const result = await invoke<VoiceEnvActionResult>("repair_xiaomi_voice_env", {
      source: "embedded",
    });
    applyVoiceEnvResult(result);
    await refreshHost();
  } catch (e) {
    const msg = t(`修复虚拟声卡失败: ${e}`, `Sound card repair failed: ${tb(String(e))}`);
    prependLog(msg);
    host.value = { ...host.value, detail: msg, tone: "error" };
    voiceChoiceMsg.value = msg;
    voiceRepairTab.value = "repair";
    showVoiceChoice.value = true;
  } finally {
    voiceRepairing.value = false;
  }
}

async function chooseVoiceSource(
  source: "auto" | "embedded" | "download_page" | "download_zip",
) {
  if (source === "auto") {
    await runVoiceAutoRepair();
    return;
  }
  if (source === "download_zip") {
    await startCableZipDownload();
    return;
  }
  voiceRepairing.value = true;
  showVoiceChoice.value = false;
  showVoiceReboot.value = false;
  try {
    const result = await invoke<VoiceEnvActionResult>("repair_xiaomi_voice_env", {
      source,
    });
    applyVoiceEnvResult(result);
    await refreshHost();
  } catch (e) {
    const msg = t(`语音修复失败: ${e}`, `Voice repair failed: ${tb(String(e))}`);
    prependLog(msg);
    host.value = { ...host.value, detail: msg, tone: "error" };
    voiceChoiceMsg.value = msg;
    voiceRepairTab.value = "repair";
    showVoiceChoice.value = true;
  } finally {
    voiceRepairing.value = false;
  }
}

interface WinUHidActionResult {
  ok: boolean;
  ready: boolean;
  needsChoice: boolean;
  needsReboot: boolean;
  message: string;
  exportPath?: string | null;
}

function applyWinuhidResult(result: WinUHidActionResult) {
  prependLog(result.message);
  host.value = {
    ...host.value,
    winuhid_ready: result.ready,
    detail: result.message,
    tone: result.ready ? "ok" : result.needsReboot ? "warn" : result.ok ? "warn" : "error",
  };
  if (result.needsReboot) {
    voiceRebootMsg.value = result.message;
    showVoiceReboot.value = true;
  }
  if (result.needsChoice) {
    winuhidChoiceMsg.value = result.message;
    showWinuhidChoice.value = true;
  }
}

async function repairWinUHid() {
  if (winuhidRepairing.value || voiceRepairing.value || atvvRepairing.value || restarting.value) {
    return;
  }
  openWinuhidRepairChoice();
}

async function chooseWinuhidSource(
  source: "embedded" | "embedded_force" | "export" | "download_page" | "download_zip"
) {
  if (source === "download_zip") {
    await startWinuhidZipDownload();
    return;
  }
  winuhidRepairing.value = true;
  showWinuhidChoice.value = false;
  try {
    const result = await invoke<WinUHidActionResult>("repair_xiaomi_winuhid", {
      source,
      force: source === "embedded_force",
    });
    applyWinuhidResult(result);
    await refreshHost();
  } catch (e) {
    const msg = t(`虚拟键盘处理失败: ${e}`, `Virtual keyboard repair failed: ${tb(String(e))}`);
    prependLog(msg);
    host.value = { ...host.value, detail: msg, tone: "error" };
    winuhidChoiceMsg.value = msg;
    showWinuhidChoice.value = true;
  } finally {
    winuhidRepairing.value = false;
  }
}

function formatDownloadBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

const winuhidDownloadProgressLabel = computed(() => {
  const p = winuhidDownloadProgress.value;
  if (!p) return t("准备下载…", "Preparing download…");
  const downloaded = formatDownloadBytes(p.downloaded);
  if (p.total && p.total > 0) {
    const pct = p.percent != null ? t(`（${Math.round(p.percent)}%）`, ` (${Math.round(p.percent)}%)`) : "";
    return `${downloaded} / ${formatDownloadBytes(p.total)}${pct}`;
  }
  return t(`已下载 ${downloaded}`, `Downloaded ${downloaded}`);
});

function winuhidDownloadProgressWidth(): string {
  const p = winuhidDownloadProgress.value;
  if (p?.percent != null) return `${Math.min(100, Math.max(0, p.percent))}%`;
  if (winuhidDownloadPhase.value === "complete") return "100%";
  return "0%";
}

function resetWinuhidDownloadState() {
  winuhidDownloadPhase.value = "idle";
  winuhidDownloadProgress.value = null;
  winuhidDownloadMessage.value = "";
}

function resetCableDownloadState() {
  cableDownloadPhase.value = "idle";
  cableDownloadProgress.value = null;
  cableDownloadMessage.value = "";
}

const cableDownloadProgressLabel = computed(() => {
  const p = cableDownloadProgress.value;
  if (!p) return t("准备下载…", "Preparing download…");
  const downloaded = formatDownloadBytes(p.downloaded);
  if (p.total && p.total > 0) {
    const pct = p.percent != null ? t(`（${Math.round(p.percent)}%）`, ` (${Math.round(p.percent)}%)`) : "";
    return `${downloaded} / ${formatDownloadBytes(p.total)}${pct}`;
  }
  return t(`已下载 ${downloaded}`, `Downloaded ${downloaded}`);
});

function cableDownloadProgressWidth(): string {
  const p = cableDownloadProgress.value;
  if (p?.percent != null) return `${Math.min(100, Math.max(0, p.percent))}%`;
  if (cableDownloadPhase.value === "complete") return "100%";
  return "0%";
}

async function refreshCableZipName() {
  try {
    const status = await invoke<{ downloadZipUrl?: string }>("get_xiaomi_voice_env_status");
    const url = status.downloadZipUrl || "";
    const name = url.split("/").pop();
    if (name) cableZipDefaultName.value = name;
  } catch {
    /* ignore */
  }
}

async function startCableZipDownload() {
  if (cableDownloadPhase.value === "downloading" || voiceRepairing.value) return;

  const dest = await save({
    defaultPath: cableZipDefaultName.value,
    filters: [{ name: t("ZIP 压缩包", "ZIP archive"), extensions: ["zip"] }],
    title: t("保存 VB-CABLE 驱动包", "Save VB-CABLE driver package"),
  });
  if (!dest) return;

  cableDownloadPhase.value = "downloading";
  cableDownloadProgress.value = { downloaded: 0, total: null, percent: null };
  cableDownloadMessage.value = "";

  try {
    await invoke("download_xiaomi_vbcable_zip", { destPath: dest });
  } catch (e) {
    cableDownloadPhase.value = "error";
    cableDownloadMessage.value = String(e);
    prependLog(`VB-CABLE 驱动包下载失败: ${e}`, `VB-CABLE driver download failed: ${tb(String(e))}`);
  }
}

/** 停止并丢弃半成品，恢复初始态（不关弹窗，便于重试或选其它项） */
async function stopCableZipDownload() {
  if (cableDownloadPhase.value !== "downloading") return;
  try {
    await invoke("cancel_xiaomi_vbcable_zip_download");
  } catch (e) {
    console.warn("cancel vbcable download failed:", e);
  }
  resetCableDownloadState();
  prependLog("已停止 VB-CABLE 驱动包下载", "Stopped VB-CABLE driver download");
}

/** 点「修复虚拟声卡」先弹出选项，不直接跑修复（便于测下载等路径） */
function openVoiceRepairChoice() {
  showVoiceReboot.value = false;
  resetCableDownloadState();
  voiceRepairTab.value = "repair";
  voiceChoiceMsg.value = t("请选择检测 / 安装方式：", "Choose how to check / install:");
  showVoiceChoice.value = true;
  void refreshCableZipName();
}

async function refreshWinuhidZipName() {
  try {
    const status = await invoke<{ downloadZipUrl?: string }>("get_xiaomi_winuhid_status");
    const url = status.downloadZipUrl || "";
    const name = url.split("/").pop();
    if (name) winuhidZipDefaultName.value = name;
  } catch {
    /* ignore */
  }
}

async function startWinuhidZipDownload() {
  if (winuhidDownloadPhase.value === "downloading" || winuhidRepairing.value) return;

  const dest = await save({
    defaultPath: winuhidZipDefaultName.value,
    filters: [{ name: t("ZIP 压缩包", "ZIP archive"), extensions: ["zip"] }],
    title: t("保存 WinUHid 驱动包", "Save WinUHid driver package"),
  });
  if (!dest) return;

  winuhidDownloadPhase.value = "downloading";
  winuhidDownloadProgress.value = { downloaded: 0, total: null, percent: null };
  winuhidDownloadMessage.value = "";

  try {
    await invoke("download_xiaomi_winuhid_zip", { destPath: dest });
  } catch (e) {
    winuhidDownloadPhase.value = "error";
    winuhidDownloadMessage.value = String(e);
    prependLog(`WinUHid 驱动包下载失败: ${e}`, `WinUHid driver download failed: ${tb(String(e))}`);
  }
}

async function stopWinuhidZipDownload() {
  if (winuhidDownloadPhase.value !== "downloading") return;
  try {
    await invoke("cancel_xiaomi_winuhid_zip_download");
  } catch (e) {
    console.warn("cancel winuhid download failed:", e);
  }
  resetWinuhidDownloadState();
  prependLog("已停止 WinUHid 驱动包下载", "Stopped WinUHid driver download");
}

function openWinuhidRepairChoice() {
  resetWinuhidDownloadState();
  winuhidChoiceMsg.value = t("请选择修复或安装方式：", "Choose how to repair or install:");
  showWinuhidChoice.value = true;
  void refreshWinuhidZipName();
}

onMounted(async () => {
  prependLog("日志区准备就绪", "Log ready");
  await Promise.all([
    bridge.refreshStatus(type),
    configStore.loadConfig(type),
    refreshHost(),
    invoke("get_xiaomi_voice_meter")
      .then((s) => applyVoiceMeter(s as Record<string, unknown>))
      .catch(() => undefined),
  ]);
  hostPollTimer = setInterval(refreshHost, 1000);
  // 持续拉取设备信息（含电量），避免必须切页才刷新
  devicePollTimer = setInterval(() => {
    void bridge.refreshStatus(type);
  }, 1500);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);

  try {
    unlistenKey = await listen<{
      buttonId?: string;
      label?: string;
      message?: string;
      phase?: string;
    }>("xiaomi-key", (event) => {
      const p = event.payload;
      if (p.message) {
        prependLog(p.message);
        if (p.message.startsWith("电量")) {
          void bridge.refreshStatus(type);
        }
        return;
      }
      const id = p.buttonId || "unknown";
      const label = p.label ? tb(p.label) : resolveKeyLabel(id);
      const phase: "down" | "up" = p.phase === "up" ? "up" : "down";
      const isVoice = id === "mic" || id === "voice";
      const voiceMapOn = config.value?.voice_shortcut_enabled !== false;
      // D1：语音映射关闭时只显示按下/抬起，不写映射段
      const lineMapped = isVoice && !voiceMapOn ? null : resolveMappedActionLabel(id);
      showMappingFlash(label, lineMapped, phase);
    });
  } catch (e) {
    console.warn("listen xiaomi-key failed:", e);
  }

  try {
    unlistenMeter = await listen<Record<string, unknown>>("xiaomi-voice-meter", (event) => {
      applyVoiceMeter(event.payload);
    });
  } catch (e) {
    console.warn("listen xiaomi-voice-meter failed:", e);
  }

  try {
    unlistenAtvvRepair = await listen<{ ok?: boolean; message?: string }>(
      "xiaomi-atvv-repair-result",
      async (event) => {
        const p = event.payload || {};
        host.value = {
          ...host.value,
          status_text: p.ok ? "ATVV 已修复" : "ATVV 修复未完成",
          detail: p.message || "",
          tone: p.ok ? "ok" : "error",
        };
        atvvRepairing.value = false;
        await refreshHost();
      },
    );
  } catch (e) {
    console.warn("listen xiaomi-atvv-repair-result failed:", e);
  }

  try {
    unlistenAtvvCancel = await listen<{ message?: string }>(
      "xiaomi-atvv-repair-cancelled",
      (event) => {
        host.value = {
          ...host.value,
          status_text: "ATVV 修复已取消",
          detail: event.payload?.message || t("已取消修复", "Repair cancelled"),
          tone: "warn",
        };
        atvvRepairing.value = false;
      },
    );
  } catch (e) {
    console.warn("listen xiaomi-atvv-repair-cancelled failed:", e);
  }

  try {
    unlistenWinuhidProgress = await listen<{
      downloaded: number;
      total?: number | null;
      percent?: number | null;
    }>("winuhid-download-progress", (event) => {
      if (!event.payload) return;
      winuhidDownloadPhase.value = "downloading";
      winuhidDownloadProgress.value = event.payload;
    });
  } catch (e) {
    console.warn("listen winuhid-download-progress failed:", e);
  }

  try {
    unlistenWinuhidComplete = await listen<{ path: string }>(
      "winuhid-download-complete",
      (event) => {
        const path = event.payload?.path || "";
        const msg = path
          ? t(
              `WinUHid 驱动包已保存到：${path}。请解压后阅读「安装说明.txt」，双击 Run-Install.cmd 安装。`,
              `WinUHid driver package saved to: ${path}. Unzip it, read “安装说明.txt”, then double-click Run-Install.cmd to install.`,
            )
          : t(
              "WinUHid 驱动包下载完成。请解压后阅读「安装说明.txt」安装。",
              "WinUHid driver package downloaded. Unzip it and follow “安装说明.txt” to install.",
            );
        prependLog(msg);
        resetWinuhidDownloadState();
      },
    );
  } catch (e) {
    console.warn("listen winuhid-download-complete failed:", e);
  }

  try {
    unlistenWinuhidError = await listen<{ message: string }>(
      "winuhid-download-error",
      (event) => {
        const msg = event.payload?.message || t("下载失败", "Download failed");
        if (msg.includes("已取消")) {
          resetWinuhidDownloadState();
          return;
        }
        winuhidDownloadPhase.value = "error";
        winuhidDownloadProgress.value = null;
        winuhidDownloadMessage.value = msg;
        prependLog(`WinUHid 驱动包下载失败: ${msg}`, `WinUHid driver download failed: ${tb(msg)}`);
      },
    );
  } catch (e) {
    console.warn("listen winuhid-download-error failed:", e);
  }

  try {
    unlistenCableProgress = await listen<{
      downloaded: number;
      total?: number | null;
      percent?: number | null;
    }>("vbcable-download-progress", (event) => {
      if (!event.payload) return;
      cableDownloadPhase.value = "downloading";
      cableDownloadProgress.value = event.payload;
    });
  } catch (e) {
    console.warn("listen vbcable-download-progress failed:", e);
  }

  try {
    unlistenCableComplete = await listen<{ path: string }>(
      "vbcable-download-complete",
      (event) => {
        const path = event.payload?.path || "";
        const msg = path
          ? t(
              `VB-CABLE 驱动包已保存到：${path}。请解压后按说明安装，完成后点「修复虚拟声卡」→「自动修复」。`,
              `VB-CABLE driver package saved to: ${path}. Unzip and install it, then click “Repair sound card” → “Auto repair”.`,
            )
          : t(
              "VB-CABLE 驱动包下载完成。请解压安装后点「修复虚拟声卡」→「自动修复」。",
              "VB-CABLE driver package downloaded. Unzip and install it, then click “Repair sound card” → “Auto repair”.",
            );
        prependLog(msg);
        resetCableDownloadState();
      },
    );
  } catch (e) {
    console.warn("listen vbcable-download-complete failed:", e);
  }

  try {
    unlistenCableError = await listen<{ message: string }>(
      "vbcable-download-error",
      (event) => {
        const msg = event.payload?.message || t("下载失败", "Download failed");
        if (msg.includes("已取消")) {
          resetCableDownloadState();
          return;
        }
        cableDownloadPhase.value = "error";
        cableDownloadProgress.value = null;
        cableDownloadMessage.value = msg;
        prependLog(`VB-CABLE 驱动包下载失败: ${msg}`, `VB-CABLE driver download failed: ${tb(msg)}`);
      },
    );
  } catch (e) {
    console.warn("listen vbcable-download-error failed:", e);
  }
});

onUnmounted(() => {
  unlistenKey?.();
  unlistenMeter?.();
  unlistenAtvvRepair?.();
  unlistenAtvvCancel?.();
  unlistenWinuhidProgress?.();
  unlistenWinuhidComplete?.();
  unlistenWinuhidError?.();
  unlistenCableProgress?.();
  unlistenCableComplete?.();
  unlistenCableError?.();
  if (hostPollTimer) clearInterval(hostPollTimer);
  if (devicePollTimer) clearInterval(devicePollTimer);
  if (voiceTipCloseTimer) clearTimeout(voiceTipCloseTimer);
  if (gainTipCloseTimer) clearTimeout(gainTipCloseTimer);
  if (triggerTipCloseTimer) clearTimeout(triggerTipCloseTimer);
  if (repairTipCloseTimer) clearTimeout(repairTipCloseTimer);
  if (winuhidTipCloseTimer) clearTimeout(winuhidTipCloseTimer);
  if (atvvTipCloseTimer) clearTimeout(atvvTipCloseTimer);
  if (restartTipCloseTimer) clearTimeout(restartTipCloseTimer);
  if (gainSaveTimer) {
    flushGainPersist();
  }
  if (gainToastTimer) clearTimeout(gainToastTimer);
  if (mappingFlashClearTimer) clearTimeout(mappingFlashClearTimer);
  if (tipViewportRaf != null) {
    cancelAnimationFrame(tipViewportRaf);
    tipViewportRaf = null;
  }
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
});

watch(
  () => device.value.status,
  (status, prev) => {
    if (status === prev) return;
    if (status === "Connected") {
      const name = device.value.device_name || "MI RC";
      prependLog(`已连接 ${name}`, `Connected to ${name}`);
    } else if (status === "Connecting") {
      prependLog("正在连接...", "Connecting...");
    } else if (status === "Disconnected") {
      prependLog("已断开", "Disconnected");
    } else if (status.startsWith("Error")) {
      prependLog(bridge.statusLabel(status));
    }
  }
);

function toggleConnection() {
  if (device.value.status === "Connected") {
    bridge.stopBridge(type);
  } else {
    bridge.startBridge(type);
  }
}

async function retryLoadConfig() {
  await configStore.loadConfig(type);
  if (configStore.loadErrors[type]) {
    prependLog(
      t(
        `配置加载失败: ${configStore.loadErrors[type]}`,
        `Couldn't load settings: ${tb(configStore.loadErrors[type])}`,
      ),
    );
  }
}

</script>

<template>
  <div class="page">
    <header class="page-header">
      <div class="title-row">
        <h2>{{ t("小米遥控器 2 Pro", "Xiaomi Remote 2 Pro") }}</h2>
      </div>
      <DeviceStatus
        :status="device.status"
        :loading="bridge.loading[type]"
        @toggle="toggleConnection"
      />
    </header>

    <div class="overview-row">
      <div class="overview-left">
        <div class="device-info-row">
          <div class="device-info-col">
            <div class="info-line">
              <span class="info-label">{{ t("设备名称", "Device name") }}</span>
              <span class="info-value">{{ device.device_name || "—" }}</span>
            </div>
            <div class="info-line">
              <span class="info-label">{{ t("蓝牙地址", "Bluetooth address") }}</span>
              <span class="info-value">{{ device.device_address || "—" }}</span>
            </div>
          </div>
          <div class="device-info-col">
            <div class="info-line">
              <span class="info-label">{{ t("剩余电量", "Battery") }}</span>
              <span class="info-value info-value-battery">
                <BatteryLevelIcon :level="device.battery_level" />
                {{ device.battery_level != null ? device.battery_level + "%" : "—" }}
              </span>
            </div>
            <div class="info-line">
              <span class="info-label">{{ t("连接方式", "Connection") }}</span>
              <span class="info-value">{{ t("蓝牙 BLE", "Bluetooth BLE") }}</span>
            </div>
          </div>
          <div
            class="info-item info-item-audio"
            :class="{
              'is-session': voiceMeter.bleState === 'session',
              'is-receiving': voiceMeter.bleState === 'receiving',
            }"
            :title="t('遥控器 BLE 解码后的 PCM', 'PCM decoded from the remote’s BLE audio')"
          >
            <div class="audio-label-row">
              <span class="info-label">{{ t("音频信号", "Audio signal") }}</span>
              <span
                v-if="showAtvvFailLabel"
                class="audio-atvv-fail"
              >{{ t("ATVV 未连接", "ATVV not connected") }}</span>
              <span
                v-else-if="voiceMeter.bleState !== 'idle'"
                class="audio-state"
              >{{ bleSignalLabel }}</span>
            </div>
            <div class="ble-wave" aria-hidden="true">
              <svg class="ble-wave-svg" viewBox="0 0 100 100" preserveAspectRatio="none">
                <path class="ble-wave-fill" :d="waveAreaPath" />
                <polyline class="ble-wave-line" :points="waveLinePoints" />
              </svg>
            </div>
          </div>
          <div
            class="info-item info-item-cable-vol"
            :class="[
              `cable-zone-${cableVolZone}`,
              { 'is-active': cableReady && voiceMeter.cableActive },
            ]"
            :title="t('经增益处理后送往虚拟声卡的实时电平（dBFS，0 为数字满幅）', 'Live level sent to the virtual sound card after gain (dBFS, 0 = digital full scale)')"
          >
            <div class="audio-label-row cable-vol-label-row">
              <span v-if="cableReady" class="info-label">{{ isLinux ? t("虚拟麦克风音量", "Virtual mic level") : t("虚拟声卡音量", "Sound card level") }}</span>
              <span v-else-if="linuxAsrMode" class="info-label">{{ t("虚拟麦克风（本地识别模式不使用）", "Virtual mic (unused)") }}</span>
              <span v-else class="cable-vol-fail">{{ isLinux ? t("虚拟麦克风未就绪", "Virtual mic not ready") : t("虚拟声卡未就绪", "Sound card not ready") }}</span>
              <span
                class="cable-vol-state"
                :class="{
                  'is-low': cableReady && cableVolZone === 'low',
                  'is-high': cableReady && cableVolZone === 'high',
                  'is-ok': cableReady && cableVolZone === 'ok',
                  'is-idle': !cableReady || !voiceMeter.cableActive,
                  'is-sending':
                    cableReady &&
                    voiceMeter.cableActive &&
                    cableVolZone === 'idle',
                }"
              >{{ cableVolHint }}</span>
            </div>
            <CableVolRuler
              :level="voiceMeter.cableLevel"
              :disabled="!cableReady"
              :active="cableReady && voiceMeter.cableActive"
            />
          </div>
        </div>

        <section class="card host-card">
          <div class="host-status-row" role="list" :aria-label="t('运行状态', 'Runtime status')">
            <div
              v-for="item in host.items"
              :key="item.id"
              class="host-status-item"
              role="listitem"
            >
              <span
                class="host-dot"
                :class="itemToneClass(item.tone)"
                aria-hidden="true"
              />
              <span class="host-item-label">{{ tb(item.label) }}</span>
              <span class="host-item-state" :class="itemToneClass(item.tone)">
                {{ tb(item.state_label) }}
              </span>
            </div>
          </div>
          <p v-if="host.detail" class="host-detail">{{ tb(host.detail) }}</p>
          <div class="host-actions">
            <template v-if="!isLinux">
            <div class="host-action-group">
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="voiceRepairing || restarting"
                @click="voiceDetectAndRepair"
              >
                {{ voiceRepairing ? t("处理中...", "Working...") : t("修复虚拟声卡", "Repair sound card") }}
              </button>
              <button
                ref="repairInfoBtn"
                type="button"
                class="title-info"
                :aria-expanded="showRepairTip"
                :aria-label="t('修复虚拟声卡说明', 'About Repair sound card')"
                @mouseenter="openRepairTip"
                @mouseleave="scheduleCloseRepairTip"
                @focus="openRepairTip"
                @blur="scheduleCloseRepairTip"
                @click.stop="toggleRepairTip"
              >
                <span class="title-info-icon" aria-hidden="true">i</span>
              </button>
              <Teleport to="body">
                <div
                  v-if="showRepairTip"
                  ref="repairTipEl"
                  class="floating-info-tip voice-info-tip"
                  role="tooltip"
                  :style="repairTipStyle"
                  @mouseenter="openRepairTip"
                  @mouseleave="scheduleCloseRepairTip"
                >
                  <p class="tip-lead">
                    {{ t("用来检查并修好电脑上的语音通路（VB-CABLE 虚拟声卡），让遥控器麦克风声音能进系统、供输入法听写。", "Checks and fixes the PC voice path (VB-CABLE virtual sound card) so the remote’s mic audio reaches the system for IME dictation.") }}
                  </p>
                  <div class="tip-block tip-on">
                    <div class="tip-badge">{{ t("会做什么", "What it does") }}</div>
                    <ul>
                      <li>{{ t("检测 VB-CABLE 是否已安装、是否可用", "Checks whether VB-CABLE is installed and working") }}</li>
                      <li>{{ t("「自动修复」与「使用内置驱动安装」均打开内嵌官方安装包界面", "“Auto repair” and “Install bundled driver” both open the bundled official installer") }}</li>
                      <li>{{ t("也可下载官网最新版自行安装", "Or download the latest version from the official site and install it yourself") }}</li>
                    </ul>
                  </div>
                  <div class="tip-block tip-off">
                    <div class="tip-badge">{{ t("什么时候点", "When to use") }}</div>
                    <ul>
                      <li>{{ t("首次使用语音，或重装系统 / 换电脑后", "First time using voice, or after reinstalling Windows / switching PCs") }}</li>
                      <li>{{ t("按语音键没声音、输入法听不到遥控器", "No sound from the voice key, or the IME can’t hear the remote") }}</li>
                      <li>{{ t("提示未检测到 VB-CABLE、语音环境异常时", "You’re told VB-CABLE wasn’t detected or the voice setup is broken") }}</li>
                    </ul>
                  </div>
                  <p class="tip-foot">
                    {{ t("点「修复虚拟声卡」会先弹出选项：「自动修复」或「使用内置官方 V2.15.18 版安装」都会打开官方安装界面（无黑框）。也可用下载包 / 官网。若提示必须重启，重启后再打开本软件。", "“Repair sound card” first shows options: “Auto repair” and “Install bundled official V2.15.18” both open the official installer (no console window). You can also use the download package or the official site. If a restart is required, restart and reopen this app.") }}
                  </p>
                </div>
              </Teleport>
            </div>
            <div class="host-action-group">
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="winuhidRepairing || voiceRepairing || atvvRepairing || restarting"
                @click="repairWinUHid"
              >
                {{ winuhidRepairing ? t("修复中...", "Repairing...") : t("修复虚拟键盘", "Repair keyboard") }}
              </button>
              <button
                ref="winuhidInfoBtn"
                type="button"
                class="title-info"
                :aria-expanded="showWinuhidTip"
                :aria-label="t('修复虚拟键盘说明', 'About Repair keyboard')"
                @mouseenter="openWinuhidTip"
                @mouseleave="scheduleCloseWinuhidTip"
                @focus="openWinuhidTip"
                @blur="scheduleCloseWinuhidTip"
                @click.stop="toggleWinuhidTip"
              >
                <span class="title-info-icon" aria-hidden="true">i</span>
              </button>
              <Teleport to="body">
                <div
                  v-if="showWinuhidTip"
                  ref="winuhidTipEl"
                  class="floating-info-tip voice-info-tip"
                  role="tooltip"
                  :style="winuhidTipStyle"
                  @mouseenter="openWinuhidTip"
                  @mouseleave="scheduleCloseWinuhidTip"
                >
                  <p class="tip-lead">
                    {{ t("在电脑里装一块「虚拟键盘」，让豆包、千问等输入法把遥控器的语音键当成真键盘按键，而不是普通模拟点击（那种方式常被输入法忽略）。", "Installs a “virtual keyboard” so IMEs like Doubao and Qwen treat the remote’s voice key as a real key press, not a simulated click (which IMEs often ignore).") }}
                  </p>
                  <div class="tip-block tip-on">
                    <div class="tip-badge">{{ t("会做什么", "What it does") }}</div>
                    <ul>
                      <li>{{ t("部署 WinUHid 组件，并安装内嵌的虚拟键盘驱动", "Deploys WinUHid and installs the bundled virtual keyboard driver") }}</li>
                      <li>{{ t("在系统里注册并启动虚拟键盘设备，让语音组合键按硬件方式注入", "Registers and starts the virtual keyboard device so voice hotkeys are injected as hardware input") }}</li>
                      <li>{{ t("完成后状态栏「虚拟键盘」应显示就绪；仍不行可选导出安装包", "Afterwards “Virtual keyboard” in the status bar should show ready; if not, export the installer") }}</li>
                    </ul>
                  </div>
                  <div class="tip-block tip-off">
                    <div class="tip-badge">{{ t("什么时候点", "When to use") }}</div>
                    <ul>
                      <li>{{ t("首次用豆包 / 千问语音，或重装系统、换电脑后", "First time using Doubao / Qwen voice, or after reinstalling Windows / switching PCs") }}</li>
                      <li>{{ t("状态里「虚拟键盘」未就绪，或按语音键唤不醒输入法", "“Virtual keyboard” isn’t ready, or the voice key doesn’t wake the IME") }}</li>
                      <li>{{ t("日志提示需要 WinUHid、语音键被拦截时", "The log says WinUHid is needed or the voice key is blocked") }}</li>
                    </ul>
                  </div>
                  <p class="tip-foot">
                    {{ t("会弹出 UAC 管理员确认，请点允许。点按钮后会打开修复选项：自动修复、强制重装、导出到桌面或从 Release 下载。仅当 Windows 返回必须重启时才重启；否则再点一次「自动修复」。这和「修复虚拟声卡」「修复 ATVV 连接」不是一回事。", "A UAC admin prompt will appear; click Yes. The button opens repair options: auto repair, force reinstall, export to desktop, or download from Release. Restart only if Windows says it’s required; otherwise click “Auto repair” again. This is separate from “Repair sound card” and “Repair ATVV”.") }}
                  </p>
                </div>
              </Teleport>
            </div>
            </template>
            <div class="host-action-group">
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="atvvRepairing || restarting || voiceRepairing || winuhidRepairing"
                @click="repairAtvv"
              >
                {{ atvvRepairing ? t("修复中...", "Repairing...") : t("修复 ATVV 连接", "Repair ATVV") }}
              </button>
              <button
                ref="atvvInfoBtn"
                type="button"
                class="title-info"
                :aria-expanded="showAtvvTip"
                :aria-label="t('修复 ATVV 连接说明', 'About Repair ATVV')"
                @mouseenter="openAtvvTip"
                @mouseleave="scheduleCloseAtvvTip"
                @focus="openAtvvTip"
                @blur="scheduleCloseAtvvTip"
                @click.stop="toggleAtvvTip"
              >
                <span class="title-info-icon" aria-hidden="true">i</span>
              </button>
              <Teleport to="body">
                <div
                  v-if="showAtvvTip"
                  ref="atvvTipEl"
                  class="floating-info-tip voice-info-tip"
                  role="tooltip"
                  :style="atvvTipStyle"
                  @mouseenter="openAtvvTip"
                  @mouseleave="scheduleCloseAtvvTip"
                >
                  <p class="tip-lead">
                    {{ t("修好遥控器到电脑的「语音专用蓝牙通道」（ATVV）。通道正常后，按住语音键才有绿色音频波动，语音听写才能用。", "Repairs the remote’s dedicated Bluetooth voice channel (ATVV). Once it works, holding the voice key shows a green waveform and dictation works.") }}
                  </p>
                  <div class="tip-block tip-on">
                    <div class="tip-badge">{{ t("会做什么", "What it does") }}</div>
                    <ul>
                      <li>{{ t("检查是否有其它遥控桥接软件占用", "Checks whether another remote bridge app is using it") }}</li>
                      <li v-if="!isLinux">{{ t("暂停 HID Tap 后软重启连接，并重新订阅语音通道", "Pauses HID Tap, soft-restarts the connection and resubscribes to the voice channel") }}</li>
                      <li v-else>{{ t("软重启与遥控器的蓝牙连接，并重新订阅语音通道", "Soft-restarts the Bluetooth connection to the remote and resubscribes to the voice channel") }}</li>
                      <li>{{ t("有占用时会先弹窗让你结束相关进程，再继续修复", "If something is using it, you’re asked to end those processes before repair continues") }}</li>
                    </ul>
                  </div>
                  <div class="tip-block tip-off">
                    <div class="tip-badge">{{ t("什么时候点", "When to use") }}</div>
                    <ul>
                      <li>{{ t("「音频信号」旁出现红字「ATVV 未连接」", "Red “ATVV not connected” appears next to “Audio signal”") }}</li>
                      <li>{{ t("按住语音键说话，绿色波形一直不动", "The green waveform stays flat while you hold the voice key and speak") }}</li>
                      <li v-if="!isLinux">{{ t("按语音键后记事本等处插入了日期时间", "Pressing the voice key inserts the date/time in Notepad etc.") }}</li>
                    </ul>
                  </div>
                  <p class="tip-foot">
                    {{ t("平时语音和波形都正常就不必点。这和「修复虚拟声卡」不同：那边管电脑声卡，这边管遥控器蓝牙语音通道。", "No need to click if voice and the waveform work. Unlike “Repair sound card” (the PC’s sound card), this fixes the remote’s Bluetooth voice channel.") }}
                  </p>
                </div>
              </Teleport>
            </div>
            <div class="host-action-group">
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="restarting || voiceRepairing || atvvRepairing"
                @click="restartBridge"
              >
                {{ restarting ? t("重启中...", "Restarting...") : t("重启桥接", "Restart bridge") }}
              </button>
              <button
                ref="restartInfoBtn"
                type="button"
                class="title-info"
                :aria-expanded="showRestartTip"
                :aria-label="t('重启桥接说明', 'About Restart bridge')"
                @mouseenter="openRestartTip"
                @mouseleave="scheduleCloseRestartTip"
                @focus="openRestartTip"
                @blur="scheduleCloseRestartTip"
                @click.stop="toggleRestartTip"
              >
                <span class="title-info-icon" aria-hidden="true">i</span>
              </button>
              <Teleport to="body">
                <div
                  v-if="showRestartTip"
                  ref="restartTipEl"
                  class="floating-info-tip voice-info-tip"
                  role="tooltip"
                  :style="restartTipStyle"
                  @mouseenter="openRestartTip"
                  @mouseleave="scheduleCloseRestartTip"
                >
                  <p class="tip-lead">
                    {{ t("软重启「与遥控器的蓝牙连接」，按最新配置重新连上；无需退出整个应用。", "Soft-restarts the Bluetooth connection to the remote and reconnects with the latest settings — no need to quit the app.") }}
                  </p>
                  <div class="tip-block tip-on">
                    <div class="tip-badge">{{ t("会做什么", "What it does") }}</div>
                    <ul>
                      <li>{{ t("停止并重新拉起蓝牙 / ATVV 连接", "Stops and restarts the Bluetooth / ATVV connection") }}</li>
                      <li>{{ t("按当前映射、增益等配置重新尝试连接遥控器", "Reconnects to the remote using the current mapping, gain and other settings") }}</li>
                      <li>{{ t("语音路由异常时也会顺带尝试拉起", "Also tries to restart voice routing if it’s down") }}</li>
                    </ul>
                  </div>
                  <div class="tip-block tip-off">
                    <div class="tip-badge">{{ t("什么时候点", "When to use") }}</div>
                    <ul>
                      <li>{{ t("虚拟声卡、ATVV 或蓝牙连接异常", "Problems with the virtual sound card, ATVV or Bluetooth") }}</li>
                      <li>{{ t("状态显示异常、按键失灵、连上又掉线", "Odd status, unresponsive keys, or repeated disconnects") }}</li>
                      <li>{{ t("长时间不用后突然不响应，想快速恢复", "Stops responding after a long idle and you want a quick fix") }}</li>
                    </ul>
                  </div>
                  <p class="tip-foot">
                    {{ t("返回 / 音量专用通道会尽量保持，一般不必为此反复重启。若仍无效，可再试「修复虚拟声卡」，或查看日志。", "The Back / volume channel is kept alive where possible, so you rarely need to restart for it. If it still fails, try “Repair sound card” or check the logs.") }}
                  </p>
                </div>
              </Teleport>
            </div>
            <div v-if="!isLinux" class="host-action-group">
              <button
                class="btn btn-secondary"
                type="button"
                @click="showSetupTips = true"
              >
                {{ t("输入法设置说明", "IME setup guide") }}
              </button>
            </div>
          </div>
        </section>
      </div>

      <aside class="log-aside">
        <section class="card log-card">
          <div class="log-card-head">
            <p class="card-text">{{ t("状态日志", "Status log") }}</p>
            <button class="btn btn-tiny btn-secondary" type="button" @click="openLogs">
              {{ t("日志", "Logs") }}
            </button>
          </div>
          <div ref="logAreaRef" class="log-area">
            <p v-for="entry in logs" :key="entry.id" class="log-entry">
              <span class="log-time">{{ entry.time }}</span>
              <span class="log-text">{{ entry.en !== undefined ? t(entry.text, entry.en) : tb(entry.text) }}</span>
            </p>
          </div>
        </section>
      </aside>
    </div>

    <div class="page-body">
      <!-- 小米专用运行状态弹层等 -->
      <div v-if="showSetupTips" class="voice-modal-backdrop" @click.self="showSetupTips = false">
        <div class="voice-modal setup-tips-modal" role="dialog" aria-modal="true" aria-labelledby="setup-tips-title">
          <div class="setup-tips-head">
            <h3 id="setup-tips-title">{{ t("输入法设置", "IME setup") }}</h3>
            <button class="btn btn-secondary" type="button" @click="showSetupTips = false">{{ t("关闭", "Close") }}</button>
          </div>
          <div class="setup-ime-tabs" role="tablist" :aria-label="t('输入法分类', 'IME categories')">
            <button
              v-for="tab in imeTabs"
              :key="tab.id"
              type="button"
              class="setup-ime-tab"
              :class="{ active: setupImeTab === tab.id }"
              role="tab"
              :aria-selected="setupImeTab === tab.id"
              @click="setupImeTab = tab.id"
            >
              {{ tab.label }}
            </button>
          </div>
          <div class="setup-tips-body">
          <p v-if="setupApplyHint" class="setup-apply-hint setup-apply-hint-global">
            {{ setupApplyHint }}
          </p>

          <div v-if="setupImeTab === 'faq'" class="setup-ime-panel" role="tabpanel">
            <div class="setup-ime-warn setup-ime-faq-warn" role="note">
              <p class="setup-ime-warn-title">{{ imeFaq.warnTitle }}</p>
            </div>
            <section
              v-for="(section, sIdx) in imeFaq.sections"
              :key="sIdx"
              class="setup-faq-section"
            >
              <h4 class="setup-faq-section-title">{{ section.title }}</h4>
              <ol class="setup-ime-steps setup-faq-list">
                <li v-for="(item, idx) in section.items" :key="idx">{{ item }}</li>
              </ol>
            </section>
          </div>

          <div v-else-if="setupImeTab === 'qianwen'" class="setup-ime-panel" role="tabpanel">
            <article class="setup-ime-card">
              <header class="setup-ime-head">
                <h4>{{ qianwenGuide.title }}</h4>
                <span class="setup-ime-tag">{{ qianwenGuide.tag }}</span>
              </header>
              <ol class="setup-ime-steps">
                <li v-for="(step, idx) in qianwenGuide.steps" :key="idx">
                  <span class="setup-ime-step-text">{{ step.text }}</span>
                  <span v-if="step.aside" class="setup-ime-step-aside">{{ step.aside }}</span>
                </li>
              </ol>
              <div class="setup-ime-apply setup-ime-apply-row">
                <button
                  v-for="preset in qianwenPresets"
                  :key="preset.id"
                  class="btn btn-ime-apply"
                  type="button"
                  :disabled="!config"
                  @click="applyImePreset(preset.id)"
                >
                  {{ t("快速应用：", "Quick apply: ") }}{{ presetShortcutLabel(preset) }}
                </button>
              </div>
            </article>
          </div>

          <div v-else class="setup-ime-panel" role="tabpanel">
            <article
              v-for="preset in activeImePresets"
              :key="preset.id"
              class="setup-ime-card"
            >
              <header class="setup-ime-head">
                <h4>{{ preset.title }}</h4>
                <span class="setup-ime-tag">{{ preset.tag }}</span>
              </header>
              <ol class="setup-ime-steps">
                <li v-for="(step, idx) in preset.steps" :key="idx">
                  <span class="setup-ime-step-text">{{ step.text }}</span>
                  <span v-if="step.aside" class="setup-ime-step-aside">{{ step.aside }}</span>
                </li>
              </ol>
              <p v-if="preset.quickTip" class="setup-ime-quick-tip">{{ preset.quickTip }}</p>
              <div class="setup-ime-apply">
                <button
                  class="btn btn-ime-apply"
                  type="button"
                  :disabled="!config"
                  @click="applyImePreset(preset.id)"
                >
                  {{ t("快速应用：", "Quick apply: ") }}{{ presetShortcutLabel(preset) }}
                </button>
              </div>
              <figure v-if="isWechatPreset(preset.id)" class="setup-ime-figure">
                <figcaption>{{ t("微信 · 「按住说话」须设为「F5 + 本软件快捷键」（例：F5 + 左 Ctrl + 左 Win）", "WeChat · “Hold to talk” must be set to “F5 + this app’s hotkey” (e.g. F5 + Left Ctrl + Left Win)") }}</figcaption>
                <img
                  :src="wechatImeHotkeysImg"
                  :alt="t('微信输入法按住说话：F5 加本软件设置的快捷键', 'WeChat IME hold to talk: F5 plus the hotkey set in this app')"
                  class="setup-ime-img"
                />
              </figure>
              <figure v-if="isDoubaoHoldPreset(preset.id)" class="setup-ime-figure">
                <figcaption>{{ t("豆包 · 「长按模式」快捷键", "Doubao · “Long-press mode” hotkey") }}</figcaption>
                <img
                  :src="doubaoImeHotkeysImg"
                  :alt="t('豆包输入法长按模式快捷键设置', 'Doubao IME long-press mode hotkey settings')"
                  class="setup-ime-img"
                />
              </figure>
            </article>
          </div>
          </div>
        </div>
      </div>

      <div v-if="showLogModal" class="voice-modal-backdrop" @click.self="showLogModal = false">
        <div class="voice-modal log-modal" role="dialog" aria-modal="true">
          <div class="log-modal-head">
            <h3>{{ t("运行日志", "App log") }}</h3>
            <button
              type="button"
              class="log-clear-link"
              :disabled="logLoading"
              @click="clearLog"
            >
              {{ t("清空日志", "Clear log") }}
            </button>
          </div>
          <p v-if="logPath" class="log-path">{{ logPath }}</p>
          <pre class="log-viewer">{{ logLoading ? t("读取中…", "Loading…") : logText }}</pre>
          <div class="log-modal-actions">
            <button class="btn btn-primary" type="button" :disabled="logLoading" @click="copyLog">
              {{ logCopyHint || t("复制", "Copy") }}
            </button>
            <button class="btn btn-secondary" type="button" @click="openLogExternally">
              {{ isLinux ? t("用文本编辑器打开", "Open in text editor") : t("用记事本打开", "Open in Notepad") }}
            </button>
            <button class="btn btn-secondary" type="button" @click="showLogModal = false">
              {{ t("关闭", "Close") }}
            </button>
          </div>
        </div>
      </div>
      <div
        v-if="showVoiceChoice"
        class="voice-modal-backdrop"
        @click.self="cableDownloadPhase !== 'downloading' && !voiceRepairing && (showVoiceChoice = false)"
      >
        <div class="voice-modal" role="dialog" aria-modal="true">
          <h3>{{ t("修复虚拟声卡", "Repair sound card") }}</h3>
          <div class="voice-repair-tabs" role="tablist" :aria-label="t('修复虚拟声卡菜单', 'Repair sound card menu')">
            <button
              v-for="tab in voiceRepairTabs"
              :key="tab.id"
              type="button"
              class="voice-repair-tab"
              :class="{ active: voiceRepairTab === tab.id }"
              role="tab"
              :aria-selected="voiceRepairTab === tab.id"
              @click="voiceRepairTab = tab.id"
            >
              {{ tab.label }}
            </button>
          </div>

          <div v-show="voiceRepairTab === 'repair'" class="voice-repair-panel" role="tabpanel">
            <p>{{ voiceChoiceMsg || t("请选择检测 / 安装方式：", "Choose how to check / install:") }}</p>
            <p class="voice-modal-reboot-tip">{{ t("新装驱动后点击修复如果未就绪或无声,最好重启系统一次", "If it’s still not ready or silent after a new driver install, restart your PC once") }}</p>

            <div
              v-if="cableDownloadPhase === 'downloading' || cableDownloadPhase === 'error'"
              class="winuhid-download-progress"
              role="status"
              aria-live="polite"
            >
              <div class="winuhid-download-head">
                <span class="winuhid-download-label">
                  {{
                    cableDownloadPhase === "downloading"
                      ? t("正在下载驱动包…", "Downloading driver package…")
                      : t("下载失败", "Download failed")
                  }}
                </span>
                <span
                  v-if="cableDownloadPhase === 'downloading'"
                  class="winuhid-download-meta"
                >
                  {{ cableDownloadProgressLabel }}
                </span>
              </div>
              <div
                v-if="cableDownloadPhase === 'downloading'"
                class="winuhid-download-track"
                :class="{
                  indeterminate: cableDownloadProgress?.percent == null,
                }"
              >
                <div
                  class="winuhid-download-bar"
                  :style="{ width: cableDownloadProgressWidth() }"
                />
              </div>
              <p v-if="cableDownloadMessage" class="winuhid-download-msg">
                {{ tb(cableDownloadMessage) }}
              </p>
            </div>

            <div class="voice-modal-actions">
              <button
                class="btn btn-primary"
                type="button"
                :disabled="voiceRepairing || cableDownloadPhase === 'downloading'"
                @click="chooseVoiceSource('auto')"
              >
                {{ voiceRepairing ? t("处理中…", "Working…") : t("自动修复", "Auto repair") }}
              </button>
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="voiceRepairing || cableDownloadPhase === 'downloading'"
                @click="chooseVoiceSource('embedded')"
              >
                {{ t("使用内置官方VB-CABLE (V2.15.18) 版安装", "Install bundled official VB-CABLE (V2.15.18)") }}
              </button>
              <div class="voice-modal-download-row">
                <button
                  class="btn btn-secondary"
                  type="button"
                  :disabled="voiceRepairing || cableDownloadPhase === 'downloading'"
                  @click="chooseVoiceSource('download_zip')"
                >
                  {{
                    cableDownloadPhase === "downloading"
                      ? t("下载中…", "Downloading…")
                      : t("从官网下载最新VB-CABLE驱动包手动安装", "Download latest VB-CABLE package to install manually")
                  }}
                </button>
                <button
                  v-if="cableDownloadPhase === 'downloading'"
                  class="btn btn-danger"
                  type="button"
                  @click="stopCableZipDownload"
                >
                  {{ t("停止下载", "Stop download") }}
                </button>
              </div>
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="voiceRepairing || cableDownloadPhase === 'downloading'"
                @click="chooseVoiceSource('download_page')"
              >
                {{ t("打开VB-CABLE官网", "Open VB-CABLE website") }}
              </button>
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="cableDownloadPhase === 'downloading'"
                @click="showVoiceChoice = false"
              >
                {{ t("取消", "Cancel") }}
              </button>
            </div>
          </div>

          <div v-show="voiceRepairTab === 'guide'" class="voice-repair-panel" role="tabpanel">
            <p class="voice-modal-uac-tip">
              {{ t("「自动修复」与「使用内置驱动安装」都会打开", "“Auto repair” and “Install bundled driver” both open the ") }}<strong>{{ t("官方 VB-CABLE 安装界面", "official VB-CABLE installer") }}</strong>{{ t("（无黑框）。弹出 Windows 管理员确认（UAC）时请点允许。", " (no console window). When Windows asks for admin approval (UAC), click Yes.") }}
            </p>
            <div class="voice-modal-reboot-followup">
              <p class="voice-modal-reboot-followup-title">{{ t("安装 / 修复怎么做：", "How to install / repair:") }}</p>
              <ol>
                <li>{{ t("在「修复虚拟声卡」窗口的「修复」页点「自动修复」或「使用内置官方 VB-CABLE (V2.15.18) 版安装」", "In the “Repair sound card” window, on the “Repair” tab, click “Auto repair” or “Install bundled official VB-CABLE (V2.15.18)”") }}</li>
                <li>{{ t("按官方安装向导完成安装；新装驱动完成后必须重启电脑", "Finish the official setup wizard; restart your PC after installing a new driver") }}</li>
                <li>{{ t("重启后重新打开本软件；需要时再点「修复虚拟声卡」", "After restarting, reopen this app; click “Repair sound card” again if needed") }}</li>
              </ol>
              <p>
                {{ t("两键都使用软件内嵌的完整官方安装包与安装页面。装完必须重启，虚拟声卡才会真正可用。", "Both buttons use the full official installer bundled with the app. You must restart after installing for the virtual sound card to work.") }}
              </p>
            </div>
            <p class="voice-modal-note">
              {{ t("也可在「修复虚拟声卡」窗口的「修复」页下载官网最新包，或打开 VB-CABLE 官网自行安装。详细排错见「常见问题」。", "You can also download the latest package on the “Repair” tab, or install it from the VB-CABLE website. See “FAQ” for troubleshooting.") }}
            </p>
          </div>

          <div v-show="voiceRepairTab === 'faq'" class="voice-repair-panel voice-repair-faq" role="tabpanel">
            <section class="voice-faq-item">
              <h4>{{ t("一、没有声音 / 输入法听不到遥控器", "1. No sound / the IME can’t hear the remote") }}</h4>
              <p>
                {{ t("多数时候不是遥控器坏了，而是输入法听的不是虚拟声卡那一路。", "Usually the remote is fine — the IME just isn’t listening to the virtual sound card.") }}
              </p>
              <ol>
                <li>{{ t("打开你用的输入法设置（微信 / 豆包 / 千问等），找到「麦克风」或「录音设备」", "Open your IME’s settings (WeChat / Doubao / Qwen etc.) and find “Microphone” or “Recording device”") }}</li>
                <li>{{ t("把它改成 ", "Set it to ") }}<strong>CABLE Output</strong>{{ t("（名字里带 CABLE、Output 即可）", " (any name containing CABLE and Output)") }}</li>
                <li>{{ t("回到本软件，点「修复虚拟声卡」→「修复」页，再点一次「自动修复」，按提示允许管理员权限并完成安装向导", "Back in this app, click “Repair sound card” → “Repair” tab → “Auto repair” again, allow admin access and finish the wizard") }}</li>
                <li>{{ t("若刚装过驱动，先重启电脑再测语音", "If you just installed the driver, restart your PC before testing voice") }}</li>
              </ol>
            </section>
            <section class="voice-faq-item">
              <h4>{{ t("二、系统播放/输出乱了，或听不到电脑声音", "2. System output is wrong, or you can’t hear your PC") }}</h4>
              <p>
                {{ t("VB-CABLE 会在系统里多出几条「声卡」。如果误把扬声器/耳机设成了 CABLE，正常音乐、视频会没声或很怪。", "VB-CABLE adds several “sound cards” to the system. If your speakers/headphones are set to CABLE by mistake, music and video will be silent or sound odd.") }}
              </p>
              <ol>
                <li>{{ t("打开 Windows「声音设置」→「输出」", "Open Windows “Sound settings” → “Output”") }}</li>
                <li>{{ t("先试着在 ", "First try switching between ") }}<strong>CABLE In 16ch</strong>{{ t(" 和 ", " and ") }}<strong>CABLE Input</strong>{{ t(" 之间切换（个别机器显示名略有差别）", " (names may differ slightly on some PCs)") }}</li>
                <li>{{ t("真正听喇叭/耳机时，请把输出改回你的真实音箱或耳机，不要长期停在 CABLE 上", "To listen through speakers/headphones, switch output back to your real device — don’t leave it on CABLE") }}</li>
                <li>{{ t("若怎么切都异常：点「修复虚拟声卡」→「修复」页卸载/重装官方驱动，装完", "If nothing works: click “Repair sound card” → “Repair” tab to uninstall/reinstall the official driver, ") }}<strong>{{ t("重启", "restart") }}</strong>{{ t("，再打开本软件测一次", ", then reopen this app and test again") }}</li>
              </ol>
            </section>
            <section class="voice-faq-item">
              <h4>{{ t("三、设备管理器里有多个 VB-CABLE", "3. Multiple VB-CABLE devices in Device Manager") }}</h4>
              <p>
                {{ t("重复安装或异常修复后，可能出现两个（甚至更多）VB-Audio / VB-CABLE 设备，容易冲突、Failed Start，表现为没声或一直修不好。", "After repeated installs or failed repairs, two or more VB-Audio / VB-CABLE devices may appear. They can conflict (Failed Start), causing silence or repairs that never stick.") }}
              </p>
              <ol>
                <li>{{ t("打开「设备管理器」→ 展开「声音、视频和游戏控制器」（有的在「音频输入和输出」）", "Open “Device Manager” → expand “Sound, video and game controllers” (or “Audio inputs and outputs”)") }}</li>
                <li>{{ t("若看到多个 VB-CABLE / VB-Audio Virtual Cable，对多余或带感叹号的项：右键 → 卸载设备（有「删除驱动程序」可勾上）", "If you see several VB-CABLE / VB-Audio Virtual Cable entries, right-click each extra one (or one with a warning icon) → Uninstall device (tick “Delete the driver” if offered)") }}</li>
                <li>{{ t("回到本软件，点「修复虚拟声卡」→「修复」页，再点「自动修复」或「使用内置官方安装」，走完官方向导", "Back in this app, click “Repair sound card” → “Repair” tab → “Auto repair” or “Install bundled official”, and finish the official wizard") }}</li>
                <li><strong>{{ t("重启电脑", "Restart your PC") }}</strong>{{ t("后再打开本软件，确认状态里虚拟声卡为就绪", ", then reopen this app and check that the virtual sound card shows ready") }}</li>
              </ol>
            </section>
          </div>
        </div>
      </div>

      <div
        v-if="showVoiceReboot"
        class="voice-modal-backdrop"
        @click.self="showVoiceReboot = false"
      >
        <div
          class="voice-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="voice-reboot-title"
        >
          <h3 id="voice-reboot-title">{{ t("需要重启 Windows", "Restart Windows required") }}</h3>
          <p>{{ tb(voiceRebootMsg) || t("驱动已安装，必须重启系统后虚拟声卡才会生效。", "The driver is installed. Restart Windows for the virtual sound card to take effect.") }}</p>
          <p class="voice-modal-reboot-tip">{{ t("安装完成必须重启系统", "You must restart after installation") }}</p>
          <div class="voice-modal-reboot-followup">
            <p class="voice-modal-reboot-followup-title">{{ t("重启后请按下面做一遍：", "After restarting, do the following:") }}</p>
            <ol>
              <li>{{ t("重新打开本软件", "Reopen this app") }}</li>
              <li>{{ t("再点「修复虚拟声卡」→「自动修复」一次", "Click “Repair sound card” → “Auto repair” once more") }}</li>
              <li>{{ t("若弹出 UAC，点允许；成功后默认麦克风会设为 CABLE Output", "If UAC appears, click Yes; on success the default mic is set to CABLE Output") }}</li>
            </ol>
            <p>{{ t("不要只重启、不点「自动修复」，否则端点可能仍未校正。", "Don’t just restart without clicking “Auto repair” — the audio endpoints may stay misconfigured.") }}</p>
          </div>
          <div class="voice-modal-actions">
            <button class="btn btn-primary" type="button" @click="showVoiceReboot = false">
              {{ t("知道了", "Got it") }}
            </button>
          </div>
        </div>
      </div>

      <div
        v-if="showWinuhidChoice"
        class="voice-modal-backdrop"
        @click.self="winuhidDownloadPhase !== 'downloading' && (showWinuhidChoice = false)"
      >
        <div class="voice-modal" role="dialog" aria-modal="true">
          <h3>{{ t("虚拟键盘修复", "Repair virtual keyboard") }}</h3>
          <p>{{ tb(winuhidChoiceMsg) || t("请选择修复或安装方式：", "Choose how to repair or install:") }}</p>
          <p class="voice-modal-uac-tip">{{ t("自动修复会弹出 UAC；导出包请阅读「安装说明.txt」后双击 Run-Install.cmd", "Auto repair shows a UAC prompt; for the exported package, read “安装说明.txt” then double-click Run-Install.cmd") }}</p>
          <p class="voice-modal-reboot-tip">{{ t("仅在 Windows 明确要求时才必须重启；否则再点一次「自动修复」即可", "Restart only if Windows explicitly asks; otherwise just click “Auto repair” again") }}</p>

          <div
            v-if="winuhidDownloadPhase === 'downloading' || winuhidDownloadPhase === 'error'"
            class="winuhid-download-progress"
            role="status"
            aria-live="polite"
          >
            <div class="winuhid-download-head">
              <span class="winuhid-download-label">
                {{
                  winuhidDownloadPhase === "downloading"
                    ? t("正在下载驱动包…", "Downloading driver package…")
                    : t("下载失败", "Download failed")
                }}
              </span>
              <span
                v-if="winuhidDownloadPhase === 'downloading'"
                class="winuhid-download-meta"
              >
                {{ winuhidDownloadProgressLabel }}
              </span>
            </div>
            <div
              v-if="winuhidDownloadPhase === 'downloading'"
              class="winuhid-download-track"
              :class="{
                indeterminate: winuhidDownloadProgress?.percent == null,
              }"
            >
              <div
                class="winuhid-download-bar"
                :style="{ width: winuhidDownloadProgressWidth() }"
              />
            </div>
            <p v-if="winuhidDownloadMessage" class="winuhid-download-msg">
              {{ tb(winuhidDownloadMessage) }}
            </p>
          </div>

          <div class="voice-modal-actions">
            <button
              class="btn btn-primary"
              type="button"
              :disabled="winuhidRepairing || winuhidDownloadPhase === 'downloading'"
              @click="chooseWinuhidSource('embedded')"
            >
              {{ t("自动修复", "Auto repair") }}
            </button>
            <button
              class="btn btn-secondary"
              type="button"
              :disabled="winuhidRepairing || winuhidDownloadPhase === 'downloading'"
              @click="chooseWinuhidSource('embedded_force')"
            >
              {{ t("强制重装（完整走一遍安装）", "Force reinstall (full install)") }}
            </button>
            <button
              class="btn btn-secondary"
              type="button"
              :disabled="winuhidRepairing || winuhidDownloadPhase === 'downloading'"
              @click="chooseWinuhidSource('export')"
            >
              {{ t("导出到桌面手动安装", "Export to desktop to install manually") }}
            </button>
            <div class="voice-modal-download-row">
              <button
                class="btn btn-secondary"
                type="button"
                :disabled="winuhidRepairing || winuhidDownloadPhase === 'downloading'"
                @click="chooseWinuhidSource('download_zip')"
              >
                {{
                  winuhidDownloadPhase === "downloading"
                    ? t("下载中…", "Downloading…")
                    : t("下载驱动包手动安装", "Download package to install manually")
                }}
              </button>
              <button
                v-if="winuhidDownloadPhase === 'downloading'"
                class="btn btn-danger"
                type="button"
                @click="stopWinuhidZipDownload"
              >
                {{ t("停止下载", "Stop download") }}
              </button>
            </div>
            <button
              class="btn btn-secondary"
              type="button"
              :disabled="winuhidRepairing || winuhidDownloadPhase === 'downloading'"
              @click="chooseWinuhidSource('download_page')"
            >
              {{ t("打开 Release 页", "Open Release page") }}
            </button>
            <button
              class="btn btn-secondary"
              type="button"
              :disabled="winuhidDownloadPhase === 'downloading'"
              @click="showWinuhidChoice = false"
            >
              {{ t("取消", "Cancel") }}
            </button>
          </div>
          <p class="voice-modal-note">
            {{ t("导出/下载包内含「安装说明.txt」与 Run-Install.cmd。虚拟键盘已就绪时「自动修复」会跳过；需完整验证请选「强制重装」。未就绪且未要求重启时，再点一次「自动修复」即可。", "Exported/downloaded packages include “安装说明.txt” and Run-Install.cmd. If the virtual keyboard is ready, “Auto repair” skips it; choose “Force reinstall” for a full check. If it’s not ready and no restart was requested, just click “Auto repair” again.") }}
          </p>
        </div>
      </div>

      <LinuxVoicePanel v-if="isLinux" />

      <section v-if="configSectionLoading" class="card mapping-layout mapping-placeholder">
        <h3>{{ t("按键映射", "Key mapping") }}</h3>
        <p class="mapping-placeholder-text">{{ t("正在加载按键映射…", "Loading key mapping…") }}</p>
      </section>

      <section v-else-if="configLoadState === 'error'" class="card mapping-layout mapping-error">
        <h3>{{ t("按键映射", "Key mapping") }}</h3>
        <p class="mapping-error-text">
          {{ t("配置未能加载，按键映射区域无法显示。", "Settings failed to load, so key mapping can’t be shown.") }}
          <span v-if="configLoadError">{{ t("（", " (") }}{{ tb(configLoadError) }}{{ t("）", ")") }}</span>
        </p>
        <button class="btn btn-secondary" type="button" @click="retryLoadConfig">
          {{ t("重试加载", "Retry") }}
        </button>
      </section>

      <section v-else-if="config" class="card mapping-layout">
        <div class="mapping-heading">
          <h3>{{ t("按键映射", "Key mapping") }}</h3>
          <p
            v-if="lastMappingFlash"
            :key="lastMappingFlash.seq"
            class="mapping-flash"
            role="status"
            aria-live="polite"
          >
            <span class="mapping-flash-phase">{{
              lastMappingFlash.phase === "up" ? t("抬起", "Released") : t("按下", "Pressed")
            }}</span>
            <span class="mapping-flash-remote">{{ lastMappingFlash.remote }}</span>
            <template v-if="lastMappingFlash.mapped">
              <span class="mapping-flash-sep" aria-hidden="true">{{ t("：", ":") }}</span>
              <span class="mapping-flash-mapped">{{ lastMappingFlash.mapped }}</span>
            </template>
          </p>
        </div>
        <div class="voice-toolbar" role="group" :aria-label="t('语音听写设置', 'Voice dictation settings')">
          <div class="voice-toolbar-item">
            <span class="voice-toolbar-label">{{ t("点击语音键是否发送映射按键", "Send mapped key on voice press") }}</span>
            <label class="switch" :title="t('点击语音键是否发送映射按键', 'Send mapped key on voice press')">
              <input
                type="checkbox"
                v-model="voiceShortcutEnabled"
                :aria-label="t('点击语音键是否发送映射按键', 'Send mapped key on voice press')"
              />
              <span class="switch-slider" aria-hidden="true"></span>
            </label>
            <button
              ref="voiceInfoBtn"
              type="button"
              class="title-info voice-info"
              :aria-expanded="showVoiceShortcutTip"
              :aria-label="t('语音映射按键说明', 'About voice key mapping')"
              @mouseenter="openVoiceTip"
              @mouseleave="scheduleCloseVoiceTip"
              @focus="openVoiceTip"
              @blur="scheduleCloseVoiceTip"
              @click.stop="toggleVoiceTip"
            >
              <span class="title-info-icon" aria-hidden="true">i</span>
            </button>
            <Teleport to="body">
              <div
                v-if="showVoiceShortcutTip"
                ref="voiceTipEl"
                class="floating-info-tip voice-info-tip"
                role="tooltip"
                :style="voiceTipStyle"
                @mouseenter="openVoiceTip"
                @mouseleave="scheduleCloseVoiceTip"
              >
                <p class="tip-lead">
                  {{ t("只管「按语音键时要不要发映射快捷键」。传声（VB-CABLE）不受此开关影响。", "Only controls whether the voice key sends the mapped hotkey. Audio (VB-CABLE) isn’t affected by this switch.") }}
                </p>
                <div class="tip-block tip-on">
                  <div class="tip-badge">{{ t("开", "On") }}</div>
                  <ul>
                    <li>{{ t("声音送到电脑", "Audio is sent to the PC") }}</li>
                    <li>{{ t("按住语音键时发送你设好的映射快捷键", "Holding the voice key sends your mapped hotkey") }}</li>
                  </ul>
                  <p class="tip-aside">{{ t("适合靠快捷键开/关的语音输入法。", "For voice IMEs toggled by a hotkey.") }}</p>
                </div>
                <div class="tip-block tip-off">
                  <div class="tip-badge">{{ t("关", "Off") }}</div>
                  <ul>
                    <li>{{ t("声音照样送到电脑", "Audio is still sent to the PC") }}</li>
                    <li>{{ t("不发送映射键（日志只记按下/抬起语音键）", "No mapped key is sent (the log only records voice key press/release)") }}</li>
                  </ul>
                  <p class="tip-aside">{{ t("听写需自行打开输入法语音。", "Turn on IME voice input yourself to dictate.") }}</p>
                </div>
              </div>
            </Teleport>
          </div>

          <div v-if="SHOW_VOICE_TRIGGER_MODE" class="voice-toolbar-item">
            <span class="voice-toolbar-label">{{ t("触发模式", "Trigger mode") }}</span>
            <select
              v-model="config.trigger_mode"
              class="form-select voice-toolbar-select"
              @change="persistVoiceSettings"
            >
              <option value="Toggle">{{ t("点击", "Tap") }}</option>
              <option value="Hold">{{ t("按住", "Hold") }}</option>
            </select>
            <button
              ref="triggerInfoBtn"
              type="button"
              class="title-info voice-info"
              :aria-expanded="showTriggerTip"
              :aria-label="t('触发模式说明', 'About trigger mode')"
              @mouseenter="openTriggerTip"
              @mouseleave="scheduleCloseTriggerTip"
              @focus="openTriggerTip"
              @blur="scheduleCloseTriggerTip"
              @click.stop="toggleTriggerTip"
            >
              <span class="title-info-icon" aria-hidden="true">i</span>
            </button>
            <Teleport to="body">
              <div
                v-if="showTriggerTip"
                ref="triggerTipEl"
                class="floating-info-tip voice-info-tip"
                role="tooltip"
                :style="triggerTipStyle"
                @mouseenter="openTriggerTip"
                @mouseleave="scheduleCloseTriggerTip"
              >
                <p class="tip-lead">
                  {{ t("快捷键跟随遥控器实际操作：点一下就点按，按住就按住。", "The hotkey mirrors the remote: tap to tap, hold to hold.") }}
                </p>
                <div class="tip-block tip-on">
                  <div class="tip-badge">{{ t("点击", "Tap") }}</div>
                  <ul>
                    <li>{{ t("短按语音键：点按一次映射快捷键", "Short press: taps the mapped hotkey once") }}</li>
                    <li>{{ t("长按语音键：按住映射快捷键，松手释放", "Long press: holds the mapped hotkey until you let go") }}</li>
                  </ul>
                  <p class="tip-aside">{{ t("适合「点一下开/关」类输入法，也会正确处理长按。", "For tap-to-toggle IMEs; long presses work too.") }}</p>
                </div>
                <div class="tip-block tip-off">
                  <div class="tip-badge">{{ t("按住", "Hold") }}</div>
                  <ul>
                    <li>{{ t("按下语音键：立刻按住映射快捷键并传声", "Press: immediately holds the mapped hotkey and sends audio") }}</li>
                    <li>{{ t("松开语音键：释放快捷键并结束", "Release: releases the hotkey and stops") }}</li>
                  </ul>
                  <p class="tip-aside">{{ t("适合「按住说话」类输入法。", "For hold-to-talk IMEs.") }}</p>
                </div>
              </div>
            </Teleport>
          </div>

          <div class="voice-toolbar-item">
            <span class="voice-toolbar-label">{{ t("增益 (dB)", "Gain (dB)") }}</span>
            <div class="number-stepper" role="group" :aria-label="t('增益分贝', 'Gain in dB')">
              <button
                type="button"
                class="stepper-btn"
                :aria-label="t('减小增益', 'Decrease gain')"
                :disabled="gainDb <= GAIN_MIN || configStore.saving || configSectionLoading"
                @click="stepGain(-GAIN_STEP)"
              >
                −
              </button>
              <input
                type="number"
                class="gain-input"
                v-model.number="gainDb"
                :min="GAIN_MIN"
                :max="GAIN_MAX"
                :step="GAIN_STEP"
                :disabled="configStore.saving || configSectionLoading"
                @blur="clampGainOnBlur"
              />
              <button
                type="button"
                class="stepper-btn"
                :aria-label="t('增大增益', 'Increase gain')"
                :disabled="gainDb >= GAIN_MAX || configStore.saving || configSectionLoading"
                @click="stepGain(GAIN_STEP)"
              >
                +
              </button>
            </div>
            <button
              ref="gainInfoBtn"
              type="button"
              class="title-info voice-info"
              :aria-expanded="showGainTip"
              :aria-label="t('增益说明', 'About gain')"
              @mouseenter="openGainTip"
              @mouseleave="scheduleCloseGainTip"
              @focus="openGainTip"
              @blur="scheduleCloseGainTip"
              @click.stop="toggleGainTip"
            >
              <span class="title-info-icon" aria-hidden="true">i</span>
            </button>
            <Teleport to="body">
              <div
                v-if="showGainTip"
                ref="gainTipEl"
                class="floating-info-tip voice-info-tip"
                role="tooltip"
                :style="gainTipStyle"
                @mouseenter="openGainTip"
                @mouseleave="scheduleCloseGainTip"
              >
                <p class="tip-lead">
                  {{ t("增益 = 把遥控器麦克风声音「放大或缩小」再送进电脑（VB-CABLE）。", "Gain boosts or cuts the remote’s mic audio before it’s sent to the PC (VB-CABLE).") }}
                  {{ t("只影响音量大小，不改变能不能说话。", "It only changes loudness, not whether voice works.") }}
                </p>
                <div class="tip-block tip-on">
                  <div class="tip-badge">{{ t("怎么调", "How to adjust") }}</div>
                  <ul>
                    <li>{{ t("听不清、识别漏字 → 调高（如 10 → 14）", "Hard to hear or missed words → raise it (e.g. 10 → 14)") }}</li>
                    <li>{{ t("破音、刺耳、识别乱 → 调低（如 10 → 6）", "Distorted, harsh or garbled → lower it (e.g. 10 → 6)") }}</li>
                    <li>{{ t("常用默认 ", "Default ") }}<strong>10 dB</strong>{{ t("；范围 -12 ～ 30", "; range -12 to 30") }}</li>
                  </ul>
                </div>
                <div class="tip-block tip-off">
                  <div class="tip-badge">{{ t("注意", "Notes") }}</div>
                  <ul>
                    <li>{{ t("保存后立即生效（约 0.3 秒内自动保存），无需重启桥接", "Takes effect right away (auto-saved within ~0.3 s); no bridge restart needed") }}</li>
                    <li>{{ t("一次加减 2～4 dB 即可，别一次拉满", "Change 2–4 dB at a time; don’t max it out") }}</li>
                  </ul>
                </div>
                <p class="tip-foot">
                  {{ t("简单记：声音太小就加，太吵就减。", "Rule of thumb: too quiet, turn it up; too loud, turn it down.") }}
                </p>
              </div>
            </Teleport>
          </div>
        </div>
        <KeyMappingStage
          :config="config"
          @save="onKeyMappingSave"
        />
      </section>
    </div>
  </div>

  <Teleport to="body">
    <div
      v-if="gainToastVisible"
      class="gain-toast"
      :class="{ 'gain-toast--error': gainToastError }"
      role="status"
    >
      {{ gainToastMessage }}
    </div>
  </Teleport>
</template>

<style scoped>
.page {
  width: 100%;
  max-width: none;
  box-sizing: border-box;
  /* 抵消 main-content 底部 padding 的一半（20 → 有效 10） */
  margin-bottom: -10px;
}
.mapping-layout.card {
  /* 相对 .card 的 10px，下边减半 */
  padding-bottom: 5px;
}
.mapping-heading {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 6px 16px;
  margin-bottom: 8px;
  min-height: 1.4em;
}
.mapping-layout h3 {
  margin: 0;
  flex: 0 0 auto;
}
.mapping-placeholder,
.mapping-error {
  min-height: 160px;
}
.mapping-placeholder-text,
.mapping-error-text {
  margin: 10px 0 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-secondary);
}
.mapping-error-text {
  margin-bottom: 12px;
}
.mapping-flash {
  margin: 0;
  padding: 0;
  font-size: 13px;
  line-height: 1.35;
  color: var(--text-muted, #64748b);
  animation: mapping-flash-in 0.28s ease-out;
}
.mapping-flash-phase {
  margin-right: 6px;
  color: var(--text-muted, #94a3b8);
  font-weight: 500;
}
.mapping-flash-remote {
  color: var(--text, #334155);
  font-weight: 600;
}
.mapping-flash-sep {
  margin: 0 1px;
  color: var(--text-muted, #94a3b8);
}
.mapping-flash-mapped {
  color: var(--accent, #0f766e);
  font-weight: 600;
}
@keyframes mapping-flash-in {
  from {
    opacity: 0;
    transform: translateX(-4px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
.voice-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: stretch;
  gap: 10px;
  margin-bottom: 12px;
  padding: 0;
  border: none;
  background: transparent;
}
.voice-toolbar-item {
  display: inline-flex;
  align-items: center;
  justify-content: flex-start;
  gap: 8px;
  flex: 1 1 auto;
  min-width: max-content;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #fff;
}
.voice-toolbar-label {
  font-size: 13px;
  line-height: 1.3;
  font-weight: 400;
  color: var(--text);
  white-space: nowrap;
}
.voice-toolbar-select {
  min-width: 72px;
  padding: 4px 8px;
}
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}
.title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.page-header h2 { font-size: 20px; font-weight: 600; margin: 0; }
.title-info {
  position: relative;
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  padding: 0;
  border: 1.5px solid #94a3b8;
  border-radius: 50%;
  background: transparent;
  color: #64748b;
  cursor: help;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.title-info:hover,
.title-info:focus-visible {
  border-color: #2563eb;
  color: #2563eb;
  outline: none;
}
.title-info-icon {
  font-size: 11px;
  font-weight: 700;
  font-style: italic;
  font-family: Georgia, "Times New Roman", serif;
  line-height: 1;
}

.switch {
  position: relative;
  display: inline-block;
  width: 40px;
  height: 22px;
  flex-shrink: 0;
}
.switch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}
.switch-slider {
  position: absolute;
  inset: 0;
  border-radius: 999px;
  background: #cbd5e1;
  cursor: pointer;
  transition: background 0.15s ease;
}
.switch-slider::before {
  content: "";
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.2);
  transition: transform 0.15s ease;
}
.switch input:checked + .switch-slider {
  background: var(--primary, #2563eb);
}
.switch input:checked + .switch-slider::before {
  transform: translateX(18px);
}
.switch input:focus-visible + .switch-slider {
  outline: 2px solid rgba(37, 99, 235, 0.35);
  outline-offset: 2px;
}

.device-info-row {
  display: flex;
  gap: 16px;
  margin-bottom: 0;
  padding: 12px 14px;
  background: var(--card-bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  align-items: stretch;
}
.device-info-col {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 8px;
  min-width: 0;
  flex: 1 1 0;
}
.info-line {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.info-line .info-label {
  flex-shrink: 0;
}
.info-line .info-value {
  min-width: 0;
  font-size: 12px;
  font-weight: 400;
  color: var(--text, #1e293b);
}
.info-value-battery {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
@media (max-width: 720px) {
  .device-info-row {
    flex-direction: column;
  }
  .info-item-audio,
  .info-item-cable-vol {
    width: 100%;
  }
}

.overview-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 280px;
  gap: 12px;
  align-items: stretch;
  margin-bottom: 16px;
}
.overview-left {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
/* 仅由左侧撑高；日志绝对铺满同高 */
.log-aside {
  position: relative;
  min-height: 0;
}
.log-card {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  min-width: 0;
  width: auto;
  max-width: none;
  padding: 5px;
  overflow: hidden;
  box-sizing: border-box;
}
.log-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  margin-bottom: 4px;
}
.log-card-head .card-text {
  margin: 0;
}
.log-card h3 {
  margin: 0 0 6px;
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 400;
  color: var(--text-secondary);
}
@media (max-width: 840px) {
  .overview-row {
    grid-template-columns: 1fr;
  }
  .log-aside {
    position: static;
    height: 180px;
  }
  .log-card {
    position: relative;
    inset: auto;
    height: 100%;
  }
}

.page-body { display: flex; flex-direction: column; gap: 16px; }

.card {
  background: var(--card-bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px;
}

.card-text {
  font-size: 12px;
  
  margin-bottom: 8px;
  color: var(--text);
}

.host-card {
  padding: 16px 18px;
}
.host-status-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  align-items: stretch;
  margin: 0 0 12px;
}
.host-status-item {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 10px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #fff;
}
.host-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: #94a3b8;
}
.host-dot.ok {
  background: var(--success, #22c55e);
}
.host-dot.warn {
  background: var(--warning, #f59e0b);
}
.host-dot.error {
  background: var(--danger, #ef4444);
}
.host-item-label {
  font-size: 13px;
  font-weight: 400;
  color: var(--text);
  white-space: nowrap;
  flex-shrink: 0;
}
.host-item-state {
  margin-left: auto;
  padding-left: 4px;
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  white-space: nowrap;
}
.host-item-state.ok {
  color: #15803d;
}
.host-item-state.warn {
  color: #b45309;
}
.host-item-state.error {
  color: #b91c1c;
}
@media (min-width: 841px) and (max-width: 980px) {
  /* 侧栏日志并排时内容区偏窄，改为 2×2 避免块内文字挤出 */
  .host-status-row {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
.host-detail {
  margin: 0 0 14px;
  font-size: 13px;
  color: #555;
  line-height: 1.5;
}
.host-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.host-action-group {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 1 auto;
}
.host-actions .btn {
  padding: 4px 10px;
  font-size: 12px;
  font-weight: normal;
  border-radius: 5px;
}
.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  font-weight: normal;
  cursor: pointer;
  transition: all 0.15s ease;
}
.btn-tiny {
  padding: 2px 8px;
  font-size: 11px;
  font-weight: 500;
  line-height: 1.3;
  border-radius: 4px;
  flex-shrink: 0;
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
.btn-danger {
  background: #dc2626;
  color: #fff;
  border: 1px solid #b91c1c;
}
.btn-danger:hover:not(:disabled) {
  background: #b91c1c;
}
.voice-modal-download-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.voice-modal-download-row .btn {
  flex: 1 1 auto;
}
.voice-modal-download-row .btn-danger {
  flex: 0 0 auto;
}

.voice-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1000;
  background: rgba(15, 23, 42, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}
.voice-modal {
  width: min(440px, 100%);
  background: var(--card-bg, #fff);
  border: 1px solid var(--border);
  border-radius: var(--radius, 8px);
  padding: 20px 22px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
}
.voice-modal h3 {
  margin: 0 0 10px;
  font-size: 16px;
}
.voice-modal p {
  margin: 0 0 16px;
  font-size: 13px;
  color: #555;
  line-height: 1.5;
}
.voice-modal-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.winuhid-download-progress {
  margin-bottom: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border);
}
.winuhid-download-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.winuhid-download-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.winuhid-download-meta {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.winuhid-download-track {
  height: 8px;
  border-radius: 999px;
  background: #e2e8f0;
  overflow: hidden;
}
.winuhid-download-track.indeterminate .winuhid-download-bar {
  width: 35% !important;
  animation: winuhid-progress-indeterminate 1.2s ease-in-out infinite;
}
.winuhid-download-bar {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #3b82f6, #1d4ed8);
  transition: width 0.15s ease;
}
.winuhid-download-msg {
  margin: 8px 0 0;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary);
  word-break: break-all;
}
@keyframes winuhid-progress-indeterminate {
  0% {
    transform: translateX(-120%);
  }
  100% {
    transform: translateX(320%);
  }
}
.voice-modal-uac-tip {
  margin: -8px 0 8px !important;
  font-size: 13px !important;
  font-weight: 600;
  color: #ea580c !important;
  line-height: 1.45;
}
.voice-modal-reboot-tip {
  margin: 0 0 8px !important;
  font-size: 14px !important;
   
  color: #dc2626 !important;
  text-align: center;
  line-height: 1.45;
}
.voice-modal-reboot-followup {
  margin: 0 0 16px;
  padding: 10px 12px;
  border-radius: 8px;
  background: #fef2f2;
  border: 1px solid #fecaca;
  font-size: 12px;
  color: #7f1d1d;
  line-height: 1.5;
  text-align: left;
}
.voice-modal-reboot-followup-title {
  margin: 0 0 6px !important;
  font-size: 12px !important;
  font-weight: 700;
  color: #991b1b !important;
}
.voice-modal-reboot-followup ol {
  margin: 0 0 8px;
  padding-left: 1.25em;
}
.voice-modal-reboot-followup li {
  margin: 0 0 2px;
}
.voice-modal-reboot-followup p {
  margin: 0 !important;
  font-size: 12px !important;
  color: #7f1d1d !important;
}
.voice-modal-note {
  margin-top: 14px !important;
  margin-bottom: 0 !important;
  font-size: 12px !important;
  color: #777 !important;
}
.voice-repair-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0 0 12px;
  padding: 0 0 10px;
  border-bottom: 1px solid var(--border);
}
.voice-repair-tab {
  padding: 6px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #f8fafc;
  color: #475569;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition:
    background 0.15s,
    border-color 0.15s,
    color 0.15s;
}
.voice-repair-tab:hover {
  background: #f1f5f9;
  border-color: #cbd5e1;
}
.voice-repair-tab.active {
  background: #eff6ff;
  border-color: #93c5fd;
  color: #1d4ed8;
}
.voice-repair-panel {
  min-height: 80px;
}
.voice-repair-faq {
  max-height: min(52vh, 420px);
  overflow-y: auto;
  padding-right: 4px;
}
.voice-faq-item + .voice-faq-item {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
}
.voice-faq-item h4 {
  margin: 0 0 6px;
  font-size: 14px;
  font-weight: 600;
  color: #0f172a;
}
.voice-faq-item p {
  margin: 0 0 8px !important;
  font-size: 13px !important;
  color: #475569 !important;
  line-height: 1.55;
}
.voice-faq-item ol {
  margin: 0;
  padding-left: 1.25em;
  font-size: 13px;
  color: #334155;
  line-height: 1.55;
}
.voice-faq-item li + li {
  margin-top: 4px;
}

.log-modal {
  width: min(720px, 100%);
  max-height: min(80vh, 720px);
  display: flex;
  flex-direction: column;
}
.log-modal-head {
  display: flex;
  align-items: baseline;
  gap: 14px;
  flex-wrap: wrap;
  margin-bottom: 4px;
}
.log-modal-head h3 {
  margin: 0;
}
.log-clear-link {
  padding: 0;
  border: none;
  background: transparent;
  color: #2563eb;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
}
.log-clear-link:hover:not(:disabled) {
  color: #1d4ed8;
  text-decoration: underline;
}
.log-clear-link:disabled {
  color: #94a3b8;
  cursor: default;
}

.setup-tips-modal {
  width: min(560px, 100%);
  max-height: min(72vh, 560px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 16px 18px 18px;
}
.setup-tips-body {
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  padding-right: 2px;
}
.setup-tips-body::-webkit-scrollbar {
  width: 8px;
}
.setup-tips-body::-webkit-scrollbar-track {
  background: #f1f5f9;
  border-radius: 4px;
}
.setup-tips-body::-webkit-scrollbar-thumb {
  background: #94a3b8;
  border-radius: 4px;
}
.setup-tips-body::-webkit-scrollbar-thumb:hover {
  background: #64748b;
}
.setup-tips-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin: 0;
  padding: 0 0 8px;
  flex-shrink: 0;
  z-index: 2;
  background: var(--card-bg, #fff);
}
.setup-tips-head h3 {
  margin: 0;
}
.setup-tips-head .btn {
  padding: 4px 10px;
  font-size: 12px;
}
.setup-ime-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  flex-shrink: 0;
  margin: 0 0 10px;
  padding: 8px 0 10px;
  border-bottom: 1px solid var(--border);
  background: var(--card-bg, #fff);
}
.setup-ime-tab {
  padding: 6px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #f8fafc;
  color: #475569;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition:
    background 0.15s,
    border-color 0.15s,
    color 0.15s;
}
.setup-ime-tab:hover {
  background: #f1f5f9;
  border-color: #cbd5e1;
}
.setup-ime-tab.active {
  background: #eff6ff;
  border-color: #93c5fd;
  color: #1d4ed8;
}
.setup-ime-panel {
  min-height: 120px;
}
.setup-faq-section + .setup-faq-section {
  margin-top: 14px;
}
.setup-faq-section-title {
  margin: 0 0 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}
.setup-faq-list {
  margin-bottom: 0 !important;
}
.setup-ime-faq-warn {
  margin-bottom: 14px;
}
.setup-ime-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 14px 14px;
  background: #fafbfc;
}
.setup-ime-card + .setup-ime-card {
  margin-top: 12px;
}
.setup-ime-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}
.setup-ime-head h4 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}
.setup-ime-tag {
  flex-shrink: 0;
  font-size: 11px;
  color: #2563eb;
  background: #eff6ff;
  border: 1px solid #bfdbfe;
  border-radius: 4px;
  padding: 2px 6px;
}
.setup-ime-warn {
  margin: 0 0 12px;
  padding: 10px 12px;
  border: 1px solid #fecaca;
  border-radius: 6px;
  background: #fef2f2;
}
.setup-ime-warn-title {
  margin: 0 0 8px !important;
  font-size: 13px !important;
  font-weight: 600 !important;
  color: #dc2626 !important;
  line-height: 1.5 !important;
}
.setup-ime-warn-sub {
  margin: 0 0 4px !important;
  font-size: 12px !important;
  font-weight: 600 !important;
  color: #b91c1c !important;
}
.setup-ime-warn-ways {
  margin: 0;
  padding-left: 1.25em;
  font-size: 12px;
  line-height: 1.55;
  color: #b91c1c;
}
.setup-ime-warn-ways li + li {
  margin-top: 4px;
}
.setup-ime-steps {
  margin: 0 0 12px;
  padding-left: 1.25em;
  font-size: 13px;
  line-height: 1.55;
  color: #334155;
}
.setup-ime-steps li + li {
  margin-top: 4px;
}
.setup-ime-step-text {
  display: block;
}
.setup-ime-step-aside {
  display: block;
  margin-top: 4px;
  font-size: 12px;
  line-height: 1.5;
  color: #64748b;
  font-weight: 400;
}
.setup-ime-quick-tip {
  margin: 10px 0 16px !important;
  padding: 0;
  font-size: 13px;
  line-height: 1.55;
  color: #2563eb !important;
  font-weight: 600;
}
.setup-ime-steps code {
  font-size: 12px;
  padding: 1px 5px;
  border-radius: 3px;
  background: #e2e8f0;
  color: #0f172a;
}
.setup-ime-apply {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 10px;
  margin-bottom: 12px;
}
.setup-ime-apply .btn {
  padding: 6px 14px;
  font-size: 13px;
  font-weight: 600;
}
/* 马卡龙绿：白字仍清晰可读 */
.btn-ime-apply {
  background: #4db88a;
  color: #fff;
  border: 1px solid #3ea578;
}
.btn-ime-apply:hover:not(:disabled) {
  background: #3ea578;
  border-color: #35956b;
}
.btn-ime-apply:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.setup-ime-apply-row {
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  margin-bottom: 0;
}
.setup-ime-apply-row .btn {
  width: 100%;
  text-align: center;
  justify-content: center;
}
.setup-apply-hint {
  font-size: 12px;
  color: #16a34a;
}
.setup-apply-hint-global {
  margin: 0 0 10px;
}
.setup-ime-figure {
  margin: 0;
}
.setup-ime-img {
  display: block;
  width: 100%;
  max-width: 100%;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: #fff;
}
.setup-ime-figure figcaption {
  margin-top: 6px;
  font-size: 11px;
  color: #94a3b8;
  text-align: center;
}
.log-path {
  margin: 0 0 8px !important;
  font-size: 11px !important;
  color: #888 !important;
  word-break: break-all;
}
.log-viewer {
  flex: 1;
  min-height: 240px;
  max-height: 48vh;
  margin: 0 0 14px;
  padding: 10px 12px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #0f172a;
  color: #e2e8f0;
  font-size: 12px;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: ui-monospace, Consolas, "Courier New", monospace;
}
.log-modal-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.info-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.info-item-audio .audio-label-row {
  min-height: 18px;
  align-items: baseline;
}

.info-item-audio .ble-wave,
.info-item-cable-vol .cable-vol-ruler {
  flex-shrink: 0;
  height: 28px;
}

.info-item-audio,
.info-item-cable-vol {
  gap: 3px;
  flex: 1.1 1 0;
  min-width: 120px;
}

.cable-vol-label-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 3em;
  align-items: center;
  column-gap: 8px;
  height: 18px;
  line-height: 18px;
}

.cable-vol-label-row .info-label,
.cable-vol-label-row .cable-vol-fail {
  line-height: 18px;
}

.cable-vol-fail {
  font-size: 12px;
  font-weight: 400;
  color: var(--danger, #ef4444);
  white-space: nowrap;
}

.cable-vol-label-row .cable-vol-state {
  justify-self: end;
  width: 3em;
  min-width: 3em;
  text-align: right;
  white-space: nowrap;
  font-weight: 400;
  font-size: 12px;
  line-height: 18px;
  font-variant-numeric: tabular-nums;
}

.cable-vol-label-row .cable-vol-state.is-idle {
  color: var(--text-secondary);
}

.cable-vol-label-row .cable-vol-state.is-sending {
  color: #15803d;
}

.info-item-cable-vol .cable-vol-state.is-ok {
  color: #15803d;
}

.info-item-cable-vol .cable-vol-state.is-low {
  color: #ca8a04;
}

.info-item-cable-vol .cable-vol-state.is-high {
  color: #dc2626;
}

.audio-label-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.audio-label-row .info-label {
  flex-shrink: 0;
}
.audio-state {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  line-height: 1.2;
  white-space: nowrap;
}
.audio-atvv-fail {
  font-size: 12px;
  font-weight: 600;
  color: var(--danger, #ef4444);
  line-height: 1.2;
  white-space: nowrap;
}
.info-item-audio.is-session .audio-state {
  color: #b45309;
}
.info-item-audio.is-receiving .audio-state {
  color: #15803d;
}
.ble-wave {
  height: 28px;
  padding: 0;
  border-radius: 4px;
  background: #f1f5f9;
  border: 1px solid var(--border);
  color: #94a3b8;
  overflow: hidden;
}
.ble-wave-svg {
  display: block;
  width: 100%;
  height: 100%;
}
.ble-wave-fill {
  fill: currentColor;
  opacity: 0.22;
  transition: d 60ms linear;
}
.ble-wave-line {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
  transition: points 60ms linear;
}
.info-item-audio.is-receiving .ble-wave {
  background: #ecfdf5;
  border-color: #bbf7d0;
  color: #16a34a;
}
.info-item-audio.is-session .ble-wave {
  background: #fffbeb;
  border-color: #fde68a;
  color: #d97706;
}

.info-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.info-value {
  font-size: 12px;
  font-weight: 400;
  color: var(--text, #1e293b);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.form-select {
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 13px;
  background: var(--card-bg);
}

.number-stepper {
  display: inline-flex;
  align-items: stretch;
  border: 1px solid var(--border);
  border-radius: 4px;
  overflow: hidden;
  background: var(--card-bg);
}

.stepper-btn {
  width: 30px;
  padding: 0;
  border: none;
  background: #f1f5f9;
  color: var(--text);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  user-select: none;
}

.stepper-btn:hover:not(:disabled) {
  background: #e2e8f0;
}

.stepper-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.stepper-btn + .gain-input,
.gain-input + .stepper-btn {
  border-left: 1px solid var(--border);
}

.gain-input {
  width: 56px;
  padding: 6px 4px;
  border: none;
  border-radius: 0;
  font-size: 13px;
  text-align: center;
  background: transparent;
  font-variant-numeric: tabular-nums;
  -moz-appearance: textfield;
  appearance: textfield;
}

.gain-input::-webkit-outer-spin-button,
.gain-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.gain-input:focus {
  outline: none;
  background: #f8fafc;
}

.log-area {
  background: #f1f5f9;
  border-radius: 4px;
  padding: 6px 10px;
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  line-height: 1.45;
}

.log-entry {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  color: var(--text);
  margin: 0 0 4px;
  white-space: normal;
}

.log-time {
  color: var(--text-secondary);
  flex-shrink: 0;
}

.log-text {
  min-width: 0;
  flex: 1;
  overflow-wrap: anywhere;
  word-break: break-word;
  white-space: pre-wrap;
}
</style>

<style>
/* Teleport 到 body：不用 scoped，避免样式丢失 */
.floating-info-tip {
  box-sizing: border-box;
  width: min(420px, calc(100vw - 16px));
  padding: 10px 12px;
  border-radius: 8px;
  background: #0f172a;
  color: #f8fafc;
  font-size: 12px;
  font-weight: 400;
  line-height: 1.55;
  text-align: left;
  box-shadow: 0 8px 24px rgba(15, 23, 42, 0.28);
  white-space: normal;
  pointer-events: auto;
}
.floating-info-tip.voice-info-tip {
  width: min(360px, calc(100vw - 16px));
  padding: 12px 14px;
}
.floating-info-tip .tip-lead {
  margin: 0 0 10px;
  color: #e2e8f0;
  line-height: 1.55;
}
.floating-info-tip .tip-block {
  margin: 0 0 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.06);
}
.floating-info-tip .tip-badge {
  display: inline-block;
  margin-bottom: 6px;
  padding: 1px 7px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.02em;
}
.floating-info-tip .tip-on .tip-badge {
  background: rgba(34, 197, 94, 0.22);
  color: #86efac;
}
.floating-info-tip .tip-off .tip-badge {
  background: rgba(148, 163, 184, 0.22);
  color: #cbd5e1;
}
.floating-info-tip ul {
  margin: 0;
  padding-left: 1.1em;
  color: #f1f5f9;
}
.floating-info-tip li {
  margin: 2px 0;
}
.floating-info-tip .tip-aside {
  margin: 6px 0 0;
  color: #94a3b8;
  font-size: 11px;
  line-height: 1.45;
}
.floating-info-tip .tip-foot {
  margin: 10px 0 0;
  padding-top: 8px;
  border-top: 1px solid rgba(148, 163, 184, 0.28);
  color: #94a3b8;
  font-size: 11px;
  line-height: 1.5;
}

.gain-toast {
  position: fixed;
  left: 50%;
  top: 60px;
  transform: translateX(-50%);
  z-index: 4000;
  padding: 10px 18px;
  border-radius: 8px;
  background: rgba(15, 23, 42, 0.92);
  color: #fff;
  font-size: 13px;
  font-weight: 500;
  box-shadow: 0 8px 24px rgba(15, 23, 42, 0.25);
  pointer-events: none;
  animation: gain-toast-in 0.2s ease-out;
}

.gain-toast--error {
  background: rgba(127, 29, 29, 0.94);
}

@keyframes gain-toast-in {
  from {
    opacity: 0;
    transform: translateX(-50%) translateY(-8px);
  }
  to {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
}
</style>
