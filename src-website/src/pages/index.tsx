import { StrictMode } from "react";
import { Code2, Monitor, Feather, Download, Compass } from "lucide-react";

import { Layout } from "@/components/Layout";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import type { Lang } from "@/lib/i18n";
import { useLang } from "@/hooks/lang";
import demoVideo from "@/demo.mp4";
import { renderRoot } from "@/lib/render";

const REPO = "https://github.com/gyf304/underpane";
const RELEASES = `${REPO}/releases`;

const HOME_STRINGS = {
  en: {
    hero_title_1: "Live wallpapers",
    hero_title_2: "for macOS and Windows",
    hero_subtitle: "Animated, interactive wallpapers for every monitor.",
    cta_download: "Download",
    cta_github: "Discover Wallpapers",
    feature_live_title: "Live, interactive wallpapers",
    feature_live_desc:
      "Animations, shaders, dashboards — bring your desktop background to life.",
    feature_monitor_title: "Per-monitor configuration",
    feature_monitor_desc:
      "Pick a different wallpaper and settings for each display you own.",
    feature_light_title: "Lightweight",
    feature_light_desc: "A small native footprint on both macOS and Windows.",
  },
  "zh-CN": {
    hero_title_1: "动态壁纸",
    hero_title_2: "适用于 macOS 与 Windows",
    hero_subtitle: "为每块显示器打造动态、可交互的壁纸。",
    cta_download: "下载",
    cta_github: "发现壁纸",
    feature_live_title: "动态可交互壁纸",
    feature_live_desc: "动画、着色器、仪表盘 —— 让你的桌面背景动起来。",
    feature_monitor_title: "按显示器配置",
    feature_monitor_desc: "为每块显示器分别选择壁纸与设置。",
    feature_light_title: "轻量",
    feature_light_desc: "在 macOS 与 Windows 上都保持极小的原生体积。",
  },
} satisfies Record<Lang, Record<string, string>>;

type HomeStringKey = keyof (typeof HOME_STRINGS)["en"];

const features = [
  { icon: Code2, titleKey: "feature_live_title", descKey: "feature_live_desc" },
  {
    icon: Monitor,
    titleKey: "feature_monitor_title",
    descKey: "feature_monitor_desc",
  },
  {
    icon: Feather,
    titleKey: "feature_light_title",
    descKey: "feature_light_desc",
  },
] satisfies {
  icon: typeof Code2;
  titleKey: HomeStringKey;
  descKey: HomeStringKey;
}[];

export function HomePage() {
  const [lang] = useLang();
  const t = (key: HomeStringKey) =>
    HOME_STRINGS[lang][key] || HOME_STRINGS["en"][key] || key;

  return (
    <div className="w-full">
      <main className="mx-auto max-w-5xl px-6">
        <section className="py-20 text-center sm:py-28">
          <h1 className="text-4xl font-bold tracking-tight sm:text-6xl animate-fade-in">
            {t("hero_title_1")}
            <br />
            {t("hero_title_2")}
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground animate-fade-in delay-75">
            {t("hero_subtitle")}
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4 animate-fade-in delay-150">
            <Button asChild size="lg">
              <a href={RELEASES} target="_blank" rel="noreferrer">
                <Download className="mr-2 size-4" />
                {t("cta_download")}
              </a>
            </Button>
            <Button asChild size="lg" variant="outline">
              <a href="/discover">
                <Compass className="mr-2 size-4" />
                {t("cta_github")}
              </a>
            </Button>
          </div>
        </section>

        <section className="pb-20 animate-fade-in delay-200">
          <video
            src={demoVideo}
            autoPlay
            loop
            muted
            playsInline
            className="w-full rounded-xl border shadow-lg"
          />
        </section>

        <section className="grid gap-6 pb-24 sm:grid-cols-3">
          {features.map(({ icon: Icon, titleKey, descKey }) => (
            <Card
              key={titleKey}
              className="hover:-translate-y-1 transition-transform duration-300"
            >
              <CardHeader className="gap-3">
                <Icon className="size-6 text-primary" />
                <CardTitle>{t(titleKey)}</CardTitle>
              </CardHeader>
              <CardContent className="text-muted-foreground">
                {t(descKey)}
              </CardContent>
            </Card>
          ))}
        </section>
      </main>
    </div>
  );
}

// Mount the Home Page to the DOM
const rootApp = (
  <StrictMode>
    <Layout>
      <HomePage />
    </Layout>
  </StrictMode>
);

renderRoot(rootApp);
export default rootApp;
