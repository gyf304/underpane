use std::sync::{LazyLock, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Monitor, PhysicalRect};
use tokio::sync::watch;

static MONITORS_TX: OnceLock<watch::Sender<Vec<Monitor>>> = OnceLock::new();

pub static MONITORS: LazyLock<watch::Receiver<Vec<Monitor>>> = LazyLock::new(|| {
    let (tx, rx) = watch::channel(vec![]);
    MONITORS_TX.get_or_init(|| tx);
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
        Rect { x: value.position.x, y: value.position.y, w: value.size.width, h: value.size.height }
    }
}

pub fn init(app: &AppHandle) {
    let _ = &*MONITORS;

    let monitors: Vec<Monitor> = app.available_monitors().unwrap_or_default();
    let monitors_clone = monitors.clone();
    let tx = MONITORS_TX.get().unwrap();
    tx.send(monitors).ok();

    let app = app.clone();
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
}
