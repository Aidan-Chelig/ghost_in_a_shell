use bevy::prelude::*;

use super::{
    TerminalCursorBlink, copy_selection_system, cursor_blink_system, keyboard_input_system,
    mouse_input_system, mouse_wheel_system, spawn_terminal_state, sync_terminal_view_system,
    terminal_rx_system,
};

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerminalCursorBlink>()
            .add_systems(Startup, spawn_terminal_state)
            .add_systems(
                Update,
                (
                    terminal_rx_system,
                    keyboard_input_system,
                    mouse_input_system,
                    mouse_wheel_system,
                    copy_selection_system,
                    cursor_blink_system,
                    sync_terminal_view_system,
                )
                    .chain(),
            );
    }
}
