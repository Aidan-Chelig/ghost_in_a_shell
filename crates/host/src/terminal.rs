use std::{
    io::{Read, Write},
    sync::Arc,
    thread,
    time::Duration,
};

use arboard::Clipboard;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use alacritty_terminal::{
    event::{Event as AlacrittyEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point},
    selection::{Selection, SelectionType},
    term::{Config as TermConfig, Term, cell::Cell},
};

#[derive(Resource)]
pub struct TerminalState {
    pub term: Arc<Mutex<Term<NoopListener>>>,
    pub parser: Arc<Mutex<alacritty_terminal::vte::ansi::Processor>>,
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

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerminalCursorBlink>();
    }
}

#[derive(Resource, Default)]
pub struct TerminalCursorBlink(pub f32);

//
// ─────────────────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────────────────
//

pub fn spawn_terminal_backend(commands: &mut Commands) {
    let rows = 40usize;
    let cols = 120usize;

    let size = SimpleSize { rows, cols };

    let term = Arc::new(Mutex::new(Term::new(
        TermConfig::default(),
        &size,
        NoopListener,
    )));
    let parser = Arc::new(Mutex::new(alacritty_terminal::vte::ansi::Processor::new()));

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");

    pair.slave.spawn_command(cmd).unwrap();

    let reader = pair.master.try_clone_reader().unwrap();
    let writer = pair.master.take_writer().unwrap();

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
            let mut buf = vec![0; 16 * 1024];
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(buf).is_err() {
                        break;
                    }
                }
                Err(_) => thread::sleep(Duration::from_millis(4)),
            }
        }
    });
}

//
// ─────────────────────────────────────────────────────────
// Input
// ─────────────────────────────────────────────────────────
//

pub fn keyboard_input_system(
    mut evr_key: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut terminal: ResMut<TerminalState>,
) {
    while let Ok(buf) = terminal.rx.try_recv() {
        let mut term = terminal.term.lock();
        let mut parser = terminal.parser.lock();
        parser.advance(&mut *term, &buf);
    }
    terminal.dirty = true;

    for event in evr_key.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }

        let mut bytes = None;

        if keys.pressed(KeyCode::ControlLeft) {
            match &event.logical_key {
                Key::Character(ch) if ch.eq_ignore_ascii_case("v") => {
                    if let Ok(mut cb) = Clipboard::new() {
                        if let Ok(text) = cb.get_text() {
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
        Key::Backspace => Some(vec![0x7f]),
        Key::Character(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

//
// ─────────────────────────────────────────────────────────
// Rendering (COLOR VERSION)
// ─────────────────────────────────────────────────────────
//

#[derive(Clone)]
struct StyledRun {
    text: String,
    fg: Color,
}

fn push_run(runs: &mut Vec<StyledRun>, ch: char, fg: Color) {
    if let Some(last) = runs.last_mut() {
        if last.fg == fg {
            last.text.push(ch);
            return;
        }
    }
    runs.push(StyledRun {
        text: ch.to_string(),
        fg,
    });
}

fn term_color_to_bevy(_cell: &Cell) -> Color {
    Color::srgb(0.8, 0.8, 0.8)
}

pub fn sync_terminal_view_system(
    mut commands: Commands,
    mut terminal: ResMut<TerminalState>,
    q_lines: Query<(Entity, &TerminalLine)>,
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

        let cells: Vec<_> = content.display_iter.collect();

        let min_line = cells.iter().map(|c| c.point.line.0).min().unwrap_or(0);

        let mut rows: Vec<Vec<StyledRun>> = vec![Vec::new(); terminal.rows];

        for c in cells {
            let row = (c.point.line.0 - min_line) as usize;
            if row >= rows.len() {
                continue;
            }

            let ch = if c.cell.c == '\0' { ' ' } else { c.cell.c };
            let fg = term_color_to_bevy(c.cell);

            push_run(&mut rows[row], ch, fg);
        }

        // rebuild UI
        for (entity, line) in &q_lines {
            commands.entity(entity).despawn_children();

            if line.row >= rows.len() {
                continue;
            }

            let runs = &rows[line.row];

            commands.entity(entity).with_children(|parent| {
                for run in runs {
                    parent.spawn((TextSpan::new(run.text.clone()), TextColor(run.fg)));
                }
            });
        }
    }

    terminal.dirty = false;
}
