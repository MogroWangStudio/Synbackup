// 个性化偏好的应用与持久化。
//
// 外观 / 字体 / 缩放全部落在 html 根元素的 data 属性与 CSS 变量上，
// tokens.css 据此换肤；本模块不关心设置从哪来（localStorage 或后端），
// 只负责把一份设置同步到 DOM。
//
// 首帧前同步应用一次（localStorage），避免启动时外观闪变；
// 设置页变更与 refreshAll 的权威数据到达后再各应用一次。

import type { AppSettings } from "./types";
// 只引零依赖的语言注册表（./i18n/index 会带上 store，形成 store→theme→i18n→store 循环）
import { localeTag } from "./i18n/locales";

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
  "logoStyle",
  "logoCustomText",
  "logoImagePath",
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

/** 特殊主题集合（与 tokens.css 的 data-theme 块一一对应；standard = 默认无块） */
export const UI_THEMES = ["standard", "paper", "terminal", "glass"] as const;

/** 同步任务卡片按钮显示模式集合（both = 默认无块） */
const TASK_BTN_STYLES = ["both", "icon", "text"];
/** 界面字体预设集合（system = 默认跟随主题；其余为自定义字族名） */
const FONT_PRESETS = ["system", "serif", "rounded"];
/** 自定义字族的回退栈（与 tokens.css 的默认系统栈保持一致） */
const FONT_FALLBACK =
  '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro SC", "PingFang SC", ' +
  '"HarmonyOS Sans SC", "MiSans", "Microsoft YaHei UI", "Hiragino Kaku Gothic ProN", ' +
  '"Yu Gothic UI", "Meiryo", "Apple SD Gothic Neo", "Malgun Gothic", "Segoe UI", sans-serif';

/** 把语言 / 外观 / 字体 / 缩放 / 启动动画同步到根元素 */
export function applyUiPrefs(s: AppSettings) {
  const root = document.documentElement;
  // 真实 BCP-47 传入：浏览器据此选择正确的字形变体（日文汉字 / 繁简字形）与断行规则
  root.lang = localeTag(s.language);

  if (s.appearance === "light" || s.appearance === "dark") {
    root.dataset.appearance = s.appearance;
  } else {
    delete root.dataset.appearance; // follow：交给 prefers-color-scheme
  }

  // 特殊主题：未知值（旧配置 / 手改）一律退回默认
  if (s.theme && (UI_THEMES as readonly string[]).includes(s.theme) && s.theme !== "standard") {
    root.dataset.theme = s.theme;
  } else {
    delete root.dataset.theme;
  }

  // 任务卡片按钮显示模式：未知值退回默认（both，无块）
  if (s.taskButtonStyle && TASK_BTN_STYLES.includes(s.taskButtonStyle) && s.taskButtonStyle !== "both") {
    root.dataset.taskBtn = s.taskButtonStyle;
  } else {
    delete root.dataset.taskBtn;
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

  // 界面字体：预设（data-font 规则）与自定义字族（内联 --font，含既有回退栈）分流
  if (s.fontFamily && !FONT_PRESETS.includes(s.fontFamily)) {
    // 自定义字族：CSS font-family 字符串中的引号需转义
    const family = s.fontFamily.replace(/"/g, '\\"');
    root.style.setProperty("--font", `"${family}", ${FONT_FALLBACK}`);
    delete root.dataset.font;
  } else {
    delete root.dataset.font;
    root.style.removeProperty("--font");
  }

  root.style.setProperty("--font-scale", String(s.fontScale));
  root.style.setProperty("--ui-scale", String(s.uiScale));

  // 强调色就位后再读实际值，计算其上的自适应文字色
  const accent = getComputedStyle(root).getPropertyValue("--accent");
  root.style.setProperty("--on-accent", onAccentColor(accent));

  root.dataset.bootAnim = s.launchAnimation ? s.launchAnimPreset : "off";
  // 启动幕布的开关与 bootAnim 分开：改预设 / 改时长只影响"下次启动怎么演"，
  // 不该让已经演完的幕布在设置页里再盖一次
  root.dataset.splash = s.launchAnimation ? "on" : "off";
  // 必须带单位：无单位的 "340" 会让 animation-duration 解析为 0s，
  // 动画瞬间放完——这是"启动动画不生效"的根因
  root.style.setProperty("--boot-ms", `${s.launchAnimMs}ms`);
  // 启动幕布（logo）的时长：跟随动画时长，但设下限（太短只看到一闪）
  // 与上限（托盘重开时不该被品牌幕布压住太久）
  const splash = Math.min(Math.max(s.launchAnimMs, 480), 900);
  root.style.setProperty("--splash-ms", `${splash}ms`);
  root.style.setProperty("--boot-ease", bootCurve(s.launchAnimCurve));
}

/** 重播启动动画：窗口真正可见时调用（冷启动首帧后 / 托盘重开 / 设置页预览）。
    挂载即播的动画在真实窗口首次绘制前就已放完，托盘重开的已挂载窗口
    更不会自己重播——统一以 off → 强制 reflow → 挂回预设 重启 */
export function replayBootAnim(s: AppSettings) {
  const root = document.documentElement;
  if (!s.launchAnimation) {
    root.dataset.bootAnim = "off";
    root.dataset.splash = "off";
    return;
  }
  // 三个属性一起归零再挂回：幕布与入场动画都必须重头播，
  // 否则托盘重开时看到的是"已经演完"的终态
  root.dataset.bootAnim = "off";
  root.dataset.splash = "off";
  void root.offsetWidth; // 强制 reflow：让 off 态落地后再挂回动画，动画才会重头播
  root.dataset.splash = "on";
  root.dataset.bootAnim = s.launchAnimPreset;
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
