export interface MonitorConfig {
  wallpaper: string;
  config?: Record<string, unknown>;
}

export interface SystemConfig {
  monitors?: Record<string, MonitorConfig>;
}

export interface MonitorInfo {
  id: string;
  position: { x: number; y: number; };
  size: { width: number; height: number; };
}

interface WallpaperConfigSchemaBase {
  name: string;
  group: string;
  description?: string;
}

interface WallpaperConfigSchemaBool extends WallpaperConfigSchemaBase {
  type: "bool";
  default?: boolean;
}

interface WallpaperConfigSchemaString extends WallpaperConfigSchemaBase {
  type: "string";
  default?: string;
}

interface WallpaperConfigSchemaNumber extends WallpaperConfigSchemaBase {
  type: "number";
  default?: number;
  min?: number;
  max?: number;
  step?: number;
}

export type WallpaperConfigSchema =
  | WallpaperConfigSchemaBool
  | WallpaperConfigSchemaString
  | WallpaperConfigSchemaNumber;

export interface WallpaperManifest {
  name: string;
  config: Record<string, WallpaperConfigSchema>;
}

export type SaveStatus = "idle" | "unsaved" | "saving" | "saved" | "error";
