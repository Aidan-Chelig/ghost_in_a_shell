use std::{
    io::{Read, Write},
    sync::Arc,
    thread,
    time::Duration,
};

use alacritty_terminal::{
    event::{Event as AlacrittyEvent, EventListener},
    grid::Dimensions,
    index::{Boundary, Column, Direction, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::{Config as TermConfig, Term, cell::Flags},
    tty::Options as TtyOptions,
    vte::ansi::{Processor, StdSyncHandler},
};
use arboard::Clipboard;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyCode, KeyboardInput},
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[derive(Resource)]
pub struct TerminalState {
    pub term: Arc<Mutex<Term<NoopListener>>>,
    pub parser: Arc<Mutex<Processor<StdSyncHandler>>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub rx: Receiver<Vec<u8>>,
    pub cols: usize,
    pub rows: usize,
    pub cell_width_px: f32,
    pub cell_height_px: f32,
    pub selection_anchor: Option<Point>,
    pub selection: Option<Selection>,
    pub dirty: bool,
}

#[derive(Component)]
pub struct TerminalLine {
    pub row: usize,
}

#[derive(Resource, Default)]
pub struct PendingCopy(pub bool);

#[derive(Resource, Default)]
pub struct TerminalCursorBlink(pub f32);

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PendingCopy::default())
            .insert_resource(TerminalCursorBlink::default());
    }
}

#[derive(Clone, Default)]
pub struct NoopListener;

impl EventListener for NoopListener {
    fn send_event(&self, _event: AlacrittyEvent) {}
}

#[derive(Copy, Clone)]
struct SimpleSize {
    rows: usize,
    cols: usize,
}

impl Dimensions for SimpleSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

pub fn spawn_terminal_backend(commands: &mut Commands) {
    let rows = 40usize;
    let cols = 120usize;

    let size = SimpleSize { rows, cols };

    let term = Arc::new(Mutex::new(Term::new(
        TermConfig::default(),
        &size,
        NoopListener,
    )));
    let parser = Arc::new(Mutex::new(Processor::<StdSyncHandler>::new()));

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("failed to create PTY");

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");

    pair.slave
        .spawn_command(cmd)
        .expect("failed to spawn shell");

    let reader = pair
        .master
        .try_clone_reader()
        .expect("failed to clone PTY reader");

    let writer = pair.master.take_writer().expect("failed to get PTY writer");

    let (tx, rx) = unbounded::<Vec<u8>>();

    spawn_reader_thread(reader, tx);

    commands.insert_resource(TerminalState {
        term,
        parser,
        writer: Arc::new(Mutex::new(writer)),
        rx,
        cols,
        rows,
        cell_width_px: 10.8,
        cell_height_px: 20.0,
        selection_anchor: None,
        selection: None,
        dirty: true,
    });
}

fn spawn_reader_thread(mut reader: Box<dyn Read + Send>, tx: Sender<Vec<u8>>) {
    thread::spawn(move || {
        loop {
            let mut buf = vec![0_u8; 16 * 1024];
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(buf).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(4));
                }
            }
        }
    });
}

pub fn keyboard_input_system(
    mut evr_key: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut terminal: ResMut<TerminalState>,
) {
    while let Ok(buf) = terminal.rx.try_recv() {
        {
            let mut term = terminal.term.lock();
            let mut parser = terminal.parser.lock();
            parser.advance(&mut *term, &buf);
        }
        terminal.dirty = true;
    }

    for event in evr_key.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }

        let mut bytes: Option<Vec<u8>> = None;

        if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
            match &event.logical_key {
                Key::Character(ch) if ch.eq_ignore_ascii_case("c") => {
                    // handled elsewhere for copy if selection exists
                    continue;
                }
                Key::Character(ch) if ch.eq_ignore_ascii_case("v") => {
                    if let Ok(mut clipboard) = Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            bytes = Some(text.into_bytes());
                        }
                    }
                }
                _ => {}
            }
        }

        if bytes.is_none() {
            bytes = key_event_to_bytes(event);
        }

        if let Some(bytes) = bytes {
            let _ = terminal.writer.lock().write_all(&bytes);
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
        Key::Space => Some(" ".as_bytes().to_vec()),
        Key::Character(text) => Some(text.as_str().as_bytes().to_vec()),
        _ => None,
    }
}

