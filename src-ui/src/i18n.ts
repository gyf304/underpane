import enUS from "./locales/en-US";
import zhCN from "./locales/zh-CN";

type Messages = Record<string, string>;

const LOCALES: Record<string, Messages> = {
  "en-US": enUS,
  "zh-CN": zhCN,
};

const FALLBACK = "en-US";

function pickLocale(): string {
  const candidates: string[] = [];
  if (typeof navigator !== "undefined") {
    if (navigator.languages) candidates.push(...navigator.languages);
    if (navigator.language) candidates.push(navigator.language);
  }

  for (const c of candidates) {
    if (LOCALES[c]) return c;
  }
  for (const c of candidates) {
    const prefix = c.split("-")[0];
    const match = Object.keys(LOCALES).find(k => k.split("-")[0] === prefix);
    if (match) return match;
  }
  return FALLBACK;
}

export const LOCALE = pickLocale();

const fallbackMessages = LOCALES[FALLBACK]!;
const messages = LOCALES[LOCALE] ?? fallbackMessages;

export function t(key: string): string {
  return messages[key] ?? fallbackMessages[key] ?? key;
}
