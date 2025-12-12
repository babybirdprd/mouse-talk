//! Native Windows overlay for live transcription preview
//! 
//! Uses Windows API directly for reliable show/hide/show pattern

#![cfg(windows)]

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use winapi::um::winuser::*;
use winapi::um::wingdi::*;
use winapi::um::dwmapi::*;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::shared::windef::*;
use winapi::shared::minwindef::*;

const CLASS_NAME: &str = "MouseTalkOverlay";
// Reduced header height since we removed the distinct bar, this is just top padding now
const HEADER_HEIGHT: i32 = 20; 
const WIDTH: i32 = 300; // Increased width slightly to be proportional
const MIN_HEIGHT: i32 = 300; // Make it square-ish by default
const MAX_HEIGHT: i32 = 600;

// Transparent color key - this color will be fully transparent
const TRANSPARENT_COLOR: u32 = 0x00FF00FF; // Magenta (RGB 255, 0, 255)

// Custom message for updating text
const WM_UPDATE_TEXT: UINT = WM_USER + 1;

/// Convert Rust string to wide string for Windows API
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Shared state for the overlay
pub struct OverlayState {
    pub text: String,
    pub should_close: bool,
    hwnd: HWND, // Use raw pointer, null if not created
}

unsafe impl Send for OverlayState {}
unsafe impl Sync for OverlayState {}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            text: String::new(),
            should_close: false,
            hwnd: ptr::null_mut(),
        }
    }
}

/// Controller for the overlay
pub struct OverlayController {
    state: Arc<Mutex<OverlayState>>,
    is_running: Arc<AtomicBool>,
}

impl OverlayController {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(OverlayState::default())),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Show the overlay
    pub fn show(&self) {
        if self.is_running.load(Ordering::SeqCst) {
            return;
        }
        
        // Reset state
        if let Ok(mut state) = self.state.lock() {
            state.text.clear();
            state.should_close = false;
            state.hwnd = ptr::null_mut();
        }
        
        self.is_running.store(true, Ordering::SeqCst);
        let state = self.state.clone();
        let is_running = self.is_running.clone();
        
        thread::spawn(move || {
            unsafe { run_overlay(state, is_running) };
        });
    }
    
    /// Hide the overlay (non-blocking)
    pub fn hide(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.should_close = true;
            let hwnd = state.hwnd;
            if !hwnd.is_null() {
                unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
            }
        }
        // Don't wait - let it close asynchronously
    }
    
    /// Append text (thread-safe via message)
    pub fn append_text(&self, text: &str) {
        if let Ok(mut state) = self.state.lock() {
            if !state.text.is_empty() {
                state.text.push(' ');
            }
            state.text.push_str(text);
            let hwnd = state.hwnd;
            if !hwnd.is_null() {
                // Post message to trigger redraw on window thread
                unsafe { PostMessageW(hwnd, WM_UPDATE_TEXT, 0, 0); }
            }
        }
    }
    
    pub fn set_text(&self, text: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.text = text.to_string();
            let hwnd = state.hwnd;
            if !hwnd.is_null() {
                unsafe { PostMessageW(hwnd, WM_UPDATE_TEXT, 0, 0); }
            }
        }
    }
    
    pub fn get_text(&self) -> String {
        self.state.lock().map(|s| s.text.clone()).unwrap_or_default()
    }
}

/// Calculate required height based on text
unsafe fn calculate_height(hwnd: HWND, text: &str) -> i32 {
    if text.is_empty() {
        return MIN_HEIGHT;
    }
    
    let hdc = GetDC(hwnd);
    let font = CreateFontW(
        -18, 0, 0, 0, FW_NORMAL, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    let old_font = SelectObject(hdc, font as *mut _);
    
    let wide_text = to_wide(text);
    
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: WIDTH - 60, // 30px padding each side
        bottom: 0,
    };
    
    DrawTextW(hdc, wide_text.as_ptr(), -1, &mut rect, DT_CALCRECT | DT_WORDBREAK);
    
    SelectObject(hdc, old_font);
    DeleteObject(font as *mut _);
    ReleaseDC(hwnd, hdc);
    
    let text_height = rect.bottom - rect.top;
    
    // Vertical centering logic implies we need enough space
    // But for dynamic resizing, we just ensure it grows if needed.
    // Base padding makes it look good.
    let total = text_height + 100; // ample vertical padding
    total.max(MIN_HEIGHT).min(MAX_HEIGHT)
}

