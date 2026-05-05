# ActiveDesk

[English](README.md) | [简体中文](README.zh-CN.md)

适用于 macOS 与 Windows 的动态 HTML 壁纸。ActiveDesk 可将任意网页作为桌面背景渲染,支持按显示器分别配置。

## 特性

- HTML/CSS/JS 壁纸 —— 任意网页内容均可作为桌面背景
- 多显示器独立选择与配置壁纸
- 基于 Tauri 2 的轻量原生外壳
- 托盘菜单控制

## 壁纸

壁纸存放于应用的壁纸目录(可通过托盘菜单 → 配置 打开)。每个壁纸是一个文件夹,包含:

- `index.html` —— 壁纸入口
- `index.toml` —— 名称与可配置项(bool / string / number)

`index.html` 必须可移植:能够在普通浏览器中独立运行,并从 `location.hash` 读取配置。

## 开发

依赖:[Rust](https://rustup.rs)、[Bun](https://bun.sh)。

```sh
cargo tauri dev
```

构建发布版本:

```sh
cargo tauri build
```

前端(`src-ui/`)使用 Bun 构建,`tauri.conf.json` 会自动执行 `bun install && bun run build`。

## 目录结构

- `src-tauri/` —— Rust 后端
- `src-ui/` —— TypeScript/React 配置界面