pub fn mouse_wheel_system(
    mut evr_wheel: MessageReader<MouseWheel>,
    mut terminal: ResMut<TerminalState>,
) {
    let mut scroll_lines: i32 = 0;

    for event in evr_wheel.read() {
        match event.unit {
            MouseScrollUnit::Line => scroll_lines += event.y as i32,
            MouseScrollUnit::Pixel => {
                scroll_lines += (event.y / terminal.cell_height_px).round() as i32
            }
        }
    }

    if scroll_lines == 0 {
        return;
    }

    {
        let mut term = terminal.term.lock();
        term.scroll_display(if scroll_lines > 0 {
            alacritty_terminal::grid::Scroll::Delta(scroll_lines)
        } else {
            alacritty_terminal::grid::Scroll::Delta(scroll_lines)
        });
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
        terminal.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
        terminal.dirty = true;
    }

    if buttons.pressed(MouseButton::Left) {
        if let Some(selection) = terminal.selection.as_mut() {
            selection.update(point, Side::Right);
            terminal.dirty = true;
        }
    }

    if buttons.just_released(MouseButton::Left) {
        terminal.dirty = true;
    }
}

pub fn copy_selection_system(keys: Res<ButtonInput<KeyCode>>, mut terminal: ResMut<TerminalState>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    if !ctrl || !keys.just_pressed(KeyCode::KeyC) {
        return;
    }

    let Some(selection) = terminal.selection.as_ref() else {
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

pub fn sync_terminal_view_system(
    mut terminal: ResMut<TerminalState>,
    mut q_lines: Query<(&TerminalLine, &mut Text, &mut TextColor)>,
) {
    while let Ok(buf) = terminal.rx.try_recv() {
        {
            let mut term = terminal.term.lock();
            let mut parser = terminal.parser.lock();
            parser.advance(&mut *term, &buf);
        }
        terminal.dirty = true;
    }

    if !terminal.dirty {
        return;
    }

    {
        let term = terminal.term.lock();
        let content = term.renderable_content();

        let mut rendered_rows: Vec<String> = vec![String::new(); terminal.rows];

        for indexed in content.display_iter {
            let point = indexed.point;
            let row = point.line.0 as usize;
            if row >= rendered_rows.len() {
                continue;
            }

            let ch = if indexed.cell.c == '\t' {
                ' '
            } else {
                indexed.cell.c
            };

            ensure_len(&mut rendered_rows[row], point.column.0 as usize);
            rendered_rows[row].push(if ch == '\0' { ' ' } else { ch });
        }

        if let Some(selection) = terminal.selection.as_ref() {
            // very simple visual hint for now; real highlight comes later
            let _ = selection;
        }

        for (line, mut text, mut color) in &mut q_lines {
            if line.row < rendered_rows.len() {
                *text = Text::new(rendered_rows[line.row].clone());
                *color = TextColor(Color::srgb(0.85, 0.85, 0.85));
            }
        }
    }
    terminal.dirty = false;
}

fn ensure_len(s: &mut String, target: usize) {
    let current = s.chars().count();
    if current < target {
        for _ in current..target {
            s.push(' ');
        }
    }
}

fn cursor_to_terminal_point(window: &Window, cursor_pos: Vec2, terminal: &TerminalState) -> Point {
    let x = (cursor_pos.x - 12.0).max(0.0);
    let y = (cursor_pos.y - 12.0).max(0.0);

    let col = (x / terminal.cell_width_px).floor() as usize;
    let row = (y / terminal.cell_height_px).floor() as usize;

    let row = row.min(terminal.rows.saturating_sub(1));
    let col = col.min(terminal.cols.saturating_sub(1));

    Point::new(Line(row as i32), Column(col))
}
