use bevy::prelude::*;

use alacritty_terminal::{
    term::cell::Cell,
    vte::ansi::{Color as AnsiColor, NamedColor, Rgb},
};

pub fn default_fg() -> Color {
    Color::srgb(0.85, 0.85, 0.85)
}

pub fn default_bg() -> Color {
    Color::srgb(0.06, 0.06, 0.08)
}

pub fn cursor_color() -> Color {
    Color::srgb(0.95, 0.95, 0.95)
}

fn dim_color(c: Color) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red * 0.66, s.green * 0.66, s.blue * 0.66, s.alpha)
}

pub fn named_color_to_bevy(named: NamedColor) -> Color {
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
        NamedColor::Cursor => cursor_color(),
        NamedColor::BrightForeground => Color::WHITE,
        NamedColor::DimForeground => dim_color(default_fg()),
    }
}

pub fn indexed_color_to_bevy(idx: u8) -> Color {
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

pub fn ansi_color_to_bevy(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(named) => named_color_to_bevy(named),
        AnsiColor::Spec(Rgb { r, g, b }) => Color::srgb_u8(r, g, b),
        AnsiColor::Indexed(idx) => indexed_color_to_bevy(idx),
    }
}

pub fn term_fg_to_bevy(cell: &Cell) -> Color {
    ansi_color_to_bevy(cell.fg)
}

pub fn term_bg_to_bevy(cell: &Cell) -> Color {
    ansi_color_to_bevy(cell.bg)
}
