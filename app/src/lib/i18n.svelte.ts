import en from "./i18n/en.json";
import ru from "./i18n/ru.json";

export type Lang = "en" | "ru";
type Dict = { [key: string]: string | Dict };

const dicts: Record<Lang, Dict> = { en, ru };

export const i18n = $state({ lang: "en" as Lang });

export function detectLanguage(): Lang {
  const langs = navigator.languages?.length ? navigator.languages : [navigator.language];
  return langs.some((l) => l?.toLowerCase().startsWith("ru")) ? "ru" : "en";
}

export function setLanguage(pref: string) {
  i18n.lang = pref === "en" || pref === "ru" ? pref : detectLanguage();
  document.documentElement.lang = i18n.lang;
}

function lookup(dict: Dict, key: string): string | undefined {
  let node: string | Dict | undefined = dict;
  for (const part of key.split(".")) {
    if (typeof node !== "object") return undefined;
    node = node[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** Translates `key` ("a.b.c"), substituting `{name}` placeholders. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const text = lookup(dicts[i18n.lang], key) ?? lookup(dicts.en, key) ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? `{${name}}`));
}
