# Underpane

[English](README.md) | [简体中文](README.zh-CN.md)

Live HTML wallpapers for macOS and Windows. Underpane renders any web page as your desktop background, with per-monitor configuration.

https://github.com/user-attachments/assets/935e5452-5772-4dcb-be23-0fde652abc9b

## Features

- HTML/CSS/JS wallpapers — any web content as a desktop background
- Per-monitor wallpaper selection and configuration
- Lightweight Tauri 2 native shell
- Tray-based controls

## Wallpapers

Wallpapers live in the app's wallpaper directory (open via tray → Configure). Each wallpaper is a folder containing:

- `index.html` — the wallpaper entry point
- `index.toml` — name and exposed config schema (bool / string / number / color). Color fields store a `#rrggbb[aa]` hex string; add `alpha = true` to the schema entry to enable opacity.

The HTML must be portable: it should run standalone in a plain browser and read its config from `location.hash`.

## Development

Requirements: [Rust](https://rustup.rs), [Bun](https://bun.sh).

```sh
cargo tauri dev
```

Build a release bundle:

```sh
cargo tauri build
```

The frontend (`src-ui/`) is built with Bun; `tauri.conf.json` invokes `bun install && bun run build` automatically.

## Layout

- `src-tauri/` — Rust backend (windowing, monitor tracking, tray, config)
- `src-ui/` — TypeScript/React configuration UI
