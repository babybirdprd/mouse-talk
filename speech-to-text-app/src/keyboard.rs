use rdev::{simulate, EventType, Key};
use std::{thread, time};

/// Normalize unicode punctuation to ASCII equivalents
fn normalize_text(text: &str) -> String {
    text.chars().map(|c| match c {
        // Chinese punctuation to English
        '，' => ',',
        '。' => '.',
        '？' => '?',
        '！' => '!',
        '：' => ':',
        '；' => ';',
        '"' | '"' | '「' | '」' => '"',
        '\u{2018}' | '\u{2019}' => '\'',
        '（' => '(',
        '）' => ')',
        '【' => '[',
        '】' => ']',
        '—' | '–' => '-',
        '…' => '.',
        _ => c,
    }).collect()
}

/// Type out text character by character using simulated keyboard input
/// Returns the number of characters actually typed
pub fn type_text(text: &str) -> usize {
    let normalized = normalize_text(text);
    let mut count = 0;
    for c in normalized.chars() {
        if let Some((key, needs_shift)) = char_to_key(c) {
            send_key(key, needs_shift);
            count += 1;
        }
        // Silently skip unsupported characters
    }
    count
}

/// Delete characters by sending backspace keys
pub fn delete_chars(count: usize) {
    // Small pause before starting deletion to ensure typing has stopped
    thread::sleep(time::Duration::from_millis(50));
    for _ in 0..count {
        let _ = simulate(&EventType::KeyPress(Key::Backspace));
        let _ = simulate(&EventType::KeyRelease(Key::Backspace));
        thread::sleep(time::Duration::from_millis(10));
    }
    // Small pause after deletion before typing new text
    thread::sleep(time::Duration::from_millis(50));
}

fn send_key(key: Key, shift: bool) {
    if shift {
        let _ = simulate(&EventType::KeyPress(Key::ShiftLeft));
    }
    let _ = simulate(&EventType::KeyPress(key));
    let _ = simulate(&EventType::KeyRelease(key));
    if shift {
        let _ = simulate(&EventType::KeyRelease(Key::ShiftLeft));
    }
    // Small delay to ensure the OS processes it
    thread::sleep(time::Duration::from_millis(10));
}

/// Convert a character to a Key and whether shift is needed
fn char_to_key(c: char) -> Option<(Key, bool)> {
    // Check for uppercase letters first
    let needs_shift = c.is_uppercase();
    let lower = c.to_ascii_lowercase();

    match lower {
        // Letters
        'a' => Some((Key::KeyA, needs_shift)),
        'b' => Some((Key::KeyB, needs_shift)),
        'c' => Some((Key::KeyC, needs_shift)),
        'd' => Some((Key::KeyD, needs_shift)),
        'e' => Some((Key::KeyE, needs_shift)),
        'f' => Some((Key::KeyF, needs_shift)),
        'g' => Some((Key::KeyG, needs_shift)),
        'h' => Some((Key::KeyH, needs_shift)),
        'i' => Some((Key::KeyI, needs_shift)),
        'j' => Some((Key::KeyJ, needs_shift)),
        'k' => Some((Key::KeyK, needs_shift)),
        'l' => Some((Key::KeyL, needs_shift)),
        'm' => Some((Key::KeyM, needs_shift)),
        'n' => Some((Key::KeyN, needs_shift)),
        'o' => Some((Key::KeyO, needs_shift)),
        'p' => Some((Key::KeyP, needs_shift)),
        'q' => Some((Key::KeyQ, needs_shift)),
        'r' => Some((Key::KeyR, needs_shift)),
        's' => Some((Key::KeyS, needs_shift)),
        't' => Some((Key::KeyT, needs_shift)),
        'u' => Some((Key::KeyU, needs_shift)),
        'v' => Some((Key::KeyV, needs_shift)),
        'w' => Some((Key::KeyW, needs_shift)),
        'x' => Some((Key::KeyX, needs_shift)),
        'y' => Some((Key::KeyY, needs_shift)),
        'z' => Some((Key::KeyZ, needs_shift)),

        // Numbers (no shift needed for digits, shift gives symbols)
        '0' => Some((Key::Num0, false)),
        '1' => Some((Key::Num1, false)),
        '2' => Some((Key::Num2, false)),
        '3' => Some((Key::Num3, false)),
        '4' => Some((Key::Num4, false)),
        '5' => Some((Key::Num5, false)),
        '6' => Some((Key::Num6, false)),
        '7' => Some((Key::Num7, false)),
        '8' => Some((Key::Num8, false)),
        '9' => Some((Key::Num9, false)),

        // Whitespace
        ' ' => Some((Key::Space, false)),
        '\n' => Some((Key::Return, false)),
        '\t' => Some((Key::Tab, false)),

        // Punctuation (US keyboard layout)
        '.' => Some((Key::Dot, false)),
        ',' => Some((Key::Comma, false)),
        ';' => Some((Key::SemiColon, false)),
        '\'' => Some((Key::Quote, false)),
        '-' => Some((Key::Minus, false)),
        '=' => Some((Key::Equal, false)),
        '[' => Some((Key::LeftBracket, false)),
        ']' => Some((Key::RightBracket, false)),
        '\\' => Some((Key::BackSlash, false)),
        '`' => Some((Key::BackQuote, false)),
        '/' => Some((Key::Slash, false)),

        // Shifted punctuation (US keyboard layout)
        '!' => Some((Key::Num1, true)),
        '@' => Some((Key::Num2, true)),
        '#' => Some((Key::Num3, true)),
        '$' => Some((Key::Num4, true)),
        '%' => Some((Key::Num5, true)),
        '^' => Some((Key::Num6, true)),
        '&' => Some((Key::Num7, true)),
        '*' => Some((Key::Num8, true)),
        '(' => Some((Key::Num9, true)),
        ')' => Some((Key::Num0, true)),
        '_' => Some((Key::Minus, true)),
        '+' => Some((Key::Equal, true)),
        '{' => Some((Key::LeftBracket, true)),
        '}' => Some((Key::RightBracket, true)),
        '|' => Some((Key::BackSlash, true)),
        ':' => Some((Key::SemiColon, true)),
        '"' => Some((Key::Quote, true)),
        '<' => Some((Key::Comma, true)),
        '>' => Some((Key::Dot, true)),
        '?' => Some((Key::Slash, true)),
        '~' => Some((Key::BackQuote, true)),

        _ => None,
    }
}
