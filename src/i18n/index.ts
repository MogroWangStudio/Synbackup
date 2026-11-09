// i18n 入口：取词 + 本地化格式化（语言注册表在零依赖的 ./locales）。
// 各语言文件与 zh.ts 以 Dict = typeof zh 做键对齐（vue-tsc 编译期强制），
// 缺键 / 多键直接编译失败，杜绝"某语言少一串文案"这类静默缺口。
import { store } from "../store";
import { zh } from "./zh";
import { en } from "./en";
import { zhTw } from "./zhTw";
import { ja } from "./ja";
import { ko } from "./ko";
import { fr } from "./fr";
import { de } from "./de";
import { es } from "./es";
import { pt } from "./pt";
import { it } from "./it";
import { ru } from "./ru";
import { isLocale, localeTag, type LocaleCode } from "./locales";

// 语言注册表对外统一从这里取（内部实现按需分文件，避免循环）
export { LOCALES, isLocale, localeTag } from "./locales";
export type { LocaleCode, LocaleInfo } from "./locales";

export type Dict = typeof zh;

const DICTS: Record<LocaleCode, Dict> = {
  "zh-CN": zh,
  "zh-TW": zhTw,
  "en-US": en,
  ja,
  ko,
  fr,
  de,
  es,
  pt,
  it,
  ru,
};

/** 取词：当前语言 → 源语言兜底 → 键名（便于发现漏网键）。
    键完整性在各语言文件的定义处由 Dict 类型强制（缺键 / 多键编译报错）；
    取词侧只做 string 索引，放宽读侧 */
export function t(key: string, params?: Record<string, string | number>): string {
  const lang = store.settings.language;
  const dict = isLocale(lang) ? (DICTS[lang] as Record<string, string>) : undefined;
  let text = dict?.[key] ?? (zh as Record<string, string>)[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replaceAll(`{${k}}`, String(v));
    }
  }
  return text;
}

/** 后端通知层的稳定错误码 → 本地化提示（其余错误原样展示） */
export function notifyError(raw: string): string {
  return raw.includes("notification-permission-denied")
    ? t("ntest.permission_denied")
    : raw;
}

export function fmtSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 ? String(v) : v >= 100 ? v.toFixed(0) : v.toFixed(1);
  return `${s} ${units[i]}`;
}

export function fmtTime(ms: number): string {
  if (!ms) return "—";
  const lang = localeTag(store.settings.language);
  const d = new Date(ms);
  const sameDay = new Date().toDateString() === d.toDateString();
  return d.toLocaleString(
    lang,
    sameDay
      ? { hour: "2-digit", minute: "2-digit" }
      : { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" },
  );
}

export function fmtDateTime(ms: number): string {
  if (!ms) return "—";
  const lang = localeTag(store.settings.language);
  return new Date(ms).toLocaleString(lang, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
