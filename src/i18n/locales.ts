// 语言注册表：纯数据、零依赖（不 import store / theme），
// 供 i18n/index 与 theme.ts 双方引用而不形成模块循环。
export type LocaleCode =
  | "zh-CN"
  | "zh-TW"
  | "en-US"
  | "ja"
  | "ko"
  | "fr"
  | "de"
  | "es"
  | "pt"
  | "it"
  | "ru";

export interface LocaleInfo {
  code: LocaleCode;
  /** 选择器里的语言名：一律用母语自称，不随界面语言翻译 */
  label: string;
}

/** 界面语言清单（顺序即设置页选择器顺序） */
export const LOCALES: LocaleInfo[] = [
  { code: "zh-CN", label: "简体中文" },
  { code: "zh-TW", label: "繁體中文" },
  { code: "en-US", label: "English" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
  { code: "pt", label: "Português" },
  { code: "it", label: "Italiano" },
  { code: "ru", label: "Русский" },
];

export function isLocale(v: string): v is LocaleCode {
  return LOCALES.some((l) => l.code === v);
}

/** toLocaleString / html.lang 用的 BCP-47；未知值回退源语言 */
export function localeTag(lang: string): string {
  return isLocale(lang) ? lang : "zh-CN";
}
