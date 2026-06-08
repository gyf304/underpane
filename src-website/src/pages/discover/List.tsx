import { useState, useEffect } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Star,
  GitFork,
  Search,
  Sparkles,
  AlertTriangle,
  ArrowRight,
} from "lucide-react";
import type { Lang } from "@/lib/i18n";
import type { DiscoverStringKey } from "./i18n";

// Helper to strip "underpane-wallpaper-" prefix and format the repository name
function formatRepoName(name: string): string {
  const prefix = "underpane-wallpaper-";
  if (name.startsWith(prefix)) {
    const raw = name.slice(prefix.length);
    return raw
      .split("-")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");
  }
  return name;
}

// Extracted metadata helper
interface WallpaperMeta {
  name: string;
  description: string;
}

function getWallpaperMeta(
  description: string | null,
  repoName: string,
  lang: Lang,
): WallpaperMeta {
  let name = formatRepoName(repoName);
  let cleanDesc = description || "";

  if (description) {
    const jsonMatch = description.match(/\{.*\}/);
    if (jsonMatch) {
      try {
        const meta = JSON.parse(jsonMatch[0]);
        cleanDesc = description.replace(jsonMatch[0], "").trim();
        if (meta && meta.name) {
          name = meta.name[lang] || meta.name[""] || meta.name["en"] || name;
        }
        if (meta && meta.desc) {
          cleanDesc =
            meta.desc[lang] || meta.desc[""] || meta.desc["en"] || cleanDesc;
        }
      } catch (e) {
        // Fallback silently if JSON parsing fails
      }
    }
  }

  return { name, description: cleanDesc };
}

// Custom HSL gradients based on repo name for a consistent, premium look
function getGradientStyle(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  const h1 = Math.abs(hash % 360);
  const h2 = (h1 + 40) % 360;
  return `linear-gradient(135deg, hsl(${h1}, 80%, 65%), hsl(${h2}, 85%, 55%))`;
}

interface GitHubRepo {
  id: number;
  name: string;
  full_name: string;
  description: string | null;
  stargazers_count: number;
  forks_count: number;
  pushed_at: string;
  default_branch: string;
  owner: {
    login: string;
    avatar_url: string;
  };
}

interface DiscoverListProps {
  lang: Lang;
  t: (key: DiscoverStringKey) => string;
  onSelectRepo: (fullName: string) => void;
}

