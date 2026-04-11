use bevy::prelude::*;

use alacritty_terminal::{
    term::cell::Cell,
    vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Rgb},
};

use super::{
    TerminalCursorBlink, TerminalLineBg, TerminalLineCursor, TerminalLineText, TerminalState,
    cursor_blink_visible,
};

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
