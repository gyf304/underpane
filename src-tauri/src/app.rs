use std::sync::{LazyLock, OnceLock};

use tauri::AppHandle;

pub static APP_HANDLE_LOCK: OnceLock<AppHandle> = OnceLock::new();
pub static APP_HANDLE: LazyLock<AppHandle> =
    LazyLock::new(|| APP_HANDLE_LOCK.get().unwrap().clone());