// Progressive Image Loader component with fallback
function WallpaperPreview({
  owner,
  repo,
  defaultBranch,
  fallbackName,
}: {
  owner: string;
  repo: string;
  defaultBranch: string;
  fallbackName: string;
}) {
  const branch = defaultBranch || "main";
  const urls = [
    `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/preview.gif`,
    `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/preview.png`,
    `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/preview.jpg`,
  ];

  const [imgUrl, setImgUrl] = useState<string | null>(urls[0] || null);
  const [attempt, setAttempt] = useState(0);
  const [loading, setLoading] = useState(true);

  const handleError = () => {
    if (attempt < urls.length - 1) {
      const next = attempt + 1;
      setAttempt(next);
      setImgUrl(urls[next] || null);
    } else {
      setImgUrl(null);
      setLoading(false);
    }
  };

  const handleLoad = () => {
    setLoading(false);
  };

  if (!imgUrl) {
    return (
      <div
        className="w-full h-full flex items-center justify-center text-white font-bold text-xl select-none"
        style={{ background: getGradientStyle(repo) }}
      >
        <div className="flex flex-col items-center gap-2 drop-shadow-md">
          <Sparkles className="size-8 opacity-90 animate-pulse" />
          <span className="text-sm font-medium tracking-wide uppercase px-3 py-1 bg-black/10 rounded-full backdrop-blur-sm">
            {fallbackName.slice(0, 2)}
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="w-full h-full relative bg-muted overflow-hidden">
      {loading && (
        <div className="absolute inset-0 animate-pulse bg-neutral-200 dark:bg-neutral-800" />
      )}
      <img
        src={imgUrl}
        alt={fallbackName}
        className={`w-full h-full object-cover transition-transform duration-500 ease-out hover:scale-105 ${
          loading ? "opacity-0" : "opacity-100"
        }`}
        onLoad={handleLoad}
        onError={handleError}
      />
    </div>
  );
}

export function DiscoverList({ lang, t, onSelectRepo }: DiscoverListProps) {
  const [repos, setRepos] = useState<GitHubRepo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<"stars" | "updated">("stars");
  const [debouncedQuery, setDebouncedQuery] = useState("");

  // Debounce the search input aggressively
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedQuery(searchQuery);
    }, 600);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  useEffect(() => {
    async function fetchData() {
      setLoading(true);
      setError(false);

      // Normalize parameters to build the stable prefixed cache key
      const normalizedQuery = debouncedQuery.trim().toLowerCase();
      let q = "topic:underpane-wallpaper";
      if (normalizedQuery !== "") {
        q = `${normalizedQuery} topic:underpane-wallpaper`;
      }
      const cacheKey = `up_cache:q=${encodeURIComponent(q)}&sort=${sortBy}`;

      // 1. Check SessionStorage cache for this query
      const cached = sessionStorage.getItem(cacheKey);
      if (cached) {
        try {
          const { timestamp, data } = JSON.parse(cached);
          // 5-minute cache TTL
          if (Date.now() - timestamp < 5 * 60 * 1000) {
            setRepos(data);
            setLoading(false);
            return;
          }
        } catch (e) {
          // Fallback to fetch
        }
      }

      // 2. Fetch from GitHub Search API using search and sorting parameters
      try {
        const url = `https://api.github.com/search/repositories?q=${encodeURIComponent(q)}&sort=${sortBy}&order=desc`;
        const response = await fetch(url);
        if (!response.ok) {
          throw new Error("GitHub API failed");
        }
        const result = await response.json();
        const items = (result.items || []) as GitHubRepo[];

        // Store response in cache
        sessionStorage.setItem(
          cacheKey,
          JSON.stringify({
            timestamp: Date.now(),
            data: items,
          }),
        );

        setRepos(items);
        setError(false);
      } catch (err) {
        console.error("Error loading wallpapers from GitHub:", err);
        setError(true);
      } finally {
        setLoading(false);
      }
    }

    fetchData();
  }, [debouncedQuery, sortBy]);

  // Process item names/descriptions
  const processedRepos = repos.map((repo) => {
    const meta = getWallpaperMeta(repo.description, repo.name, lang);
    return {
      ...repo,
      metaName: meta.name,
      metaDesc: meta.description,
    };
  });

  return (
    <div className="w-full space-y-8 animate-fade-in duration-300">
      {/* Page Title */}
      <div className="text-center sm:text-left space-y-2">
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">
          {t("discover_title")}
        </h1>
        <p className="text-muted-foreground max-w-2xl">
          {t("discover_subtitle")}
        </p>
      </div>

      {/* Info notice about publishing */}
      <div className="flex items-center gap-3 p-4 rounded-lg bg-primary/5 border border-primary/20 text-sm text-primary/80">
        <Sparkles className="size-5 shrink-0 text-primary" />
        <p>{t("publish_cta")}</p>
      </div>

      {/* Search & Sort Panel */}
      <div className="flex flex-col sm:flex-row gap-4 items-center justify-between border-b pb-6">
        <div className="relative w-full sm:max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder={t("search_placeholder")}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>

        <div className="flex items-center gap-2 w-full sm:w-auto shrink-0 justify-end">
          <span className="text-sm text-muted-foreground hidden md:inline">
            {t("sort_label")}:
          </span>
          <Select
            value={sortBy}
            onValueChange={(v) => setSortBy(v as "stars" | "updated")}
          >
            <SelectTrigger className="w-full sm:w-[180px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="stars">{t("sort_stars")}</SelectItem>
              <SelectItem value="updated">{t("sort_updated")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div className="flex flex-col items-center justify-center py-16 text-center space-y-4 border rounded-xl bg-destructive/5 border-destructive/20">
          <AlertTriangle className="size-12 text-destructive animate-bounce" />
          <p className="text-muted-foreground max-w-md font-medium">
            {t("rate_limit_error")}
          </p>
          <Button
            onClick={() => window.location.reload()}
            variant="outline"
            size="sm"
          >
            Retry
          </Button>
        </div>
      )}

      {/* Loading state */}
      {loading && (
        <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((n) => (
            <Card
              key={n}
              className="overflow-hidden border shadow-sm py-0 gap-0"
            >
              <div className="w-full aspect-video bg-neutral-200 dark:bg-neutral-800 animate-pulse" />
              <CardHeader className="space-y-2 pt-5 pb-2">
                <div className="h-6 w-2/3 bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
                <div className="h-4 w-1/3 bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
              </CardHeader>
              <CardContent className="h-16 bg-neutral-200 dark:bg-neutral-800 animate-pulse pb-5" />
            </Card>
          ))}
        </div>
      )}

      {/* Grid List */}
      {!loading && !error && (
        <>
          {processedRepos.length === 0 ? (
            <div className="text-center py-20 text-muted-foreground">
              <p className="text-lg font-medium">{t("no_results")}</p>
            </div>
          ) : (
            <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
              {processedRepos.map((repo) => (
                <Card
                  key={repo.id}
                  className="overflow-hidden border shadow-sm flex flex-col group hover:shadow-md hover:border-neutral-400/50 dark:hover:border-neutral-600/50 transition-all duration-300 cursor-pointer py-0 gap-0"
                  onClick={() => onSelectRepo(repo.full_name)}
                >
                  {/* Aspect Ratio Box for image/fallback */}
                  <div className="w-full aspect-video shrink-0 border-b relative">
                    <WallpaperPreview
                      owner={repo.owner.login}
                      repo={repo.name}
                      defaultBranch={repo.default_branch}
                      fallbackName={repo.metaName}
                    />
                  </div>

                  <CardHeader className="space-y-1 pt-5 pb-2 flex-grow">
                    <div className="flex items-center gap-2 mb-1">
                      <img
                        src={repo.owner.avatar_url}
                        alt={repo.owner.login}
                        className="size-4 rounded-full"
                      />
                      <span className="text-xs text-muted-foreground hover:underline">
                        {repo.owner.login}
                      </span>
                    </div>
                    <CardTitle className="text-xl group-hover:text-primary transition-colors duration-200 flex items-center justify-between">
                      <span className="truncate">{repo.metaName}</span>
                      <ArrowRight className="size-4 shrink-0 opacity-0 group-hover:opacity-100 transition-all duration-300 translate-x-[-10px] group-hover:translate-x-0 text-primary" />
                    </CardTitle>
                  </CardHeader>

                  <CardContent className="space-y-4 pb-5">
                    <p className="text-sm text-muted-foreground line-clamp-2 h-10">
                      {repo.metaDesc || "No description provided."}
                    </p>

                    <div className="flex items-center justify-between pt-2 border-t text-xs text-muted-foreground">
                      <div className="flex items-center gap-3">
                        <span className="flex items-center gap-1">
                          <Star className="size-3.5 fill-amber-400 stroke-amber-400" />
                          {repo.stargazers_count}
                        </span>
                        <span className="flex items-center gap-1">
                          <GitFork className="size-3.5" />
                          {repo.forks_count}
                        </span>
                      </div>
                      <span>
                        {new Date(repo.pushed_at).toLocaleDateString(
                          lang === "zh-CN" ? "zh-CN" : "en-US",
                          { year: "numeric", month: "short", day: "numeric" },
                        )}
                      </span>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
