//! System tray module for Mouse-Talk
//! 
//! Provides a minimal system tray icon with status display and mode toggles.

use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, CheckMenuItem};
use tray_icon::{
    menu::MenuId,
    TrayIcon, TrayIconBuilder,
    Icon,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Menu item IDs
pub const MENU_STREAMING_TOGGLE: &str = "streaming_toggle";
pub const MENU_BATCH_TOGGLE: &str = "batch_toggle";
pub const MENU_QUIT: &str = "quit";

/// Tray application state
pub struct TrayApp {
    pub _tray_icon: TrayIcon,
    pub streaming_enabled: Arc<AtomicBool>,
    pub batch_enabled: Arc<AtomicBool>,
    pub streaming_menu_item: CheckMenuItem,
    pub batch_menu_item: CheckMenuItem,
}

impl TrayApp {
    /// Create a new tray application
    pub fn new() -> anyhow::Result<Self> {
        let streaming_enabled = Arc::new(AtomicBool::new(true));
        let batch_enabled = Arc::new(AtomicBool::new(true));

        // Create menu items
        let streaming_item = CheckMenuItem::with_id(
            MenuId::new(MENU_STREAMING_TOGGLE),
            "Streaming Mode",
            true,
            true,
            None,
        );
        let batch_item = CheckMenuItem::with_id(
            MenuId::new(MENU_BATCH_TOGGLE),
            "Batch Mode",
            true,
            true,
            None,
        );
        let quit_item = MenuItem::with_id(
            MenuId::new(MENU_QUIT),
            "Quit",
            true,
            None,
        );

        // Build menu
        let menu = Menu::new();
        menu.append(&streaming_item)?;
        menu.append(&batch_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;

        // Create a simple icon (16x16 white square as fallback)
        let icon = create_default_icon();

        // Build tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Mouse-Talk - Idle")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            _tray_icon: tray_icon,
            streaming_enabled,
            batch_enabled,
            streaming_menu_item: streaming_item,
            batch_menu_item: batch_item,
        })
    }
    pub fn handle_menu_event(&self, event: &MenuEvent) -> bool {
        match event.id().0.as_str() {
            MENU_STREAMING_TOGGLE => {
                let current = self.streaming_enabled.load(Ordering::Relaxed);
                self.streaming_enabled.store(!current, Ordering::Relaxed);
                self.streaming_menu_item.set_checked(!current);
                println!("🎤 Streaming mode: {}", if !current { "enabled" } else { "disabled" });
                false
            }
            MENU_BATCH_TOGGLE => {
                let current = self.batch_enabled.load(Ordering::Relaxed);
                self.batch_enabled.store(!current, Ordering::Relaxed);
                self.batch_menu_item.set_checked(!current);
                println!("📝 Batch mode: {}", if !current { "enabled" } else { "disabled" });
                false
            }
            MENU_QUIT => {
                println!("👋 Quitting...");
                true
            }
            _ => false,
        }
    }
}

/// Create a simple default icon (microphone-like pattern)
fn create_default_icon() -> Icon {
    // 32x32 RGBA icon - simple microphone shape
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    
    // Draw a simple circle (microphone head) in the center-top
    let center_x = size / 2;
    let center_y = size / 3;
    let radius = size / 4;
    
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let dx = (x as i32 - center_x as i32).abs();
            let dy = (y as i32 - center_y as i32).abs();
            
            // Circle for mic head
            if dx * dx + dy * dy <= (radius * radius) as i32 {
                rgba[idx] = 76;      // R - teal color
                rgba[idx + 1] = 175; // G
                rgba[idx + 2] = 165; // B
                rgba[idx + 3] = 255; // A
            }
            // Stem below the circle
            else if x >= center_x - 2 && x <= center_x + 2 
                    && y > center_y + radius - 2 
                    && y < size - 4 {
                rgba[idx] = 76;
                rgba[idx + 1] = 175;
                rgba[idx + 2] = 165;
                rgba[idx + 3] = 255;
            }
        }
    }
    
    Icon::from_rgba(rgba, size as u32, size as u32).expect("Failed to create icon")
}

/// Hide the console window on Windows
#[cfg(windows)]
pub fn hide_console_window() {
    use winapi::um::wincon::GetConsoleWindow;
    use winapi::um::winuser::{ShowWindow, SW_HIDE};
    
    unsafe {
        let window = GetConsoleWindow();
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
pub fn hide_console_window() {
    // No-op on non-Windows platforms
}
