use crate::terminal::{CachedCursor, CachedRowRender, TerminalLine, TerminalRenderCache};

use super::{StyledRun, push_run};
use bevy::prelude::*;

use alacritty_terminal::vte::ansi::CursorShape;

use super::{
    TerminalCursorBlink, TerminalLineBg, TerminalLineCursor, TerminalLineText, TerminalState,
    cursor_blink_visible, cursor_color, default_bg, default_fg, term_bg_to_bevy, term_fg_to_bevy,
};

pub fn sync_terminal_view_system(
    mut commands: Commands,
    mut terminal: ResMut<TerminalState>,
    blink: Res<TerminalCursorBlink>,
    mut cache: ResMut<TerminalRenderCache>,
    q_bg: Query<(Entity, &TerminalLineBg)>,
    q_text: Query<(Entity, &TerminalLineText, &TextFont)>,
    q_cursor: Query<(Entity, &TerminalLineCursor)>,
) {
    if !terminal.dirty {
        return;
    }

    let show_cursor = cursor_blink_visible(&blink);

    let mut next_rows: Vec<CachedRowRender> = vec![
        CachedRowRender {
            runs: Vec::new(),
            cursor: None,
        };
        terminal.rows
    ];

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
            if row >= next_rows.len() {
                continue;
            }

            let col = c.point.column.0 as usize;

            if !seen_any_in_row[row] {
                seen_any_in_row[row] = true;
                if col > 0 {
                    for _ in 0..col {
                        push_run(&mut next_rows[row].runs, ' ', default_fg(), default_bg());
                    }
                }
            } else if col > last_col_per_row[row] + 1 {
                for _ in (last_col_per_row[row] + 1)..col {
                    push_run(&mut next_rows[row].runs, ' ', default_fg(), default_bg());
                }
            }

            let ch = match c.cell.c {
                '\0' | '\t' => ' ',
                other => other,
            };

            let fg = term_fg_to_bevy(c.cell);
            let bg = term_bg_to_bevy(c.cell);

            push_run(&mut next_rows[row].runs, ch, fg, bg);
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

    if let Some(row) = cursor_row {
        let shape = cursor_shape.unwrap_or(CursorShape::Block);
        next_rows[row].cursor = Some(CachedCursor {
            col: cursor_col,
            shape,
            visible: show_cursor,
        });
    }

    if cache.last_cursor_row != cursor_row {
        if let Some(old_row) = cache.last_cursor_row {
            terminal.mark_row_dirty(old_row);
        }
        if let Some(new_row) = cursor_row {
            terminal.mark_row_dirty(new_row);
        }
        cache.last_cursor_row = cursor_row;
    }

    for row in 0..terminal.rows {
        let next = &next_rows[row];
        let prev = cache.rows[row].as_ref();

        if prev != Some(next) {
            terminal.mark_row_dirty(row);
        }
    }

    for (entity, bg_line) in &q_bg {
        let row = bg_line.row;
        if row >= terminal.rows || !terminal.dirty_rows[row] {
            continue;
        }

        commands.entity(entity).despawn_children();

        let runs = &next_rows[row].runs;
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
        let row = text_line.row;
        if row >= terminal.rows || !terminal.dirty_rows[row] {
            continue;
        }

        commands.entity(entity).despawn_children();

        let runs = &next_rows[row].runs;
        let mut font = parent_font.clone();
        font.font_size = terminal.font_size_px;

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

    for (entity, cursor_line) in &q_cursor {
        let row = cursor_line.row;
        if row >= terminal.rows || !terminal.dirty_rows[row] {
            continue;
        }

        commands.entity(entity).despawn_children();

        let Some(cursor) = next_rows[row].cursor.as_ref() else {
            continue;
        };

        if !cursor.visible {
            continue;
        }

        let (width_px, height_px, top_px) = cursor_dimensions(
            cursor.shape,
            terminal.cell_width_px,
            terminal.cell_height_px,
        );

        let left_px = cursor.col as f32 * terminal.cell_width_px;

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

    for row in 0..terminal.rows {
        if terminal.dirty_rows[row] {
            cache.rows[row] = Some(next_rows[row].clone());
        }
    }

    terminal.clear_dirty_flags();
}

pub fn sync_terminal_metrics_system(
    terminal: Res<TerminalState>,
    mut q: Query<(
        Has<TerminalLine>,
        Has<TerminalLineBg>,
        Has<TerminalLineText>,
        Has<TerminalLineCursor>,
        &mut Node,
        Option<&mut TextFont>,
    )>,
) {
    if !terminal.is_changed() {
        return;
    }

    let h = Val::Px(terminal.cell_height_px);

    for (is_line, is_bg, is_text, is_cursor, mut node, font) in &mut q {
        if is_line {
            node.min_height = h;
            node.height = h;
        }

        if is_bg {
            node.height = h;
        }

        if is_text {
            node.min_height = h;
            node.height = h;

            if let Some(mut font) = font {
                font.font_size = terminal.font_size_px;
            }
        }

        if is_cursor {
            node.height = h;
        }
    }
}

fn cursor_dimensions(shape: CursorShape, cell_width: f32, cell_height: f32) -> (f32, f32, f32) {
    match shape {
        CursorShape::Block => (cell_width, cell_height, 0.0),
        CursorShape::Underline => (cell_width, 2.0, cell_height - 2.0),
        CursorShape::Beam => (2.0, cell_height, 0.0),
        _ => (cell_width, cell_height, 0.0),
    }
}
