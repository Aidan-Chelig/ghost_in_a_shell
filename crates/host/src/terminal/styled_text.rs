use bevy::prelude::*;

#[derive(Clone)]
pub struct StyledRun {
    pub text: String,
    pub fg: Color,
    pub bg: Color,
}

pub fn push_run(runs: &mut Vec<StyledRun>, ch: char, fg: Color, bg: Color) {
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
