//! Native Windows notification toast
//! 
//! Distinct from the main transcription overlay.
//! Positioned in Top-Right corner.
//! Smaller, transient.

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
use winapi::um::uxtheme::MARGINS;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::shared::windef::*;
use winapi::shared::minwindef::*;
use winapi::ctypes::c_void;

const CLASS_NAME: &str = "MouseTalkNotification";
const WIDTH: i32 = 250; 
const HEIGHT: i32 = 80; // Fixed small height
const PADDING: i32 = 20;

const TRANSPARENT_COLOR: u32 = 0x00FF00FF; // Magenta

// Custom message for updating text
const WM_UPDATE_TEXT: UINT = WM_USER + 1;

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub struct NotificationState {
    pub text: String,
    pub title: String,
    pub should_close: bool,
    hwnd: HWND,
}

unsafe impl Send for NotificationState {}
unsafe impl Sync for NotificationState {}

impl Default for NotificationState {
    fn default() -> Self {
        Self {
            text: String::new(),
            title: String::new(),
            should_close: false,
            hwnd: ptr::null_mut(),
        }
    }
}

pub struct NotificationController {
    state: Arc<Mutex<NotificationState>>,
    is_running: Arc<AtomicBool>,
}

impl NotificationController {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NotificationState::default())),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Show a notification with title and text for a specific duration
    pub fn show(&self, title: &str, text: &str, duration_ms: u64) {
        // Update state
        if let Ok(mut state) = self.state.lock() {
            state.title = title.to_string();
            state.text = text.to_string();
            state.should_close = false;
        }

        // If not running, start thread
        if !self.is_running.swap(true, Ordering::SeqCst) {
            let state = self.state.clone();
            let is_running = self.is_running.clone();
            
            thread::spawn(move || {
                unsafe { run_notification(state, is_running) };
            });
        } else {
             // If already running, just force a repaint/update
             if let Ok(state) = self.state.lock() {
                 if !state.hwnd.is_null() {
                     unsafe { 
                        PostMessageW(state.hwnd, WM_UPDATE_TEXT, 0, 0); 
                        // Cancel any pending auto-close? 
                        // Simplified: The original thread will still close it, or we rely on main.rs logic to control timing?
                        // For "Show for duration", we should probably handle timing here or in main.
                        // Ideally, we just show it, and rely on the caller to call hide, OR we implement a timer reset.
                        // For this implementation, let's keep it simple: Main controls lifetime via show/hide pattern or separate thread sleep
                     }
                 }
             }
        }

        // Helper: Spawn a closer thread if duration provided
        if duration_ms > 0 {
            let controller_clone = self.clone(); // Need to implement Clone or just Arc wrapper
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(duration_ms));
                controller_clone.hide();
            });
        }
    }
    
    // Manual clone helper since we wrap Arcs
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            is_running: self.is_running.clone(),
        }
    }

    pub fn hide(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.should_close = true;
            let hwnd = state.hwnd;
            if !hwnd.is_null() {
                unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_UPDATE_TEXT => {
            InvalidateRect(hwnd, ptr::null(), 1);
            0
        }
        WM_PAINT => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Mutex<NotificationState>;
            if !state_ptr.is_null() {
                let state = &*state_ptr;
                if let Ok(s) = state.lock() {
                    paint_window(hwnd, &s.title, &s.text);
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

unsafe fn paint_window(hwnd: HWND, title: &str, text: &str) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    
    let corner_radius = 16; 
    
    // Clear background
    let trans_brush = CreateSolidBrush(TRANSPARENT_COLOR);
    FillRect(hdc, &rect, trans_brush);
    DeleteObject(trans_brush as *mut _);
    
    // Background - Darker, slight distinct tint (e.g. Dark Purple/Blue)
    let bg_color = RGB(30, 25, 40);
    let bg_brush = CreateSolidBrush(bg_color);
    let rounded_region = CreateRoundRectRgn(0, 0, rect.right + 1, rect.bottom + 1, corner_radius, corner_radius);
    
    SelectClipRgn(hdc, rounded_region);
    FillRect(hdc, &rect, bg_brush);
    DeleteObject(bg_brush as *mut _);

    // Border
    let border_pen = CreatePen(PS_SOLID as i32, 1, RGB(100, 90, 140));
    let null_brush = GetStockObject(NULL_BRUSH as i32);
    SelectObject(hdc, border_pen as *mut _);
    SelectObject(hdc, null_brush);
    RoundRect(hdc, 0, 0, rect.right, rect.bottom, corner_radius, corner_radius);
    DeleteObject(border_pen as *mut _);
    
    // Title
    let title_font = CreateFontW(
        -16, 0, 0, 0, FW_BOLD, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    SelectObject(hdc, title_font as *mut _);
    SetTextColor(hdc, RGB(220, 210, 255));
    SetBkMode(hdc, TRANSPARENT as i32);
    
    let mut title_rect = RECT { left: 15, top: 10, right: rect.right - 10, bottom: 30 };
    let wide_title = to_wide(title);
    DrawTextW(hdc, wide_title.as_ptr(), -1, &mut title_rect, DT_LEFT | DT_SINGLELINE);
    DeleteObject(title_font as *mut _);

    // Text
    let text_font = CreateFontW(
        -14, 0, 0, 0, FW_NORMAL, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    SelectObject(hdc, text_font as *mut _);
    SetTextColor(hdc, RGB(200, 200, 220));
    
    let mut text_rect = RECT { left: 15, top: 35, right: rect.right - 10, bottom: rect.bottom - 10 };
    let wide_text = to_wide(text);
    DrawTextW(hdc, wide_text.as_ptr(), -1, &mut text_rect, DT_LEFT | DT_WORDBREAK);
    DeleteObject(text_font as *mut _);
    
    SelectClipRgn(hdc, ptr::null_mut());
    DeleteObject(rounded_region as *mut _);
    EndPaint(hwnd, &ps);
}

unsafe fn run_notification(state: Arc<Mutex<NotificationState>>, is_running: Arc<AtomicBool>) {
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
    
    RegisterClassExW(&wc);
    
    // Position: Bottom Right, above taskbar usually
    let screen_width = GetSystemMetrics(SM_CXSCREEN);
    let screen_height = GetSystemMetrics(SM_CYSCREEN);
    let x = screen_width - WIDTH - PADDING;
    let y = screen_height - HEIGHT - PADDING - 40; // Approx taskbar height
    
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class_name.as_ptr(),
        to_wide("Notification").as_ptr(),
        WS_POPUP,
        x, y, WIDTH, HEIGHT,
        ptr::null_mut(),
        ptr::null_mut(),
        hinstance,
        ptr::null_mut(),
    );
    
    if hwnd.is_null() {
        is_running.store(false, Ordering::SeqCst);
        return;
    }
    
    SetLayeredWindowAttributes(hwnd, TRANSPARENT_COLOR, 230, LWA_COLORKEY | LWA_ALPHA);
    
    // Glass effect (reused)
    let mut blur_behind: DWM_BLURBEHIND = std::mem::zeroed();
    blur_behind.dwFlags = DWM_BB_ENABLE | DWM_BB_BLURREGION;
    blur_behind.fEnable = 1;
    blur_behind.hRgnBlur = CreateRoundRectRgn(0, 0, WIDTH + 1, HEIGHT + 1, 16, 16);
    DwmEnableBlurBehindWindow(hwnd, &blur_behind);
    if !blur_behind.hRgnBlur.is_null() { DeleteObject(blur_behind.hRgnBlur as *mut _); }

    let state_ptr = Arc::into_raw(state.clone());
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
     if let Ok(mut s) = state.lock() { s.hwnd = hwnd; }
    
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    
    let mut msg: MSG = std::mem::zeroed();
    loop {
        if state.lock().map(|s| s.should_close).unwrap_or(true) { break; }
        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                if let Ok(mut s) = state.lock() { s.should_close = true; }
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    
    DestroyWindow(hwnd);
    let _ = Arc::from_raw(state_ptr);
    is_running.store(false, Ordering::SeqCst);
}
