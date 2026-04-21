import { invoke } from "@tauri-apps/api/core";
import type { SystemConfig, MonitorInfo, WallpaperManifest } from "./WallpaperEditor.types";

export async function readConfig(): Promise<SystemConfig> {
  return invoke("read_system_config");
}

export async function writeConfig(data: SystemConfig): Promise<void> {
  await invoke("write_system_config", { data });
}

export async function listMonitors(): Promise<MonitorInfo[]> {
  return invoke("list_monitors");
}

export async function wallpapers(): Promise<Map<string, WallpaperManifest>> {
  const result = await invoke<Record<string, WallpaperManifest>>("list_wallpapers");
  return new Map(Object.entries(result));
}
