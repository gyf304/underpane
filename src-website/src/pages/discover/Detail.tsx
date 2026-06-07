import { useState, useEffect } from "react";
import { marked } from "marked";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  ArrowLeft,
  Star,
  GitFork,
  Download,
  Github,
  Calendar,
  Layers,
  Info,
  ChevronDown,
  ChevronUp,
  FileText,
  Clock,
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

function getWallpaperMeta(description: string | null, repoName: string, lang: Lang) {
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
      } catch (e) {
        // Fallback silently on parse error
      }
    }
  }

  return { name, description: cleanDesc };
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

interface RepoInfo {
  name: string;
  full_name: string;
  description: string | null;
  stargazers_count: number;
  forks_count: number;
  created_at: string;
  updated_at: string;
  default_branch: string;
  license: { name: string } | null;
  owner: {
    login: string;
    avatar_url: string;
    html_url: string;
  };
}

interface ReleaseAsset {
  name: string;
  browser_download_url: string;
  size: number;
  download_count: number;
}

interface GitHubRelease {
  id: number;
  tag_name: string;
  name: string;
  published_at: string;
  body: string;
  html_url: string;
  assets: ReleaseAsset[];
}

interface DiscoverDetailProps {
  repoFullName: string;
  lang: Lang;
  t: (key: DiscoverStringKey) => string;
  onBack: () => void;
}

