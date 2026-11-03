// 个性化偏好的应用与持久化。
//
// 外观 / 字体 / 缩放全部落在 html 根元素的 data 属性与 CSS 变量上，
// tokens.css 据此换肤；本模块不关心设置从哪来（localStorage 或后端），
// 只负责把一份设置同步到 DOM。
//
// 首帧前同步应用一次（localStorage），避免启动时外观闪变；
// 设置页变更与 refreshAll 的权威数据到达后再各应用一次。

import type { AppSettings } from "./types";

const UI_PREFS_KEY = "synbackup.ui-prefs";

/** 只持久化影响"首帧观感"的字段，避免整份设置重复落盘 */
const UI_PREF_FIELDS = [
  "appearance",
  "accent",
  "fontFamily",
  "fontScale",
  "uiScale",
  "launchAnimation",
] as const;

export function loadUiPrefs(): Partial<AppSettings> {
  try {
    const raw = localStorage.getItem(UI_PREFS_KEY);
    if (!raw) return {};
    const obj = JSON.parse(raw) as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const k of UI_PREF_FIELDS) if (k in obj) out[k] = obj[k];
    return out as Partial<AppSettings>;
  } catch {
    return {};
  }
}

export function persistUiPrefs(s: AppSettings) {
  try {
    const out: Record<string, unknown> = {};
    for (const k of UI_PREF_FIELDS) out[k] = s[k];
    localStorage.setItem(UI_PREFS_KEY, JSON.stringify(out));
  } catch {
    /* localStorage 不可用时放弃持久化，本次会话内设置仍然生效 */
  }
}

/** 把外观 / 字体 / 缩放 / 启动动画同步到根元素 */
export function applyUiPrefs(s: AppSettings) {
  const root = document.documentElement;

  if (s.appearance === "light" || s.appearance === "dark") {
    root.dataset.appearance = s.appearance;
  } else {
    delete root.dataset.appearance; // follow：交给 prefers-color-scheme
  }

  if (s.accent && s.accent !== "brand") root.dataset.accent = s.accent;
  else delete root.dataset.accent;

  if (s.fontFamily && s.fontFamily !== "system") root.dataset.font = s.fontFamily;
  else delete root.dataset.font;

  root.style.setProperty("--font-scale", String(s.fontScale));
  root.style.setProperty("--ui-scale", String(s.uiScale));

  if (s.launchAnimation) root.dataset.bootAnim = "on";
  else delete root.dataset.bootAnim;
}
