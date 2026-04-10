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
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use alacritty_terminal::{
    event::{Event as AlacrittyEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Direction, Line, Point},
    selection::{Selection, SelectionType},
    term::{Config as TermConfig, Term, cell::Cell},
    vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Rgb},
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

#[derive(Component)]
pub struct TerminalLineBg {
    pub row: usize,
}

#[derive(Component)]
pub struct TerminalLineText {
    pub row: usize,
}

#[derive(Component)]
pub struct TerminalLineCursor {
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
        app.init_resource::<TerminalCursorBlink>()
            // startup
            .add_systems(Startup, spawn_terminal_backend)
            // update systems
            .add_systems(
                Update,
                (
                    keyboard_input_system,
                    mouse_input_system,
                    mouse_wheel_system,
                    copy_selection_system,
                    cursor_blink_system,
                    sync_terminal_view_system,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub struct TerminalCursorBlink {
    pub elapsed: f32,
}

const CURSOR_BLINK_PERIOD: f32 = 1.0;
const CURSOR_BLINK_VISIBLE_PORTION: f32 = 0.5;

fn cursor_blink_visible(blink: &TerminalCursorBlink) -> bool {
    let phase = blink.elapsed.rem_euclid(CURSOR_BLINK_PERIOD);
    phase < CURSOR_BLINK_VISIBLE_PORTION
}

pub fn cursor_blink_system(
    time: Res<Time>,
    mut blink: ResMut<TerminalCursorBlink>,
    mut terminal: ResMut<TerminalState>,
) {
    let was_visible = cursor_blink_visible(&blink);
    blink.elapsed += time.delta_secs();
    let is_visible = cursor_blink_visible(&blink);

    if was_visible != is_visible {
        terminal.dirty = true;
    }
}

fn reset_cursor_blink(blink: &mut TerminalCursorBlink) {
    blink.elapsed = 0.0;
}

//
// ─────────────────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────────────────
//

pub fn mouse_wheel_system(
    mut evr_wheel: MessageReader<MouseWheel>,
    mut terminal: ResMut<TerminalState>,
) {
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

pub fn spawn_terminal_backend(mut commands: Commands) {
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
    mut blink: ResMut<TerminalCursorBlink>,
) {
    while let Ok(buf) = terminal.rx.try_recv() {
        {
            let mut term = terminal.term.lock();
            let mut parser = terminal.parser.lock();
            parser.advance(&mut *term, &buf);
            reset_cursor_blink(&mut blink);
        }
        terminal.dirty = true;
    }

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

//
// ─────────────────────────────────────────────────────────
// Rendering (COLOR VERSION)
// ─────────────────────────────────────────────────────────
//

#[derive(Clone)]
struct StyledRun {
    text: String,
    fg: Color,
    bg: Color,
}

fn push_run(runs: &mut Vec<StyledRun>, ch: char, fg: Color, bg: Color) {
    if let Some(last) = runs.last_mut() {
        if last.fg == fg && last.bg == bg {
            last.text.push(ch);
            return;
        }
    }

    runs.push(StyledRun {
        text: ch.to_string(),
        fg,
        bg,
    });
}

pub fn default_fg() -> Color {
    Color::srgb(0.85, 0.85, 0.85)
}

fn default_bg() -> Color {
    Color::srgb(0.06, 0.06, 0.08)
}

fn dim_color(c: Color) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red * 0.66, s.green * 0.66, s.blue * 0.66, s.alpha)
}

fn named_color_to_bevy(named: NamedColor) -> Color {
    match named {
        NamedColor::Black => Color::srgb_u8(0x00, 0x00, 0x00),
        NamedColor::Red => Color::srgb_u8(0xcc, 0x55, 0x55),
        NamedColor::Green => Color::srgb_u8(0x55, 0xcc, 0x55),
        NamedColor::Yellow => Color::srgb_u8(0xcd, 0xcd, 0x55),
        NamedColor::Blue => Color::srgb_u8(0x54, 0x55, 0xcb),
        NamedColor::Magenta => Color::srgb_u8(0xcc, 0x55, 0xcc),
        NamedColor::Cyan => Color::srgb_u8(0x7a, 0xca, 0xca),
        NamedColor::White => Color::srgb_u8(0xcc, 0xcc, 0xcc),

        NamedColor::BrightBlack => Color::srgb_u8(0x55, 0x55, 0x55),
        NamedColor::BrightRed => Color::srgb_u8(0xff, 0x55, 0x55),
        NamedColor::BrightGreen => Color::srgb_u8(0x55, 0xff, 0x55),
        NamedColor::BrightYellow => Color::srgb_u8(0xff, 0xff, 0x55),
        NamedColor::BrightBlue => Color::srgb_u8(0x55, 0x55, 0xff),
        NamedColor::BrightMagenta => Color::srgb_u8(0xff, 0x55, 0xff),
        NamedColor::BrightCyan => Color::srgb_u8(0x55, 0xff, 0xff),
        NamedColor::BrightWhite => Color::srgb_u8(0xff, 0xff, 0xff),

        NamedColor::DimBlack => dim_color(Color::srgb_u8(0x00, 0x00, 0x00)),
        NamedColor::DimRed => dim_color(Color::srgb_u8(0xcc, 0x55, 0x55)),
        NamedColor::DimGreen => dim_color(Color::srgb_u8(0x55, 0xcc, 0x55)),
        NamedColor::DimYellow => dim_color(Color::srgb_u8(0xcd, 0xcd, 0x55)),
        NamedColor::DimBlue => dim_color(Color::srgb_u8(0x54, 0x55, 0xcb)),
        NamedColor::DimMagenta => dim_color(Color::srgb_u8(0xcc, 0x55, 0xcc)),
        NamedColor::DimCyan => dim_color(Color::srgb_u8(0x7a, 0xca, 0xca)),
        NamedColor::DimWhite => dim_color(Color::srgb_u8(0xcc, 0xcc, 0xcc)),

        NamedColor::Foreground => default_fg(),
        NamedColor::Background => default_bg(),
        NamedColor::Cursor => Color::srgb(0.95, 0.95, 0.95),
        NamedColor::BrightForeground => Color::WHITE,
        NamedColor::DimForeground => dim_color(default_fg()),
    }
}

fn indexed_color_to_bevy(idx: u8) -> Color {
    match idx {
        0..=15 => named_color_to_bevy(match idx {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            15 => NamedColor::BrightWhite,
            _ => unreachable!(),
        }),
        16..=231 => {
            let i = idx - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;

            fn level(v: u8) -> u8 {
                match v {
                    0 => 0,
                    _ => 55 + v * 40,
                }
            }

            Color::srgb_u8(level(r), level(g), level(b))
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            Color::srgb_u8(gray, gray, gray)
        }
    }
}

fn ansi_color_to_bevy(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(named) => named_color_to_bevy(named),
        AnsiColor::Spec(Rgb { r, g, b }) => Color::srgb_u8(r, g, b),
        AnsiColor::Indexed(idx) => indexed_color_to_bevy(idx),
    }
}

fn term_fg_to_bevy(cell: &Cell) -> Color {
    ansi_color_to_bevy(cell.fg)
}

fn term_bg_to_bevy(cell: &Cell) -> Color {
    ansi_color_to_bevy(cell.bg)
}

pub fn sync_terminal_view_system(
    mut commands: Commands,
    mut terminal: ResMut<TerminalState>,
    blink: Res<TerminalCursorBlink>,
    q_bg: Query<(Entity, &TerminalLineBg)>,
    q_text: Query<(Entity, &TerminalLineText, &TextFont)>,
    q_cursor: Query<(Entity, &TerminalLineCursor)>,
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

    let mut rows: Vec<Vec<StyledRun>> = vec![Vec::new(); terminal.rows];
    let mut cursor_row: Option<usize> = None;
    let mut cursor_col: usize = 0;
    let mut cursor_shape: Option<CursorShape> = None;

    {
        let term = terminal.term.lock();
        let content = term.renderable_content();

        let cells: Vec<_> = content.display_iter.collect();
        let min_line = cells.iter().map(|c| c.point.line.0).min().unwrap_or(0);

        let mut last_col_per_row: Vec<usize> = vec![0; terminal.rows];
        let mut seen_any_in_row: Vec<bool> = vec![false; terminal.rows];

        for c in cells {
            let visual_row_i32 = c.point.line.0 - min_line;
            if visual_row_i32 < 0 {
                continue;
            }

            let row = visual_row_i32 as usize;
            if row >= rows.len() {
                continue;
            }

            let col = c.point.column.0 as usize;

            if !seen_any_in_row[row] {
                seen_any_in_row[row] = true;
                if col > 0 {
                    for _ in 0..col {
                        push_run(&mut rows[row], ' ', default_fg(), default_bg());
                    }
                }
            } else if col > last_col_per_row[row] + 1 {
                for _ in (last_col_per_row[row] + 1)..col {
                    push_run(&mut rows[row], ' ', default_fg(), default_bg());
                }
            }

            let ch = match c.cell.c {
                '\0' | '\t' => ' ',
                other => other,
            };

            let fg = term_fg_to_bevy(c.cell);
            let bg = term_bg_to_bevy(c.cell);

            push_run(&mut rows[row], ch, fg, bg);
            last_col_per_row[row] = col;
        }

        let render_cursor = content.cursor;
        let visual_cursor_row_i32 = render_cursor.point.line.0 - min_line;
        if visual_cursor_row_i32 >= 0 {
            let row = visual_cursor_row_i32 as usize;
            if row < terminal.rows {
                cursor_row = Some(row);
                cursor_col = render_cursor.point.column.0 as usize;
                cursor_shape = Some(render_cursor.shape);
            }
        }
    }

    for (entity, bg_line) in &q_bg {
        commands.entity(entity).despawn_children();

        if bg_line.row >= rows.len() {
            continue;
        }

        let runs = &rows[bg_line.row];
        let cell_width_px = terminal.cell_width_px;
        let cell_height_px = terminal.cell_height_px;

        commands.entity(entity).with_children(|parent| {
            for run in runs {
                let width_px = run.text.chars().count() as f32 * cell_width_px;

                parent.spawn((
                    Node {
                        width: Val::Px(width_px),
                        height: Val::Px(cell_height_px),
                        ..default()
                    },
                    BackgroundColor(run.bg),
                ));
            }
        });
    }

    for (entity, text_line, parent_font) in &q_text {
        commands.entity(entity).despawn_children();

        if text_line.row >= rows.len() {
            continue;
        }

        let runs = &rows[text_line.row];
        let font = parent_font.clone();

        commands.entity(entity).with_children(|parent| {
            for run in runs {
                parent.spawn((
                    TextSpan::new(run.text.clone()),
                    font.clone(),
                    TextColor(run.fg),
                ));
            }
        });
    }

    let show_cursor = cursor_blink_visible(&blink);

    for (entity, cursor_line) in &q_cursor {
        commands.entity(entity).despawn_children();

        let Some(active_row) = cursor_row else {
            continue;
        };

        if !show_cursor || cursor_line.row != active_row {
            continue;
        }

        let shape = cursor_shape.unwrap_or(CursorShape::Block);
        let (width_px, height_px, top_px) =
            cursor_dimensions(shape, terminal.cell_width_px, terminal.cell_height_px);

        let left_px = cursor_col as f32 * terminal.cell_width_px;

        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Px(width_px),
                    height: Val::Px(height_px),
                    position_type: PositionType::Absolute,
                    left: Val::Px(left_px),
                    top: Val::Px(top_px),
                    ..default()
                },
                BackgroundColor(cursor_color()),
            ));
        });
    }

    terminal.dirty = false;
}

fn cursor_dimensions(shape: CursorShape, cell_width: f32, cell_height: f32) -> (f32, f32, f32) {
    match shape {
        CursorShape::Block => (cell_width, cell_height, 0.0),
        CursorShape::Underline => (cell_width, 2.0, cell_height - 2.0),
        CursorShape::Beam => (2.0, cell_height, 0.0),
        _ => (cell_width, cell_height, 0.0),
    }
}

fn cursor_color() -> Color {
    Color::srgb(0.95, 0.95, 0.95)
}
