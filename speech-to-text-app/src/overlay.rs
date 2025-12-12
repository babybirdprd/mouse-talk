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
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::errhandlingapi::GetLastError;
use winapi::shared::windef::*;
use winapi::shared::minwindef::*;

const CLASS_NAME: &str = "MouseTalkOverlay";
const HEADER_HEIGHT: i32 = 32;
const MIN_HEIGHT: i32 = 80;
const MAX_HEIGHT: i32 = 200;
const WIDTH: i32 = 500;

/// Convert Rust string to wide string for Windows API
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Shared state for the overlay
pub struct OverlayState {
    pub text: String,
    pub should_close: bool,
    hwnd: Option<HWND>,
}

unsafe impl Send for OverlayState {}
unsafe impl Sync for OverlayState {}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            text: String::new(),
            should_close: false,
            hwnd: None,
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
            state.hwnd = None;
        }
        
        self.is_running.store(true, Ordering::SeqCst);
        let state = self.state.clone();
        let is_running = self.is_running.clone();
        
        thread::spawn(move || {
            unsafe { run_overlay(state, is_running) };
        });
    }
    
    /// Hide the overlay
    pub fn hide(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.should_close = true;
            if let Some(hwnd) = state.hwnd {
                unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
            }
        }
    }
    
    /// Append text and trigger redraw
    pub fn append_text(&self, text: &str) {
        if let Ok(mut state) = self.state.lock() {
            if !state.text.is_empty() {
                state.text.push(' ');
            }
            state.text.push_str(text);
            if let Some(hwnd) = state.hwnd {
                unsafe { 
                    // Resize and redraw
                    resize_window(hwnd, &state.text);
                    InvalidateRect(hwnd, ptr::null(), 1); 
                }
            }
        }
    }
    
    pub fn set_text(&self, text: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.text = text.to_string();
            if let Some(hwnd) = state.hwnd {
                unsafe { 
                    resize_window(hwnd, &state.text);
                    InvalidateRect(hwnd, ptr::null(), 1); 
                }
            }
        }
    }
    
    pub fn get_text(&self) -> String {
        self.state.lock().map(|s| s.text.clone()).unwrap_or_default()
    }
}

/// Calculate required height based on text
unsafe fn calculate_height(hwnd: HWND, text: &str) -> i32 {
    let hdc = GetDC(hwnd);
    
    // Create font for measurement
    let font = CreateFontW(
        -13, 0, 0, 0, FW_NORMAL, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    let old_font = SelectObject(hdc, font as *mut _);
    
    let wide_text = to_wide(text);
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: WIDTH - 40, // Account for padding
        bottom: 0,
    };
    
    DrawTextW(hdc, wide_text.as_ptr(), -1, &mut rect, DT_CALCRECT | DT_WORDBREAK);
    
    SelectObject(hdc, old_font);
    DeleteObject(font as *mut _);
    ReleaseDC(hwnd, hdc);
    
    let text_height = rect.bottom - rect.top;
    let total = HEADER_HEIGHT + text_height + 30; // Header + text + padding
    total.max(MIN_HEIGHT).min(MAX_HEIGHT)
}

/// Resize window based on text content
unsafe fn resize_window(hwnd: HWND, text: &str) {
    let height = calculate_height(hwnd, text);
    
    // Get current position
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rect);
    
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        rect.left,
        rect.top,
        WIDTH,
        height,
        SWP_NOMOVE | SWP_NOZORDER,
    );
}

/// Window procedure
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
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

