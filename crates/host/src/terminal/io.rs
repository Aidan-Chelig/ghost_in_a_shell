use std::{io::Read, thread, time::Duration};

use bevy::prelude::*;
use crossbeam_channel::Sender;

use super::{TerminalCursorBlink, TerminalIo, TerminalState, reset_cursor_blink};

pub fn terminal_rx_system(
    mut terminal: ResMut<TerminalState>,
    io: Option<Res<TerminalIo>>,
    mut blink: ResMut<TerminalCursorBlink>,
) {
    let Some(io) = io else {
        return;
    };

    let mut got_data = false;

    while let Ok(buf) = io.rx.try_recv() {
        let mut term = terminal.term.lock();
        let mut parser = terminal.parser.lock();
        parser.advance(&mut *term, &buf);
        got_data = true;
    }

    if got_data {
        reset_cursor_blink(&mut blink);
        terminal.mark_all_rows_dirty();
    }
}

pub fn spawn_reader_thread(mut reader: Box<dyn Read + Send>, tx: Sender<Vec<u8>>) {
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
