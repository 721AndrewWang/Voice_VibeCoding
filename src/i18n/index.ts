import { computed, ref } from "vue";
import { translateBackend } from "./backend";

/** 界面语言：英文优先，可切到中文。只存在本机 WebView 的 localStorage，不进后端配置 */
export type Lang = "en" | "zh";

const STORAGE_KEY = "vvc.ui-lang";

function initialLang(): Lang {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "en" || v === "zh") return v;
  } catch {
    // localStorage 不可用：用默认
  }
  return "en";
}

export const lang = ref<Lang>(initialLang());
export const isEn = computed(() => lang.value === "en");

function applyDocumentLang(l: Lang) {
  if (typeof document === "undefined") return; // vitest 的 node 环境
  document.documentElement.lang = l === "en" ? "en" : "zh-CN";
}
applyDocumentLang(lang.value);

export function setLang(l: Lang) {
  lang.value = l;
  applyDocumentLang(l);
  try {
    localStorage.setItem(STORAGE_KEY, l);
  } catch {
    // 忽略：本次会话内仍生效
  }
}

export function toggleLang() {
  setLang(lang.value === "en" ? "zh" : "en");
}

/**
 * 中英成对写在调用处：t("已连接", "Connected")。
 * 读 lang.value，所以在模板和 computed 里会随语言切换自动刷新。
 */
export function t(zh: string, en: string): string {
  return lang.value === "en" ? en : zh;
}

/** 后端（Rust）发来的中文状态 / 日志文字：英文模式下按词典和规则翻译，认不出的原样显示 */
export function tb(text: string | null | undefined): string {
  if (!text) return text ?? "";
  return lang.value === "en" ? translateBackend(text) : text;
}