/// Window procedure
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_UPDATE_TEXT => {
            // Get text from state and redraw
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Mutex<OverlayState>;
            if !state_ptr.is_null() {
                let state = &*state_ptr;
                if let Ok(s) = state.lock() {
                    // Resize window
                    let height = calculate_height(hwnd, &s.text);
                    let mut rect: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut rect);
                    // Keep center position roughly
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, WIDTH, height, SWP_NOMOVE | SWP_NOZORDER);
                }
            }
            InvalidateRect(hwnd, ptr::null(), 1);
            0
        }
        WM_PAINT => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Mutex<OverlayState>;
            if !state_ptr.is_null() {
                let state = &*state_ptr;
                if let Ok(s) = state.lock() {
                    paint_window(hwnd, &s.text);
                }
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Paint the window
unsafe fn paint_window(hwnd: HWND, text: &str) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    
    // Reference image has large rounded corners
    let corner_radius = 24;
    
    // 1. Setup Transparency Key
    let trans_brush = CreateSolidBrush(TRANSPARENT_COLOR);
    FillRect(hdc, &rect, trans_brush);
    DeleteObject(trans_brush as *mut _);
    
    // 3. Main background fill
    // Purple-tinted frosted glass appearance (fully opaque)
    let bg_color = RGB(45, 40, 65); 
    let bg_brush = CreateSolidBrush(bg_color);
    let rounded_region = CreateRoundRectRgn(0, 0, rect.right + 1, rect.bottom + 1, corner_radius, corner_radius);
    
    SelectClipRgn(hdc, rounded_region);
    FillRect(hdc, &rect, bg_brush);
    DeleteObject(bg_brush as *mut _);

    // 4. Border
    // Subtle border
    let border_pen = CreatePen(PS_SOLID as i32, 1, RGB(80, 70, 100));
    let null_brush = GetStockObject(NULL_BRUSH as i32);
    SelectObject(hdc, border_pen as *mut _);
    SelectObject(hdc, null_brush);
    RoundRect(hdc, 0, 0, rect.right, rect.bottom, corner_radius, corner_radius);
    DeleteObject(border_pen as *mut _);
    
    // 5. Sparkle Icon (Top Right)
    // Use Unicode instead of manual drawing for better quality
    let icon_font = CreateFontW(
        -24, 0, 0, 0, FW_NORMAL, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI Symbol").as_ptr(),
    );
    SelectObject(hdc, icon_font as *mut _);
    SetTextColor(hdc, RGB(200, 180, 255));
    SetBkMode(hdc, TRANSPARENT as i32);
    
    // Draw star/sparkle in top right
    let icon_text = to_wide("✦"); // ✦ or ✨
    let mut icon_rect = RECT {
        left: rect.right - 40,
        top: 20,
        right: rect.right - 10,
        bottom: 50,
    };
    DrawTextW(hdc, icon_text.as_ptr(), -1, &mut icon_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    DeleteObject(icon_font as *mut _);
    
    // 6. Main Transcription Text
    let font = CreateFontW(
        -18, 0, 0, 0, FW_NORMAL, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    SelectObject(hdc, font as *mut _);
    // Brighter text
    SetTextColor(hdc, RGB(255, 255, 255));
    
    let display_text = if text.is_empty() { "Listening..." } else { text };
    let wide_text = to_wide(display_text);
    
    let mut text_rect = RECT {
        left: 30,
        top: 30, 
        right: rect.right - 30,
        bottom: rect.bottom - 30,
    };
    
    // Standard left-aligned text, no vertical centering gimmicks
    DrawTextW(hdc, wide_text.as_ptr(), -1, &mut text_rect, DT_LEFT | DT_WORDBREAK);
    
    DeleteObject(font as *mut _);
    
    // Cleanup
    SelectClipRgn(hdc, ptr::null_mut());
    DeleteObject(rounded_region as *mut _);
    
    EndPaint(hwnd, &ps);
}

/// Run the overlay window
unsafe fn run_overlay(state: Arc<Mutex<OverlayState>>, is_running: Arc<AtomicBool>) {
    let hinstance = GetModuleHandleW(ptr::null());
    let class_name = to_wide(CLASS_NAME);
    
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: ptr::null_mut(),
        hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };
    
    RegisterClassExW(&wc); // Ignore error if already registered
    
    let screen_width = GetSystemMetrics(SM_CXSCREEN);
    let screen_height = GetSystemMetrics(SM_CYSCREEN);
    let x = (screen_width - WIDTH) / 2;
    let y = screen_height / 6;
    
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class_name.as_ptr(),
        to_wide("Transcription").as_ptr(),
        WS_POPUP,
        x, y, WIDTH, MIN_HEIGHT,
        ptr::null_mut(),
        ptr::null_mut(),
        hinstance,
        ptr::null_mut(),
    );
    
    if hwnd.is_null() {
        is_running.store(false, Ordering::SeqCst);
        return;
    }
    
    // Use color key to make corners transparent, alpha 255 = fully opaque
    // Only the magenta color key will be transparent (for rounded corners)
    SetLayeredWindowAttributes(hwnd, TRANSPARENT_COLOR, 255, LWA_COLORKEY | LWA_ALPHA);
    
    // Enable DWM blur behind window for glassmorphic effect
    let mut blur_behind: DWM_BLURBEHIND = std::mem::zeroed();
    blur_behind.dwFlags = DWM_BB_ENABLE | DWM_BB_BLURREGION;
    blur_behind.fEnable = 1; // TRUE
    // Create a rounded region for blur
    blur_behind.hRgnBlur = CreateRoundRectRgn(0, 0, WIDTH + 1, MAX_HEIGHT + 1, 16, 16);
    DwmEnableBlurBehindWindow(hwnd, &blur_behind);
    // Clean up the region (DWM makes a copy)
    if !blur_behind.hRgnBlur.is_null() {
        DeleteObject(blur_behind.hRgnBlur as *mut _);
    }
    
    // Store state pointer
    let state_ptr = Arc::into_raw(state.clone());
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
    
    // Store hwnd in state
    if let Ok(mut s) = state.lock() {
        s.hwnd = hwnd;
    }
    
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    
    // Message loop
    let mut msg: MSG = std::mem::zeroed();
    loop {
        // Check close flag first
        if state.lock().map(|s| s.should_close).unwrap_or(true) {
            break;
        }
        
        // Process pending messages
        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                if let Ok(mut s) = state.lock() {
                    s.should_close = true;
                }
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        
        // Small sleep to prevent CPU spin
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    
    DestroyWindow(hwnd);
    
    // Cleanup
    let _ = Arc::from_raw(state_ptr);
    is_running.store(false, Ordering::SeqCst);
}