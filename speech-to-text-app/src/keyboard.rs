use rdev::{simulate, EventType, Key};
use std::{thread, time};

pub fn type_text(text: &str) {
    // rdev simulation can be tricky. We need to map chars to keys.
    // This is a simplified version.
    for c in text.chars() {
        if let Some(key) = char_to_key(c) {
            send_key(key, c.is_uppercase());
        } else {
             // Fallback or specialized handling for punctuation not mapped
             // This is a basic implementation.
             eprintln!("Cannot type char: {}", c);
        }
    }
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

fn char_to_key(c: char) -> Option<Key> {
    let lower = c.to_ascii_lowercase();
    match lower {
        'a' => Some(Key::KeyA),
        'b' => Some(Key::KeyB),
        'c' => Some(Key::KeyC),
        'd' => Some(Key::KeyD),
        'e' => Some(Key::KeyE),
        'f' => Some(Key::KeyF),
        'g' => Some(Key::KeyG),
        'h' => Some(Key::KeyH),
        'i' => Some(Key::KeyI),
        'j' => Some(Key::KeyJ),
        'k' => Some(Key::KeyK),
        'l' => Some(Key::KeyL),
        'm' => Some(Key::KeyM),
        'n' => Some(Key::KeyN),
        'o' => Some(Key::KeyO),
        'p' => Some(Key::KeyP),
        'q' => Some(Key::KeyQ),
        'r' => Some(Key::KeyR),
        's' => Some(Key::KeyS),
        't' => Some(Key::KeyT),
        'u' => Some(Key::KeyU),
        'v' => Some(Key::KeyV),
        'w' => Some(Key::KeyW),
        'x' => Some(Key::KeyX),
        'y' => Some(Key::KeyY),
        'z' => Some(Key::KeyZ),
        '0' => Some(Key::Num0),
        '1' => Some(Key::Num1),
        '2' => Some(Key::Num2),
        '3' => Some(Key::Num3),
        '4' => Some(Key::Num4),
        '5' => Some(Key::Num5),
        '6' => Some(Key::Num6),
        '7' => Some(Key::Num7),
        '8' => Some(Key::Num8),
        '9' => Some(Key::Num9),
        ' ' => Some(Key::Space),
        '.' => Some(Key::Dot),
        ',' => Some(Key::Comma),
        // Add more as needed
        _ => None,
    }
}
