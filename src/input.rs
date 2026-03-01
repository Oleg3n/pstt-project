use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::time::Duration;
use anyhow::Result;

#[derive(Debug, PartialEq)]
pub enum InputCommand {
    StartRecording,
    StopRecording,
    Exit,
    None,
}

pub fn check_input() -> Result<InputCommand> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key_event) = event::read()? {
            match key_event.code {
                KeyCode::Enter => return Ok(InputCommand::StartRecording),
                KeyCode::Esc => return Ok(InputCommand::StopRecording),
                KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(InputCommand::Exit);
                }
                _ => {}
            }
        }
    }
    Ok(InputCommand::None)
}
