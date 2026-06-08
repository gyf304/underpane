import { createContext, createElement, useContext, useEffect, useState, type ReactNode } from "react";
import { detectLang } from "@/lib/i18n";
import type { Lang } from "@/lib/i18n";

type LangSetter = (next: Lang) => void;
type LangValue = readonly [Lang, LangSetter];

const LangContext = createContext<LangValue | null>(null);

const STORAGE_KEY = "lang";
const CHANGE_EVENT = "languagechange";

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(detectLang);

  useEffect(() => {
    const handleChange = () => {
      setLang(detectLang());
    };
    window.addEventListener(CHANGE_EVENT, handleChange);
    return () => window.removeEventListener(CHANGE_EVENT, handleChange);
  }, []);

  const update = (next: Lang) => {
    setLang(next);
    window.localStorage.setItem(STORAGE_KEY, next);
    window.dispatchEvent(new Event(CHANGE_EVENT));
  };

  return createElement(
    LangContext.Provider,
    { value: [lang, update] as LangValue },
    children,
  );
}

export function useLang(): LangValue {
  const ctx = useContext(LangContext);
  if (!ctx) throw new Error("useLang must be used within a LangProvider");
  return ctx;
}
