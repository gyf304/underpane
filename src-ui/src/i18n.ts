import type { ReactNode } from "react";
import enUS from "./locales/en-US";
import zhCN from "./locales/zh-CN";

export type MessageValue =
  | string
  | ((params: Record<string, unknown>) => ReactNode);

export type Messages = typeof enUS;

const LOCALES = {
  "en-US": enUS,
  "zh-CN": zhCN,
} as const satisfies Record<string, Partial<Messages>>;

const FALLBACK = "en-US";

function pickLocale(): string {
  const candidates: string[] = [];
  if (typeof navigator !== "undefined") {
    if (navigator.languages) candidates.push(...navigator.languages);
    if (navigator.language) candidates.push(navigator.language);
  }

  for (const c of candidates) {
    if ((LOCALES as any)[c]) return c;
  }
  for (const c of candidates) {
    const prefix = c.split("-")[0];
    const match = Object.keys(LOCALES).find((k) => k.split("-")[0] === prefix);
    if (match) return match;
  }
  return FALLBACK;
}

export const LOCALE = pickLocale();

const fallbackMessages: Messages = LOCALES[FALLBACK]!;
const messages: Messages = (LOCALES as any)[LOCALE] ?? fallbackMessages;

export function t<K extends keyof Messages>(key: K): Messages[K] {
  return (messages[key] ?? fallbackMessages[key] ?? key) as Messages[K];
}
