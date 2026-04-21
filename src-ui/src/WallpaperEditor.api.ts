import type { SystemConfig, MonitorInfo, WallpaperManifest } from "./WallpaperEditor.types";

const sleep = (ms: number) => new Promise<void>(r => setTimeout(r, ms));

export async function readConfig(): Promise<SystemConfig> {
  await sleep(220);
  return {
    monitors: {
      "1": { wallpaper: "ocean-test", config: { debug: true, wave_speed: 5 } },
      "2": { wallpaper: "forest-walk", config: { intensity: 75, theme: "emerald" } },
      "3": { wallpaper: null, config: {} },
      "default": { wallpaper: "forest-walk", config: {} },
    },
  };
}

export async function writeConfig(data: SystemConfig): Promise<void> {
  await sleep(650);
  console.log("[mock] writeConfig:", JSON.stringify(data, null, 2));
}

export async function listMonitors(): Promise<MonitorInfo[]> {
  await sleep(150);
  return [
    { id: "1", position: { x: 1920, y: 0 }, size: { width: 2560, height: 1440 } },
    { id: "2", position: { x: 4480, y: 180 }, size: { width: 1920, height: 1080 } },
    { id: "3", position: { x: 0, y: 360 }, size: { width: 1920, height: 1080 } },
  ];
}

export async function wallpapers(): Promise<Map<string, WallpaperManifest>> {
  await sleep(180);
  return new Map([
    [
      "ocean-test",
      {
        name: "Ocean Test",
        config: {
          debug: {
            name: "Debug",
            group: "Debug",
            description: "Turn on to show debug information",
            type: "bool",
          },
          wave_speed: {
            name: "Wave Speed",
            group: "Animation",
            description: "Controls wave animation speed (0–10)",
            type: "number",
            default: 3,
            min: 0,
            max: 10,
            step: 0.5,
          },
          palette: {
            name: "Color Palette",
            group: "Visual",
            description: "Named palette for ocean colors",
            type: "string",
            default: "deep-ocean",
          },
        },
      },
    ],
    [
      "forest-walk",
      {
        name: "Forest Walk",
        config: {
          intensity: {
            name: "Effect Intensity",
            group: "Visual",
            description: "Controls the animation intensity (0–100)",
            type: "number",
            default: 50,
            min: 0,
            max: 100,
            step: 1,
          },
          theme: {
            name: "Color Theme",
            group: "Visual",
            description: "Pick a color theme",
            type: "string",
            default: "green",
          },
        },
      },
    ],
  ] as [string, WallpaperManifest][]);
}
