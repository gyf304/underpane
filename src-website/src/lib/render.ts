import type { ReactNode } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";

export function renderRoot(rootApp: ReactNode) {
  if (!globalThis.document) {
    return;
  }
  const elem = document.getElementById("root")!;

  if (import.meta.hot) {
    const root = (import.meta.hot.data.root ??= createRoot(elem));
    root.render(rootApp);
  } else {
    createRoot(elem).render(rootApp);
  }
}
