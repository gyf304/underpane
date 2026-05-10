use serde::Serialize;
use std::sync::LazyLock;
use std::time::Duration;
use tauri::{Monitor, PhysicalRect};
use tokio::sync::watch;

use crate::app::APP_HANDLE;

pub static MONITORS: LazyLock<watch::Receiver<Vec<Monitor>>> = LazyLock::new(|| {
    let monitors: Vec<Monitor> = APP_HANDLE.available_monitors().unwrap_or_default();
    let monitors_clone = monitors.clone();
    let (tx, rx) = watch::channel(vec![]);
    tx.send(monitors).ok();

    let app = APP_HANDLE.clone();

    tauri::async_runtime::spawn(async move {
        let mut prev_positions = monitors_clone
            .iter()
            .map(|m| Rect::from(m.work_area()))
            .collect::<Vec<_>>();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let monitors: Vec<Monitor> = app.available_monitors().unwrap_or_default();
            let positions = monitors
                .iter()
                .map(|m| Rect::from(m.work_area()))
                .collect::<Vec<_>>();
            if positions != prev_positions {
                prev_positions = positions;
                tx.send(monitors).ok();
            }
        }
    });

    rx
});

#[derive(PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

impl From<&PhysicalRect<i32, u32>> for Rect {
    fn from(value: &PhysicalRect<i32, u32>) -> Self {
        Rect {
            x: value.position.x,
            y: value.position.y,
            w: value.size.width,
            h: value.size.height,
        }
    }
}

#[derive(Serialize)]
pub struct MonitorPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize)]
pub struct MonitorSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize)]
pub struct MonitorInfo {
    pub id: String,
    pub position: MonitorPosition,
    pub size: MonitorSize,
}

pub fn current_monitors() -> Vec<MonitorInfo> {
    MONITORS
        .borrow()
        .iter()
        .enumerate()
        .map(|(i, m)| MonitorInfo {
            id: (i + 1).to_string(),
            position: MonitorPosition {
                x: m.position().x,
                y: m.position().y,
            },
            size: MonitorSize {
                width: m.size().width,
                height: m.size().height,
            },
        })
        .collect()
}
