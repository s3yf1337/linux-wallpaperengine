//! Shared runtime state (index + settings + engine child).
//! Used by Tauri invoke handlers and the headless HTTP daemon.

use crate::index::Wallpaper;
use crate::settings::PlaybackSettings;
use std::sync::Mutex;
use tokio::process::Child;
use tokio::sync::Mutex as AsyncMutex;

pub struct SharedState {
    pub items: Mutex<Vec<Wallpaper>>,
    pub settings: Mutex<PlaybackSettings>,
    pub current_id: Mutex<Option<String>>,
    pub child: AsyncMutex<Option<Child>>,
}

impl SharedState {
    pub fn new() -> Self {
        let items = crate::index::load_cache();
        let settings = crate::settings::load();
        Self {
            items: Mutex::new(items),
            settings: Mutex::new(settings),
            current_id: Mutex::new(None),
            child: AsyncMutex::new(None),
        }
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}
