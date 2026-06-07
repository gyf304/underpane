import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Github, Languages } from "lucide-react";
import { LANGS, SHARED_STRINGS, detectLang } from "@/lib/i18n";
import type { Lang, SharedStringKey } from "@/lib/i18n";
import "../index.css";

const REPO = "https://github.com/gyf304/underpane";

interface LayoutProps {
  children: (props: { lang: Lang }) => React.ReactNode;
}

export function Layout({ children }: LayoutProps) {
  const [lang, setLang] = useState<Lang>(detectLang);
  const [activePath, setActivePath] = useState("/");

  useEffect(() => {
    if (typeof window !== "undefined") {
      setActivePath(window.location.pathname);
    }
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (e: MediaQueryListEvent | MediaQueryList) => {
      document.documentElement.classList.toggle("dark", e.matches);
    };

    handleChange(mediaQuery);
    mediaQuery.addEventListener("change", handleChange);
    return () => {
      mediaQuery.removeEventListener("change", handleChange);
    };
  }, []);

  const tShared = (key: SharedStringKey) => {
    return SHARED_STRINGS[lang][key] || SHARED_STRINGS["en"][key] || key;
  };

  const changeLang = (next: Lang) => {
    setLang(next);
    window.localStorage.setItem("lang", next);
    // Dispatch custom event to notify other parts of the app
    window.dispatchEvent(new Event("languagechange"));
  };

  return (
    <div className="w-full min-h-screen flex flex-col">
      <header className="mx-auto flex w-full max-w-5xl items-center justify-between px-6 py-5">
        <div className="flex items-center gap-6">
          <a href="/" className="text-lg font-semibold tracking-tight hover:opacity-80">
            Underpane
          </a>
          <nav className="flex items-center gap-1">
            <Button
              asChild
              variant={activePath === "/" || activePath === "" ? "secondary" : "ghost"}
              size="sm"
            >
              <a href="/">{tShared("nav_home")}</a>
            </Button>
            <Button
              asChild
              variant={activePath.startsWith("/discover") ? "secondary" : "ghost"}
              size="sm"
            >
              <a href="/discover">{tShared("nav_discover")}</a>
            </Button>
          </nav>
        </div>

        <div className="flex items-center gap-1">
          <Select value={lang} onValueChange={(v) => changeLang(v as Lang)}>
            <SelectTrigger size="sm" className="gap-2 border-none shadow-none focus:ring-0">
              <Languages className="size-4" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="end">
              {LANGS.map((l) => (
                <SelectItem key={l.code} value={l.code}>
                  {l.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button asChild variant="ghost" size="sm">
            <a href={REPO} target="_blank" rel="noreferrer">
              <Github className="size-4" />
              <span className="hidden sm:inline ml-2">{tShared("nav_github")}</span>
            </a>
          </Button>
        </div>
      </header>

      <main className="flex-1 w-full">
        {children({ lang })}
      </main>

      <footer className="border-t mt-auto">
        <div className="mx-auto flex max-w-5xl flex-wrap items-center justify-between px-6 py-6 text-sm text-muted-foreground">
          <span className="text-xs">&copy; {new Date().getFullYear()} Underpane. All rights reserved.</span>
          <a href={REPO} target="_blank" rel="noreferrer" className="hover:text-foreground">
            github.com/gyf304/underpane
          </a>
        </div>
      </footer>
    </div>
  );
}
