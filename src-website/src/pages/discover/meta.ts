import type { Lang } from "@/lib/i18n";

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

export { formatRepoName, getWallpaperMeta };
export type { WallpaperMeta };
