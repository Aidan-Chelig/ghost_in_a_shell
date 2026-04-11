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
    q_bg: Query<(Entity, &TerminalLineBg)>,
    q_text: Query<(Entity, &TerminalLineText, &TextFont)>,
    q_cursor: Query<(Entity, &TerminalLineCursor)>,
) {
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

            let col = c.point.column.0;

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
                cursor_col = render_cursor.point.column.0;
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
