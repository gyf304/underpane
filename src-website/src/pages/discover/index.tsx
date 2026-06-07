import { StrictMode, useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { Layout } from "../Layout";
import { DiscoverList } from "./List";
import { DiscoverDetail } from "./Detail";
import { translateDiscover } from "./i18n";
import type { DiscoverStringKey } from "./i18n";

function getRepoFromHash(): string | null {
  if (typeof window === "undefined") return null;
  const hash = window.location.hash; // e.g., "#repo=gyf304/underpane-wallpaper-galaxy"
  const params = new URLSearchParams(hash.slice(1));
  return params.get("repo");
}

export function DiscoverPage() {
  const [activeRepo, setActiveRepo] = useState<string | null>(getRepoFromHash);

  useEffect(() => {
    const handleHashChange = () => {
      setActiveRepo(getRepoFromHash());
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  const selectRepo = (repoFullName: string) => {
    window.location.hash = `repo=${encodeURIComponent(repoFullName)}`;
  };

  const clearRepo = () => {
    window.location.hash = "";
  };

  return (
    <Layout>
      {({ lang }) => {
        const t = (key: DiscoverStringKey) => translateDiscover(lang, key);

        return (
          <div className="w-full">
            <main className="mx-auto max-w-5xl px-6 py-10">
              {activeRepo ? (
                <DiscoverDetail
                  repoFullName={activeRepo}
                  lang={lang}
                  t={t}
                  onBack={clearRepo}
                />
              ) : (
                <DiscoverList
                  lang={lang}
                  t={t}
                  onSelectRepo={selectRepo}
                />
              )}
            </main>
          </div>
        );
      }}
    </Layout>
  );
}

// Mount the Discover Page to the DOM
const elem = document.getElementById("root")!;
const rootApp = (
  <StrictMode>
    <DiscoverPage />
  </StrictMode>
);

if (import.meta.hot) {
  const root = (import.meta.hot.data.root ??= createRoot(elem));
  root.render(rootApp);
} else {
  createRoot(elem).render(rootApp);
}
