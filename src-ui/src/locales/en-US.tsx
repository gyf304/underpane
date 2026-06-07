const messages = {
  "status.idle": "No changes",
  "status.unsaved": "Unsaved changes",
  "status.saving": "Saving…",
  "status.saved": "Saved",
  "status.error": "Save failed",

  "editor.loading": "Loading…",
  "editor.monitors": "Monitors",
  "editor.generalSettings": "General Settings",
  "editor.save": "Save",
  "editor.selectMonitor": "Select a monitor to configure.",
  "editor.wallpaper": "Wallpaper",
  "editor.none": "None",
  "editor.noFields": "No configurable fields.",
  "editor.chooseFile": "Choose file…",
  "editor.noFileChosen": "No file chosen",
  "editor.chooseFolder": "Choose folder…",
  "editor.noFolderChosen": "No folder chosen",

  "general.wallpapersDir.label": "Wallpapers directory",
  "general.wallpapersDir.desc":
    "Filesystem path where wallpapers are loaded from.",
  "general.configFile.label": "Config file",
  "general.configFile.desc": "Open the config file in your default editor.",
  "general.configFile.button": "Open config file",
  "general.wallpapersFolder.label": "Wallpapers folder",
  "general.wallpapersFolder.desc":
    "Open the wallpapers directory in your file browser.",
  "general.wallpapersFolder.button": "Open wallpapers folder",

  "general.autostart.label": "Launch at login",
  "general.autostart.desc": "Automatically start Underpane when you log in.",

  "general.quickstart.label": "Quickstart",
  "general.quickstart.desc": "Replay the welcome tour.",
  "general.quickstart.button": "Show quickstart",

  "quickstart.skip": "Skip",
  "quickstart.back": "Back",
  "quickstart.next": "Next",
  "quickstart.done": "Done",

  "quickstart.welcome.title": "Welcome to Underpane",
  "quickstart.welcome.desc":
    "Underpane lets you set a live wallpaper for each of your monitors. Here's a quick tour.",
  "quickstart.monitors.title": "1. Pick a monitor",
  "quickstart.monitors.desc":
    "Click a monitor in the left panel to configure it. The top of the panel shows the arrangement of your monitor layout for reference. Pick \u201c*\u201d to apply one wallpaper to every monitor at once.",
  "quickstart.wallpaper.title": "2. Choose a wallpaper",
  "quickstart.wallpaper.desc":
    "With a monitor selected, pick a wallpaper from the dropdown (or \u201cNone\u201d to clear it).",
  "quickstart.config.title": "3. Adjust settings",
  "quickstart.config.desc":
    "Each wallpaper exposes its own options: toggles, sliders, and inputs grouped by category. Click Save in the top-right toolbar to apply your changes.",
  "quickstart.addWallpapers.title": "4. Add more wallpapers",
  "quickstart.addWallpapers.desc":
    "Open General Settings (bottom-left), then click \u201cOpen wallpapers folder\u201d and drop new wallpaper folders inside to install them.",
  "quickstart.autostart.title": "5. Launch at login",
  "quickstart.autostart.desc":
    "Still in General Settings, turn on \u201cLaunch at login\u201d so Underpane starts automatically, then Save to apply it.",
  "quickstart.done.title": "You're all set",
  "quickstart.done.desc":
    "That's everything. Welcome to Underpane! You can revisit this quickstart anytime from General Settings.",

  "install.title": "Install wallpaper",
  "install.from": ({ url }: { url: string }) => (
    <>
      From: <code className="text-xs">{url}</code>
    </>
  ),
  "install.folder.label": "Install to folder",
  "install.error.nameRequired": "Name required",
  "install.error.nameInvalid":
    "Allowed: lowercase letters, digits, '-' (cannot start or end with '-')",
  "install.cancel": "Cancel",
  "install.install": "Install",
  "install.installing": ({ name }: { name: string }) => (
    <>
      Installing <b>{name}</b>\u2026
    </>
  ),
  "install.validating": "Validating\u2026",
  "install.installed": ({ name }: { name: string }) => (
    <>
      Installed <b>{name}</b>.
    </>
  ),
  "install.close": "Close",
  "install.retry": "Retry",
  "install.filesSuffix": ({ done, total }: { done: number; total: number }) =>
    ` \u00b7 ${done}/${total} files`,
};

export default messages;
