import type { Lang } from "@/lib/i18n";

export const DISCOVER_STRINGS = {
  en: {
    search_placeholder: "Search wallpapers...",
    sort_label: "Sort by",
    sort_stars: "Most Stars",
    sort_updated: "Recently Updated",
    publish_cta: "Want to publish yours? Add the topic underpane-wallpaper to your GitHub repository.",
    rate_limit_error: "GitHub API rate limit exceeded or request failed. Please try again later.",
    no_results: "No wallpapers found.",
    stars_count: "stars",
    forks_count: "forks",
    back_to_list: "Back to Discover Wallpapers",
    download_zip: "Download Wallpaper (.zip)",
    view_on_github: "View on GitHub",
    releases_title: "Releases",
    no_releases: "No releases found. Check back later or compile from source.",
    loading: "Loading...",
    readme_tab: "README",
    discover_title: "Discover Wallpapers",
    discover_subtitle: "Explore community-created live wallpapers for Underpane.",
    install_btn: "Install",
    install_modal_title: "Underpane Required",
    install_modal_desc: "This action will attempt to open the Underpane desktop application using a custom protocol to install the wallpaper automatically. Please ensure you have Underpane installed and running.",
    install_modal_confirm: "Open Underpane",
    install_modal_fallback: "Download ZIP Directly",
    install_modal_cancel: "Cancel",
    install_modal_get_app: "Get Underpane Client",
  },
  "zh-CN": {
    search_placeholder: "搜索壁纸...",
    sort_label: "排序方式",
    sort_stars: "最多 Star",
    sort_updated: "最近更新",
    publish_cta: "想发布你的壁纸？为你的 GitHub 仓库添加 underpane-wallpaper 主题。",
    rate_limit_error: "GitHub API 达到速率限制或请求失败，请稍后再试。",
    no_results: "未找到壁纸。",
    stars_count: "获赞",
    forks_count: "分支",
    back_to_list: "返回发现壁纸",
    download_zip: "下载壁纸 (.zip)",
    view_on_github: "在 GitHub 查看",
    releases_title: "版本发布",
    no_releases: "未找到发布版本。请稍后再来，或从源码编译。",
    loading: "加载中...",
    readme_tab: "自述文件",
    discover_title: "发现壁纸",
    discover_subtitle: "探索由社区为 Underpane 创作的动态壁纸。",
    install_btn: "安装",
    install_modal_title: "需要 Underpane 客户端",
    install_modal_desc: "此操作将尝试通过自定义协议打开 Underpane 桌面应用以自动安装壁纸。请确保您已安装并运行 Underpane 客户端。",
    install_modal_confirm: "打开 Underpane",
    install_modal_fallback: "仅下载 ZIP",
    install_modal_cancel: "取消",
    install_modal_get_app: "获取 Underpane 客户端",
  },
} satisfies Record<Lang, Record<string, string>>;

export type DiscoverStringKey = keyof typeof DISCOVER_STRINGS["en"];

export function translateDiscover(lang: Lang, key: DiscoverStringKey): string {
  return DISCOVER_STRINGS[lang][key] || DISCOVER_STRINGS["en"][key] || key;
}
