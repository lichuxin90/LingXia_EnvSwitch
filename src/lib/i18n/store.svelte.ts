//! i18n 内核：语言状态 + 取词。词条只来自四个目录，缺键由 TypeScript 在编译期拦下。

import { en } from "./en";
import { ja } from "./ja";
import { zhCN, type MsgKey } from "./zh-CN";
import { zhTW } from "./zh-TW";

export type Locale = "zh-CN" | "zh-TW" | "en" | "ja";
export type LanguagePref = "auto" | Locale;
export type Params = Record<string, string | number>;

const catalogs: Record<Locale, Record<MsgKey, string>> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  en,
  ja,
};

/** 语言选择器里的本族语名称：翻成界面语言反而认不出母语长什么样。 */
export const LOCALE_LABEL: Record<Locale, string> = {
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
  ja: "日本語",
};

/** 按 navigator.language 猜一次，只在模块加载时求值。 */
function detectSystemLocale(): Locale {
  const tag = navigator.language.toLowerCase();
  if (tag.startsWith("ja")) return "ja";
  if (tag.startsWith("zh")) return /-(tw|hk|mo)\b|-hant/.test(tag) ? "zh-TW" : "zh-CN";
  return "en";
}

function fill(template: string, params: Params): string {
  let out = template;
  for (const [name, value] of Object.entries(params)) {
    out = out.split(`{${name}}`).join(String(value));
  }
  return out;
}

class I18n {
  pref = $state<LanguagePref>("auto");
  readonly system: Locale = detectSystemLocale();

  get locale(): Locale {
    return this.pref === "auto" ? this.system : this.pref;
  }

  format(key: MsgKey, params?: Params): string {
    const text = catalogs[this.locale][key];
    return params ? fill(text, params) : text;
  }
}

export const i18n = new I18n();

/** 在模板或 $derived 里调用会随语言切换重算；在事件回调里调用只是取一次词。 */
export function t(key: MsgKey, params?: Params): string {
  return i18n.format(key, params);
}

export type { MsgKey };
