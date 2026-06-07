export type Lang = "en" | "zh-CN";

export const LANGS: { code: Lang; label: string }[] = [
  { code: "en", label: "English" },
  { code: "zh-CN", label: "简体中文" },
];

export const SHARED_STRINGS = {
  en: {
    nav_home: "Home",
    nav_discover: "Discover Wallpapers",
    nav_github: "GitHub",
  },
  "zh-CN": {
    nav_home: "首页",
    nav_discover: "发现壁纸",
    nav_github: "GitHub",
  },
} satisfies Record<Lang, Record<string, string>>;

export type SharedStringKey = keyof typeof SHARED_STRINGS["en"];

export function detectLang(): Lang {
  if (typeof window === "undefined") return "en";
  const stored = window.localStorage.getItem("lang");
  if (stored === "en" || stored === "zh-CN") return stored;
  return window.navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
