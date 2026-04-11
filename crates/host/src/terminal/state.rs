use std::sync::Arc;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use alacritty_terminal::{
    event::{Event as AlacrittyEvent, EventListener},
    grid::Dimensions,
    index::Point,
    selection::Selection,
    term::{Config as TermConfig, Term},
};

#[derive(Resource)]
pub struct TerminalIo {
    pub rx: Receiver<Vec<u8>>,
    pub tx: Sender<Vec<u8>>,
}

#[derive(Resource)]
pub struct TerminalState {
    pub term: Arc<Mutex<Term<NoopListener>>>,
    pub parser: Arc<Mutex<alacritty_terminal::vte::ansi::Processor>>,
    pub cols: usize,
    pub rows: usize,
    pub cell_width_px: f32,
    pub cell_height_px: f32,
    pub selection_anchor: Option<Point>,
    pub selection: Option<Selection>,
    pub dirty: bool,
    pub dirty_rows: Vec<bool>,
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
pub struct SimpleSize {
    pub rows: usize,
    pub cols: usize,
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

#[derive(Resource, Default)]
pub struct TerminalCursorBlink {
    pub elapsed: f32,
}

pub const CURSOR_BLINK_PERIOD: f32 = 1.0;
pub const CURSOR_BLINK_VISIBLE_PORTION: f32 = 0.5;

pub fn cursor_blink_visible(blink: &TerminalCursorBlink) -> bool {
    let phase = blink.elapsed.rem_euclid(CURSOR_BLINK_PERIOD);
    phase < CURSOR_BLINK_VISIBLE_PORTION
}

pub fn reset_cursor_blink(blink: &mut TerminalCursorBlink) {
    blink.elapsed = 0.0;
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
        terminal.dirty_rows.fill(true);
        terminal.dirty = true;
    }
}

pub fn spawn_terminal_state(mut commands: Commands) {
    let rows = 40usize;
    let cols = 120usize;

    let size = SimpleSize { rows, cols };

    let term = Arc::new(Mutex::new(Term::new(
        TermConfig::default(),
        &size,
        NoopListener,
    )));
    let parser = Arc::new(Mutex::new(alacritty_terminal::vte::ansi::Processor::new()));

    commands.insert_resource(TerminalState {
        term,
        parser,
        cols,
        rows,
        cell_width_px: 10.8,
        cell_height_px: 20.0,
        selection_anchor: None,
        selection: None,
        dirty: true,
        dirty_rows: vec![true; rows],
    });
}
