use arboard::Clipboard;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};

use alacritty_terminal::{
    grid::Scroll,
    index::{Column, Direction, Line, Point},
    selection::{Selection, SelectionType},
};

use super::{TerminalCursorBlink, TerminalIo, TerminalState, reset_cursor_blink};

pub fn mouse_wheel_system(
    mut evr_wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut terminal: ResMut<TerminalState>,
    mut blink: ResMut<TerminalCursorBlink>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    if ctrl {
        let mut changed = false;

        for event in evr_wheel.read() {
            if event.y > 0.0 {
                changed |= terminal.zoom_in();
            } else if event.y < 0.0 {
                changed |= terminal.zoom_out();
            }
        }

        if changed {
            terminal.mark_all_rows_dirty();
            reset_cursor_blink(&mut blink);
        }

        return;
    }

    let mut scroll_lines: i32 = 0;

    for event in evr_wheel.read() {
        match event.unit {
            MouseScrollUnit::Line => {
                scroll_lines += event.y as i32;
            }
            MouseScrollUnit::Pixel => {
                scroll_lines += (event.y / terminal.cell_height_px).round() as i32;
            }
        }
    }

    if scroll_lines == 0 {
        return;
    }

    {
        let mut term = terminal.term.lock();
        term.scroll_display(Scroll::Delta(scroll_lines));
    }

    terminal.dirty = true;
}

pub fn mouse_input_system(
    buttons: Res<ButtonInput<MouseButton>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    mut evr_mouse: MessageReader<CursorMoved>,
    mut terminal: ResMut<TerminalState>,
) {
    let Ok(window) = window_q.single() else {
        return;
    };

    let mut latest_pos = None;
    for ev in evr_mouse.read() {
        latest_pos = Some(ev.position);
    }

    let Some(pos) = latest_pos else {
        return;
    };

    let point = cursor_to_terminal_point(window, pos, &terminal);

    if buttons.just_pressed(MouseButton::Left) {
        terminal.selection_anchor = Some(point);
        terminal.selection = Some(Selection::new(
            SelectionType::Simple,
            point,
            Direction::Left,
        ));
        terminal.dirty = true;
    }

    if buttons.pressed(MouseButton::Left) {
        if let Some(selection) = terminal.selection.as_mut() {
            selection.update(point, Direction::Right);
            terminal.dirty = true;
        }
    }

    if buttons.just_released(MouseButton::Left) {
        terminal.dirty = true;
    }
}

fn cursor_to_terminal_point(_window: &Window, cursor_pos: Vec2, terminal: &TerminalState) -> Point {
    let x = (cursor_pos.x - 12.0).max(0.0);
    let y = (cursor_pos.y - 12.0).max(0.0);

    let col = (x / terminal.cell_width_px).floor() as usize;
    let row = (y / terminal.cell_height_px).floor() as usize;

    let row = row.min(terminal.rows.saturating_sub(1));
    let col = col.min(terminal.cols.saturating_sub(1));

    Point::new(Line(row as i32), Column(col))
}

pub fn copy_selection_system(keys: Res<ButtonInput<KeyCode>>, terminal: ResMut<TerminalState>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    if !ctrl || !keys.just_pressed(KeyCode::KeyC) {
        return;
    }

    let Some(_selection) = terminal.selection.as_ref() else {
        return;
    };

    let term = terminal.term.lock();

    if let Some(text) = term.selection_to_string() {
        if !text.is_empty() {
            if let Ok(mut clipboard) = Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    }
}

pub fn keyboard_input_system(
    mut evr_key: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    io: Option<Res<TerminalIo>>,
    mut terminal: ResMut<TerminalState>,
    mut blink: ResMut<TerminalCursorBlink>,
) {
    let Some(io) = io else {
        return;
    };

    for event in evr_key.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }

        let mut bytes = None;

        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

        if ctrl {
            let mut zoom_changed = false;
            match &event.logical_key {
                Key::Character(ch) if ch == "+" => {
                    zoom_changed = terminal.zoom_in();
                }
                Key::Character(ch) if ch == "=" => {
                    zoom_changed = terminal.zoom_in();
                }
                Key::Character(ch) if ch == "-" => {
                    zoom_changed = terminal.zoom_out();
                }
                Key::Character(ch) if ch.eq_ignore_ascii_case("v") => {
                    if let Ok(mut cb) = Clipboard::new() {
                        if let Ok(text) = cb.get_text() {
                            bytes = Some(text.into_bytes());
                        }
                    }
                }
                Key::Character(ch) if ch.eq_ignore_ascii_case("c") => {
                    bytes = Some(vec![0x03]);
                }
                _ => {}
            }
            if zoom_changed {
                terminal.mark_all_rows_dirty();
                reset_cursor_blink(&mut blink);
                continue;
            }
        }

        if bytes.is_none() {
            bytes = key_event_to_bytes(event);
        }

        if let Some(bytes) = bytes {
            let _ = io.tx.send(bytes);
            reset_cursor_blink(&mut blink);
        }
    }
}

fn key_event_to_bytes(event: &KeyboardInput) -> Option<Vec<u8>> {
    match &event.logical_key {
        Key::Enter => Some(b"\r".to_vec()),
        Key::Tab => Some(b"\t".to_vec()),
        Key::Backspace => Some(vec![0x7f]),
        Key::Escape => Some(vec![0x1b]),

        Key::ArrowUp => Some(b"\x1b[A".to_vec()),
        Key::ArrowDown => Some(b"\x1b[B".to_vec()),
        Key::ArrowRight => Some(b"\x1b[C".to_vec()),
        Key::ArrowLeft => Some(b"\x1b[D".to_vec()),

        Key::Home => Some(b"\x1b[H".to_vec()),
        Key::End => Some(b"\x1b[F".to_vec()),
        Key::PageUp => Some(b"\x1b[5~".to_vec()),
        Key::PageDown => Some(b"\x1b[6~".to_vec()),
        Key::Delete => Some(b"\x1b[3~".to_vec()),
        Key::Insert => Some(b"\x1b[2~".to_vec()),

        Key::Space => Some(" ".as_bytes().to_vec()),
        Key::Character(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}
