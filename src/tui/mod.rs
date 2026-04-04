pub mod app;
pub mod events;
pub mod ui;

use anyhow::Result;
use std::path::PathBuf;

pub fn run(db_path: PathBuf, collection: Option<String>) -> Result<()> {
    // DO NOT call init_terminal() here — run_app() creates and manages its own Terminal
    // to avoid double-terminal issues that cause Windows console corruption.

    // Install panic hook so terminal is restored if the app panics
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort cleanup on panic
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        orig_hook(info);
    }));

    let mut app = app::App::new(db_path, collection);

    // Try to run initial search if there's input
    if !app.input.is_empty() {
        let _ = app.run_search();
    }

    // run_app handles its own Terminal lifecycle (setup, drawing, teardown)
    let res = events::run_app(&mut app);

    // Explicit cleanup on normal exit (in case run_app's cleanup didn't run)
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    );

    res
}
