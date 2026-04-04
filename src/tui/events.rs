use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

use super::app::{App, Focus, InputMode};

pub fn run_app(app: &mut App) -> anyhow::Result<()> {
    // Setup: enable raw mode and enter alternate screen
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(100);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(
        std::io::stdout(),
    ))?;

    let result = loop {
        terminal.draw(|f| super::ui::draw(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if handle_key(app, key)? {
                    break Ok(());
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = std::time::Instant::now();
        }
    };

    // Always restore terminal state, even if the loop broke on error
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    );

    result
}

fn handle_key(app: &mut App, key: KeyEvent) -> anyhow::Result<bool> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Ok(true);
        }
        KeyCode::Char('q') if app.input_mode == InputMode::Normal => {
            return Ok(true);
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
            return Ok(false);
        }
        _ => {}
    }

    if app.show_help {
        app.show_help = false;
        return Ok(false);
    }

    match app.input_mode {
        InputMode::Normal => handle_normal_mode(app, key),
        InputMode::Editing => handle_editing_mode(app, key),
    }
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) -> anyhow::Result<bool> {
    match key.code {
        KeyCode::Char('i') | KeyCode::Enter => {
            app.input_mode = InputMode::Editing;
            app.focus = Focus::Search;
        }
        KeyCode::Char('p') => {
            app.toggle_preview();
        }
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Search => Focus::Results,
                Focus::Results => {
                    if app.show_preview {
                        Focus::Preview
                    } else {
                        Focus::Search
                    }
                }
                Focus::Preview => Focus::Search,
            };
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.focus == Focus::Preview || key.modifiers.contains(KeyModifiers::CONTROL) {
                app.scroll_preview_down();
            } else if app.focus == Focus::Results {
                app.next_result();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.focus == Focus::Preview || key.modifiers.contains(KeyModifiers::CONTROL) {
                app.scroll_preview_up();
            } else if app.focus == Focus::Results {
                app.previous_result();
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_preview_down();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_preview_up();
        }
        KeyCode::Char(':') => {
            app.set_message("Command mode: use CLI for now".into());
        }
        _ => {}
    }
    Ok(false)
}

fn handle_editing_mode(app: &mut App, key: KeyEvent) -> anyhow::Result<bool> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.focus = Focus::Results;
        }
        KeyCode::Enter => {
            app.run_search()?;
            app.input_mode = InputMode::Normal;
            app.focus = Focus::Results;
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            app.last_search = Some(std::time::Instant::now());
        }
        KeyCode::Backspace => {
            app.input.pop();
            app.last_search = Some(std::time::Instant::now());
        }
        KeyCode::Down => {
            app.focus = Focus::Results;
            app.input_mode = InputMode::Normal;
            app.next_result();
        }
        KeyCode::Up => {
            app.focus = Focus::Results;
            app.input_mode = InputMode::Normal;
            app.previous_result();
        }
        KeyCode::Tab => {
            app.focus = Focus::Results;
            app.input_mode = InputMode::Normal;
        }
        _ => {}
    }
    Ok(false)
}

pub fn init_terminal() -> anyhow::Result<ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::new(backend)?;

    Ok(terminal)
}

pub fn restore_terminal() -> anyhow::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    Ok(())
}
