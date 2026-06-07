import { invoke } from "@tauri-apps/api/core";
import type {
  SystemConfig,
  MonitorInfo,
  WallpaperManifest,
} from "./WallpaperEditor.types";

export async function readConfig(): Promise<SystemConfig> {
  return invoke("read_system_config");
}

export async function writeConfig(data: SystemConfig): Promise<void> {
  await invoke("write_system_config", { data });
}

export async function openConfigFile(): Promise<void> {
  await invoke("open_config_file");
}

export async function openWallpapersDir(): Promise<void> {
  await invoke("open_wallpapers_dir");
}

export async function listMonitors(): Promise<MonitorInfo[]> {
  return invoke("list_monitors");
}

export async function wallpapers(): Promise<Map<string, WallpaperManifest>> {
  const result =
    await invoke<Record<string, WallpaperManifest>>("list_wallpapers");
  return new Map(Object.entries(result));
}

export async function getAutostart(): Promise<boolean> {
  return invoke("get_autostart");
}

export async function setAutostart(enabled: boolean): Promise<void> {
  await invoke("set_autostart", { enabled });
}

export async function pickFile(extensions?: string[]): Promise<string | null> {
  return invoke("pick_file", { extensions });
}

export async function pickDirectory(): Promise<string | null> {
  return invoke("pick_directory");
}

// ─── Wallpaper install ────────────────────────────────────────────────────────

export async function installWallpaper(args: {
  name: string;
  zipUrl: string;
  installId: string;
}): Promise<void> {
  await invoke("install_wallpaper", {
    name: args.name,
    zipUrl: args.zipUrl,
    installId: args.installId,
  });
}
