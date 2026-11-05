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

/** 品牌默认强调色（与 tokens.css 的 --accent-brand 预设一致） */
export const DEFAULT_ACCENT = "#9b62cf";

/** 只持久化影响"首帧观感"的字段，避免整份设置重复落盘 */
const UI_PREF_FIELDS = [
  "appearance",
  "accent",
  "fontFamily",
  "fontScale",
  "uiScale",
  "launchAnimation",
  "launchAnimPreset",
  "launchAnimMs",
  "launchAnimCurve",
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

/** 按强调色亮度决定其上的文字颜色：高亮度配深字，其余配白字 */
function onAccentColor(accent: string): string {
  const m = /^#([0-9a-f]{6})$/i.exec(accent.trim());
  if (!m) return "#ffffff";
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  // YIQ 感知亮度
  return (r * 299 + g * 587 + b * 114) / 1000 > 150 ? "#1d1a22" : "#ffffff";
}

/** 校验 #RRGGBB */
export function isValidHex(v: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(v.trim());
}

/**
 * 从自定义强调色派生整套强调色变量：
 * 实色 --accent / 悬停加深 --accent-hover / 虚色 --accent-2（同色相提亮）。
 * 派生规则与 tokens.css 预设同构（预设是静态三值，这里动态计算）。
 */
function injectCustomAccent(root: HTMLElement, hex: string) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  const mix = (c: number, target: number, k: number) =>
    Math.round(c + (target - c) * k);
  // 悬停：向黑混合 10%（与预设 #9b62cf → #8a4fc0 的比例一致）
  const hover = `#${[mix(r, 0, 0.1), mix(g, 0, 0.1), mix(b, 0, 0.1)]
    .map((c) => c.toString(16).padStart(2, "0"))
    .join("")}`;
  // 虚色：向白混合 32%（与 brand 的 #9b62cf → #be93e3 相当）
  const soft = `#${[mix(r, 255, 0.32), mix(g, 255, 0.32), mix(b, 255, 0.32)]
    .map((c) => c.toString(16).padStart(2, "0"))
    .join("")}`;
  root.style.setProperty("--accent", hex);
  root.style.setProperty("--accent-hover", hover);
  root.style.setProperty("--accent-2", soft);
}

/** 把语言 / 外观 / 字体 / 缩放 / 启动动画同步到根元素 */
export function applyUiPrefs(s: AppSettings) {
  const root = document.documentElement;
  root.lang = s.language === "en-US" ? "en" : "zh-CN";

  if (s.appearance === "light" || s.appearance === "dark") {
    root.dataset.appearance = s.appearance;
  } else {
    delete root.dataset.appearance; // follow：交给 prefers-color-scheme
  }

  // 自定义强调色 = 运行时变量覆盖（优先于 data-accent 预设）；否则清掉覆盖
  if (s.accent === "custom" && isValidHex(s.accentCustom)) {
    root.dataset.accent = "";
    injectCustomAccent(root, s.accentCustom);
  } else {
    root.style.removeProperty("--accent");
    root.style.removeProperty("--accent-hover");
    root.style.removeProperty("--accent-2");
    if (s.accent && s.accent !== "brand") root.dataset.accent = s.accent;
    else delete root.dataset.accent;
  }

  if (s.fontFamily && s.fontFamily !== "system") root.dataset.font = s.fontFamily;
  else delete root.dataset.font;

  root.style.setProperty("--font-scale", String(s.fontScale));
  root.style.setProperty("--ui-scale", String(s.uiScale));

  // 强调色就位后再读实际值，计算其上的自适应文字色
  const accent = getComputedStyle(root).getPropertyValue("--accent");
  root.style.setProperty("--on-accent", onAccentColor(accent));

  root.dataset.bootAnim = s.launchAnimation ? s.launchAnimPreset : "off";
  root.style.setProperty("--boot-ms", String(s.launchAnimMs));
  root.style.setProperty("--boot-ease", bootCurve(s.launchAnimCurve));
}

/** 启动动画曲线映射（CSS cubic-bezier） */
function bootCurve(key: string): string {
  switch (key) {
    case "spring": return "cubic-bezier(0.34, 1.4, 0.64, 1)"; // 轻微过冲
    case "linear": return "linear";
    case "swift": return "cubic-bezier(0.32, 0.72, 0, 1)";    // iOS sheet 减速
    default: return "cubic-bezier(0.2, 0.8, 0.2, 1)";         // smooth（默认）
  }
}