export function DiscoverDetail({ repoFullName, lang, t, onBack }: DiscoverDetailProps) {
  const [repo, setRepo] = useState<RepoInfo | null>(null);
  const [readmeHtml, setReadmeHtml] = useState<string>("");
  const [releases, setReleases] = useState<GitHubRelease[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedReleases, setExpandedReleases] = useState<Record<number, boolean>>({});
  const [modalZipUrl, setModalZipUrl] = useState<string | null>(null);

  useEffect(() => {
    async function loadDetail() {
      setLoading(true);
      setError(null);
      try {
        // 1. Fetch Repository Info
        const repoRes = await fetch(`https://api.github.com/repos/${repoFullName}`);
        if (!repoRes.ok) {
          throw new Error(`Failed to load repository information (${repoRes.status})`);
        }
        const repoData = (await repoRes.json()) as RepoInfo;
        setRepo(repoData);

        const branch = repoData.default_branch || "main";

        // 2. Fetch README with i18n priorities
        const readmePaths: string[] = [];
        if (lang === "zh-CN") {
          readmePaths.push("README.zh-CN.md", "README.zh.md");
        }
        readmePaths.push("README.md");

        let readmeText = "";
        let readmeLoaded = false;

        for (const filename of readmePaths) {
          try {
            const readmeRes = await fetch(
              `https://raw.githubusercontent.com/${repoFullName}/${branch}/${filename}`
            );
            if (readmeRes.ok) {
              readmeText = await readmeRes.text();
              readmeLoaded = true;
              break;
            }
          } catch (e) {
            // Check next
          }
        }

        if (readmeLoaded) {
          // Parse README markdown safely using marked
          const parsed = await marked.parse(readmeText);
          setReadmeHtml(parsed);
        } else {
          setReadmeHtml(`<p class="text-muted-foreground italic">No README found for this wallpaper.</p>`);
        }

        // 3. Fetch Releases Info
        try {
          const releasesRes = await fetch(`https://api.github.com/repos/${repoFullName}/releases`);
          if (releasesRes.ok) {
            const releasesData = (await releasesRes.json()) as GitHubRelease[];
            setReleases(releasesData);
            // Expand the latest release by default
            if (releasesData[0]) {
              setExpandedReleases({ [releasesData[0].id]: true });
            }
          }
        } catch (e) {
          console.warn("Failed to fetch releases:", e);
        }
      } catch (err: any) {
        setError(err.message || "Failed to load details");
      } finally {
        setLoading(false);
      }
    }

    loadDetail();
  }, [repoFullName, lang]);

  const toggleRelease = (id: number) => {
    setExpandedReleases((prev) => ({
      ...prev,
      [id]: !prev[id],
    }));
  };

  if (loading) {
    return (
      <div className="space-y-6 py-10 animate-fade-in">
        <Button onClick={onBack} variant="ghost" size="sm" className="gap-2">
          <ArrowLeft className="size-4" />
          {t("back_to_list")}
        </Button>
        <div className="grid gap-6 md:grid-cols-3">
          <div className="md:col-span-2 space-y-4">
            <div className="h-10 w-2/3 bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
            <div className="h-6 w-1/3 bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
            <div className="h-96 w-full bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded mt-6" />
          </div>
          <div className="space-y-4">
            <div className="h-48 w-full bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
            <div className="h-32 w-full bg-neutral-200 dark:bg-neutral-800 animate-pulse rounded" />
          </div>
        </div>
      </div>
    );
  }

  if (error || !repo) {
    return (
      <div className="space-y-6 py-10">
        <Button onClick={onBack} variant="ghost" size="sm" className="gap-2">
          <ArrowLeft className="size-4" />
          {t("back_to_list")}
        </Button>
        <div className="p-8 rounded-xl border border-destructive/20 bg-destructive/5 flex flex-col items-center justify-center text-center space-y-4">
          <Info className="size-12 text-destructive" />
          <h2 className="text-xl font-bold">Error Loading Details</h2>
          <p className="text-muted-foreground max-w-md">{error || "Repository not found."}</p>
          <Button onClick={onBack} variant="outline">
            Return to list
          </Button>
        </div>
      </div>
    );
  }

  const meta = getWallpaperMeta(repo.description, repo.name, lang);

  // Search releases for .underpane.zip download
  let downloadAsset: ReleaseAsset | null = null;
  let latestReleaseUrl = `${repo.owner.html_url}/${repo.name}/releases`;

  if (releases.length > 0 && releases[0]) {
    latestReleaseUrl = releases[0].html_url;
    const asset = releases[0].assets.find((a) => a.name.endsWith(".underpane.zip"));
    if (asset) {
      downloadAsset = asset;
    }
  }

  return (
    <div className="space-y-6 py-4 animate-fade-in">
      {/* Back button */}
      <Button onClick={onBack} variant="ghost" size="sm" className="gap-2 self-start">
        <ArrowLeft className="size-4" />
        {t("back_to_list")}
      </Button>

      {/* Header section */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-6 border-b pb-6">
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <img src={repo.owner.avatar_url} alt={repo.owner.login} className="size-5 rounded-full" />
            <a
              href={repo.owner.html_url}
              target="_blank"
              rel="noreferrer"
              className="text-sm font-medium hover:underline"
            >
              {repo.owner.login}
            </a>
          </div>
          <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">{meta.name}</h1>
          <p className="text-muted-foreground text-lg">{meta.description || "No description provided."}</p>
        </div>

        {/* Top actions */}
        <div className="flex items-center gap-3 shrink-0">
          <Button asChild variant="outline" size="lg">
            <a href={repo.owner.html_url + "/" + repo.name} target="_blank" rel="noreferrer">
              <Github className="mr-2 size-4" />
              {t("view_on_github")}
            </a>
          </Button>
          {downloadAsset ? (
            <Button
              onClick={() => setModalZipUrl(downloadAsset.browser_download_url)}
              size="lg"
              className="bg-primary hover:bg-primary/95 shadow-md"
            >
              <Download className="mr-2 size-4" />
              {t("install_btn")}
            </Button>
          ) : (
            <Button asChild size="lg" className="bg-primary hover:bg-primary/95 shadow-md">
              <a href={latestReleaseUrl} target="_blank" rel="noreferrer">
                <Github className="mr-2 size-4" />
                {t("view_on_github")}
              </a>
            </Button>
          )}
        </div>
      </div>

      {/* Grid Layout: README (main) vs Repo Metadata/Releases (sidebar) */}
      <div className="grid gap-8 md:grid-cols-3">
        {/* Main README section */}
        <div className="md:col-span-2 space-y-6">
          <Card className="border shadow-sm overflow-hidden py-0 gap-0">
            <div className="px-6 py-4 border-b bg-muted/40 flex items-center gap-2">
              <FileText className="size-4 text-muted-foreground" />
              <span className="font-semibold text-sm tracking-tight text-muted-foreground uppercase">
                {t("readme_tab")}
              </span>
            </div>
            <CardContent className="p-6 md:p-8">
              <article
                className="readme-content"
                dangerouslySetInnerHTML={{ __html: readmeHtml }}
              />
            </CardContent>
          </Card>
        </div>

        {/* Sidebar */}
        <div className="space-y-6">
          {/* Metadata Card */}
          <Card className="border shadow-sm py-0 gap-0">
            <div className="px-6 py-4 border-b bg-muted/40 flex items-center gap-2">
              <Info className="size-4 text-muted-foreground" />
              <span className="font-semibold text-sm tracking-tight text-muted-foreground uppercase">
                Wallpaper Info
              </span>
            </div>
            <CardContent className="pt-4 pb-6 space-y-4 text-sm">
              <div className="flex justify-between items-center py-1 border-b border-border/50">
                <span className="text-muted-foreground">Stars</span>
                <span className="flex items-center gap-1 font-semibold">
                  <Star className="size-4 fill-amber-400 stroke-amber-400" />
                  {repo.stargazers_count}
                </span>
              </div>
              <div className="flex justify-between items-center py-1 border-b border-border/50">
                <span className="text-muted-foreground">Forks</span>
                <span className="flex items-center gap-1 font-semibold">
                  <GitFork className="size-4" />
                  {repo.forks_count}
                </span>
              </div>
              {repo.license && (
                <div className="flex justify-between items-center py-1 border-b border-border/50">
                  <span className="text-muted-foreground">License</span>
                  <span className="font-medium truncate max-w-[150px]">{repo.license.name}</span>
                </div>
              )}
              <div className="flex justify-between items-center py-1 border-b border-border/50">
                <span className="text-muted-foreground">Default Branch</span>
                <span className="font-mono text-xs">{repo.default_branch}</span>
              </div>
              <div className="flex justify-between items-center py-1 border-b border-border/50">
                <span className="text-muted-foreground">Created</span>
                <span className="flex items-center gap-1">
                  <Calendar className="size-3.5 text-muted-foreground" />
                  {new Date(repo.created_at).toLocaleDateString(
                    lang === "zh-CN" ? "zh-CN" : "en-US",
                    { year: "numeric", month: "short", day: "numeric" }
                  )}
                </span>
              </div>
              <div className="flex justify-between items-center py-1">
                <span className="text-muted-foreground">Last Updated</span>
                <span className="flex items-center gap-1">
                  <Clock className="size-3.5 text-muted-foreground" />
                  {new Date(repo.updated_at).toLocaleDateString(
                    lang === "zh-CN" ? "zh-CN" : "en-US",
                    { year: "numeric", month: "short", day: "numeric" }
                  )}
                </span>
              </div>

              {/* Direct download stats if asset available */}
              {downloadAsset && (
                <div className="mt-4 p-3 rounded-lg bg-neutral-50 dark:bg-neutral-900 border text-xs space-y-2">
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Filename:</span>
                    <span className="font-mono truncate max-w-[160px] text-right font-medium">
                      {downloadAsset.name}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Size:</span>
                    <span className="font-medium">{formatFileSize(downloadAsset.size)}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Downloads:</span>
                    <span className="font-medium">{downloadAsset.download_count}</span>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Releases Card */}
          <Card className="border shadow-sm py-0 gap-0">
            <div className="px-6 py-4 border-b bg-muted/40 flex items-center gap-2">
              <Layers className="size-4 text-muted-foreground" />
              <span className="font-semibold text-sm tracking-tight text-muted-foreground uppercase">
                {t("releases_title")}
              </span>
            </div>
            <CardContent className="p-0 max-h-[380px] overflow-y-auto divide-y">
              {releases.length === 0 ? (
                <div className="p-6 text-center text-muted-foreground text-sm">
                  {t("no_releases")}
                </div>
              ) : (
                releases.map((release) => {
                  const isExpanded = !!expandedReleases[release.id];
                  const zipAsset = release.assets.find((a) => a.name.endsWith(".underpane.zip"));
                  const hasZip = !!zipAsset;
                  return (
                    <div key={release.id} className="p-4 space-y-2">
                      <button
                        className="w-full flex items-center justify-between text-left hover:text-primary transition-colors focus:outline-none"
                        onClick={() => toggleRelease(release.id)}
                      >
                        <div className="space-y-0.5 pr-2">
                          <div className="font-semibold text-sm flex items-center gap-1.5">
                            <span className="text-foreground">{release.tag_name}</span>
                            {hasZip && (
                              <span className="text-[10px] uppercase font-bold tracking-wide px-1.5 py-0.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 rounded-full border border-emerald-500/20">
                                ZIP
                              </span>
                            )}
                          </div>
                          <div className="text-[10px] text-muted-foreground">
                            {new Date(release.published_at).toLocaleDateString(
                              lang === "zh-CN" ? "zh-CN" : "en-US",
                              { year: "numeric", month: "short", day: "numeric" }
                            )}
                          </div>
                        </div>
                        {isExpanded ? (
                          <ChevronUp className="size-4 shrink-0 text-muted-foreground" />
                        ) : (
                          <ChevronDown className="size-4 shrink-0 text-muted-foreground" />
                        )}
                      </button>

                      {isExpanded && (
                        <div className="pt-2 text-xs text-muted-foreground space-y-3 animate-slide-down">
                          <div
                            className="readme-content text-xs leading-relaxed max-h-[150px] overflow-y-auto border p-2 rounded bg-neutral-50 dark:bg-neutral-900 border-border/50"
                            dangerouslySetInnerHTML={{
                              __html: marked.parse(release.body || "*No release notes provided.*") as string,
                            }}
                          />
                          <div className="flex flex-col gap-2">
                            {zipAsset && (
                              <Button
                                onClick={() => setModalZipUrl(zipAsset.browser_download_url)}
                                size="sm"
                                className="w-full h-8 text-[11px]"
                              >
                                <Download className="size-3 mr-1" />
                                {t("install_btn")}
                              </Button>
                            )}
                            <Button asChild size="sm" variant="outline" className="w-full h-8 text-[11px]">
                              <a href={release.html_url} target="_blank" rel="noreferrer">
                                <Github className="size-3 mr-1" />
                                Release Notes
                              </a>
                            </Button>
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </CardContent>
          </Card>
        </div>
      </div>

      {/* Installation Modal */}
      {modalZipUrl && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md animate-fade-in">
          <div className="bg-card border text-card-foreground p-6 rounded-xl max-w-md w-full shadow-2xl relative space-y-4 animate-scale-up">
            <div className="flex items-center gap-3">
              <Info className="size-6 text-primary shrink-0 animate-pulse" />
              <h3 className="text-xl font-bold tracking-tight">{t("install_modal_title")}</h3>
            </div>

            <p className="text-sm text-muted-foreground leading-relaxed">
              {t("install_modal_desc")}
            </p>

            <div className="flex flex-col gap-2 pt-2">
              <Button
                onClick={() => {
                  window.location.href = `underpane+${modalZipUrl}`;
                  setModalZipUrl(null);
                }}
                className="w-full bg-primary hover:bg-primary/95 text-white font-medium"
              >
                {t("install_modal_confirm")}
              </Button>

              <div className="grid grid-cols-2 gap-2">
                <Button
                  asChild
                  variant="outline"
                  className="w-full"
                  onClick={() => setModalZipUrl(null)}
                >
                  <a href={modalZipUrl}>
                    {t("install_modal_fallback")}
                  </a>
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => setModalZipUrl(null)}
                  className="w-full"
                >
                  {t("install_modal_cancel")}
                </Button>
              </div>

              <Button
                asChild
                variant="link"
                size="sm"
                className="w-full text-xs text-muted-foreground mt-1 hover:text-foreground"
              >
                <a href="https://github.com/gyf304/underpane/releases" target="_blank" rel="noreferrer">
                  {t("install_modal_get_app")}
                </a>
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