/// Paint the window with styled text
unsafe fn paint_window(hwnd: HWND, text: &str) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    
    // Main background - dark glass
    let bg_color = RGB(18, 18, 28);
    let bg_brush = CreateSolidBrush(bg_color);
    FillRect(hdc, &rect, bg_brush);
    DeleteObject(bg_brush as *mut _);
    
    // Draw subtle border
    let border_color = RGB(60, 60, 80);
    let border_pen = CreatePen(PS_SOLID as i32, 1, border_color);
    let old_pen = SelectObject(hdc, border_pen as *mut _);
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH as i32));
    RoundRect(hdc, 0, 0, rect.right, rect.bottom, 16, 16);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(border_pen as *mut _);
    
    // Header bar background
    let header_rect = RECT {
        left: 1,
        top: 1,
        right: rect.right - 1,
        bottom: HEADER_HEIGHT,
    };
    let header_color = RGB(25, 25, 40);
    let header_brush = CreateSolidBrush(header_color);
    FillRect(hdc, &header_rect, header_brush);
    DeleteObject(header_brush as *mut _);
    
    // Header separator line
    let sep_pen = CreatePen(PS_SOLID as i32, 1, RGB(45, 45, 65));
    let old_sep_pen = SelectObject(hdc, sep_pen as *mut _);
    MoveToEx(hdc, 1, HEADER_HEIGHT, ptr::null_mut());
    LineTo(hdc, rect.right - 1, HEADER_HEIGHT);
    SelectObject(hdc, old_sep_pen);
    DeleteObject(sep_pen as *mut _);
    
    // "Live" indicator - green dot
    let dot_brush = CreateSolidBrush(RGB(74, 222, 128)); // Green
    let old_dot_brush = SelectObject(hdc, dot_brush as *mut _);
    let dot_pen = CreatePen(PS_SOLID as i32, 1, RGB(74, 222, 128));
    let old_dot_pen = SelectObject(hdc, dot_pen as *mut _);
    Ellipse(hdc, 16, 12, 24, 20);
    SelectObject(hdc, old_dot_brush);
    SelectObject(hdc, old_dot_pen);
    DeleteObject(dot_brush as *mut _);
    DeleteObject(dot_pen as *mut _);
    
    // "LIVE" text
    let small_font = CreateFontW(
        -10, 0, 0, 0, FW_SEMIBOLD, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    let old_small_font = SelectObject(hdc, small_font as *mut _);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, RGB(74, 222, 128));
    
    let live_text = to_wide("LIVE");
    TextOutW(hdc, 30, 10, live_text.as_ptr(), 4);
    
    SelectObject(hdc, old_small_font);
    DeleteObject(small_font as *mut _);
    
    // Main content font
    let font = CreateFontW(
        -13, 0, 0, 0, FW_LIGHT, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        to_wide("Segoe UI").as_ptr(),
    );
    let old_font = SelectObject(hdc, font as *mut _);
    SetTextColor(hdc, RGB(200, 200, 215));
    
    // Draw microphone + text
    let display = if text.is_empty() { "Listening..." } else { text };
    let full_text = format!("🎤  {}", display);
    let wide_text = to_wide(&full_text);
    
    let mut text_rect = RECT {
        left: 16,
        top: HEADER_HEIGHT + 12,
        right: rect.right - 16,
        bottom: rect.bottom - 12,
    };
    
    DrawTextW(
        hdc,
        wide_text.as_ptr(),
        -1,
        &mut text_rect,
        DT_LEFT | DT_WORDBREAK,
    );
    
    SelectObject(hdc, old_font);
    DeleteObject(font as *mut _);
    
    EndPaint(hwnd, &ps);
}

/// Run the overlay window
unsafe fn run_overlay(state: Arc<Mutex<OverlayState>>, is_running: Arc<AtomicBool>) {
    let hinstance = GetModuleHandleW(ptr::null());
    let class_name = to_wide(CLASS_NAME);
    
    // Register window class
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
    
    if RegisterClassExW(&wc) == 0 {
        let err = GetLastError();
        if err != 1410 {
            eprintln!("Failed to register window class: {}", err);
            is_running.store(false, Ordering::SeqCst);
            return;
        }
    }
    
    // Get screen size for centering
    let screen_width = GetSystemMetrics(SM_CXSCREEN);
    let screen_height = GetSystemMetrics(SM_CYSCREEN);
    let x = (screen_width - WIDTH) / 2;
    let y = screen_height / 6; // Top 1/6 of screen
    
    // Create layered window
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
        eprintln!("Failed to create window: {}", GetLastError());
        is_running.store(false, Ordering::SeqCst);
        return;
    }
    
    // Set transparency
    SetLayeredWindowAttributes(hwnd, 0, 250, LWA_ALPHA);
    
    // Store state pointer
    let state_ptr = Arc::into_raw(state.clone());
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
    
    // Store hwnd in state
    if let Ok(mut s) = state.lock() {
        s.hwnd = Some(hwnd);
    }
    
    // Show window
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    
    // Message loop
    let mut msg: MSG = std::mem::zeroed();
    loop {
        let should_close = state.lock().map(|s| s.should_close).unwrap_or(false);
        if should_close {
            DestroyWindow(hwnd);
            break;
        }
        
        if PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }
    
    // Cleanup
    let _ = Arc::from_raw(state_ptr);
    is_running.store(false, Ordering::SeqCst);
}
