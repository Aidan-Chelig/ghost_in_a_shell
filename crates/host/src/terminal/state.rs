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
    vte::ansi::CursorShape,
};

use super::StyledRun;

pub const DEFAULT_FONT_SIZE_PX: f32 = 16.0;
pub const MIN_FONT_SIZE_PX: f32 = 8.0;
pub const MAX_FONT_SIZE_PX: f32 = 40.0;
pub const FONT_SIZE_STEP_PX: f32 = 1.0;

// These are approximate ratios for your current font/layout.
// Tweak if needed.
const CELL_WIDTH_RATIO: f32 = 10.8 / 16.0;
const CELL_HEIGHT_RATIO: f32 = 20.0 / 16.0;

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

    pub font_size_px: f32,
    pub cell_width_px: f32,
    pub cell_height_px: f32,

    pub selection_anchor: Option<Point>,
    pub selection: Option<Selection>,
    pub dirty: bool,
    pub dirty_rows: Vec<bool>,
}

impl TerminalState {
    pub fn mark_row_dirty(&mut self, row: usize) {
        if row < self.dirty_rows.len() {
            self.dirty_rows[row] = true;
            self.dirty = true;
        }
    }

    pub fn mark_all_rows_dirty(&mut self) {
        self.dirty_rows.fill(true);
        self.dirty = true;
    }

    pub fn clear_dirty_flags(&mut self) {
        self.dirty_rows.fill(false);
        self.dirty = false;
    }

    pub fn apply_font_size(&mut self, font_size_px: f32) {
        self.font_size_px = font_size_px.clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
        self.cell_width_px = self.font_size_px * CELL_WIDTH_RATIO;
        self.cell_height_px = self.font_size_px * CELL_HEIGHT_RATIO;
    }

    pub fn zoom_in(&mut self) -> bool {
        let old = self.font_size_px;
        self.apply_font_size(old + FONT_SIZE_STEP_PX);
        self.font_size_px != old
    }

    pub fn zoom_out(&mut self) -> bool {
        let old = self.font_size_px;
        self.apply_font_size(old - FONT_SIZE_STEP_PX);
        self.font_size_px != old
    }

    pub fn reset_zoom(&mut self) -> bool {
        let old = self.font_size_px;
        self.apply_font_size(DEFAULT_FONT_SIZE_PX);
        self.font_size_px != old
    }

    pub fn resize_grid(&mut self, rows: usize, cols: usize) -> bool {
        let rows = rows.max(1);
        let cols = cols.max(1);

        if self.rows == rows && self.cols == cols {
            return false;
        }

        self.rows = rows;
        self.cols = cols;
        self.dirty_rows.resize(rows, true);
        self.mark_all_rows_dirty();
        true
    }
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

#[derive(Clone, Debug, PartialEq)]
pub struct CachedCursor {
    pub col: usize,
    pub shape: CursorShape,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CachedRowRender {
    pub runs: Vec<StyledRun>,
    pub cursor: Option<CachedCursor>,
}

#[derive(Resource)]
pub struct TerminalRenderCache {
    pub rows: Vec<Option<CachedRowRender>>,
    pub last_cursor_row: Option<usize>,
}

impl TerminalRenderCache {
    pub fn new(rows: usize) -> Self {
        Self {
            rows: vec![None; rows],
            last_cursor_row: None,
        }
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
    cache: Res<TerminalRenderCache>,
) {
    let was_visible = cursor_blink_visible(&blink);
    blink.elapsed += time.delta_secs();
    let is_visible = cursor_blink_visible(&blink);

    if was_visible != is_visible {
        if let Some(row) = cache.last_cursor_row {
            terminal.mark_row_dirty(row);
        }
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

    let mut state = TerminalState {
        term,
        parser,
        cols,
        rows,
        font_size_px: DEFAULT_FONT_SIZE_PX,
        cell_width_px: 10.8,
        cell_height_px: 20.0,
        selection_anchor: None,
        selection: None,
        dirty: true,
        dirty_rows: vec![true; rows],
    };

    state.apply_font_size(DEFAULT_FONT_SIZE_PX);

    commands.insert_resource(state);
    commands.insert_resource(TerminalRenderCache::new(rows));
}
