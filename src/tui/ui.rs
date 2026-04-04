use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Focus, InputMode};

pub fn draw(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Search box
            Constraint::Min(5),     // Results + Preview
            Constraint::Length(1),  // Status bar
        ])
        .split(f.area());

    draw_header(f, app, main_chunks[0]);
    draw_search_box(f, app, main_chunks[1]);
    
    // Results area - split between list and preview
    if app.show_preview && !app.results.is_empty() {
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // Results list
                Constraint::Percentage(50),  // Preview
            ])
            .split(main_chunks[2]);
        
        draw_results(f, app, content_chunks[0]);
        draw_preview(f, app, content_chunks[1]);
    } else {
        draw_results(f, app, main_chunks[2]);
    }
    
    draw_status_bar(f, app, main_chunks[3]);

    if app.show_help {
        draw_help(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let title = if let Some(ref col) = app.collection {
        format!("🐉 hoard — {}", col)
    } else {
        "🐉 hoard — your knowledge hoard".into()
    };

    let header = Paragraph::new(title)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);

    f.render_widget(header, area);
}

fn draw_search_box(f: &mut Frame, app: &App, area: Rect) {
    let border_color = match app.focus {
        Focus::Search => Color::Yellow,
        _ => Color::Gray,
    };

    let search_text = if app.input.is_empty() {
        "Type to search your hoard..."
    } else {
        &app.input
    };

    let style = if app.input.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let search = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(" Search ")
        )
        .style(style);

    f.render_widget(search, area);

    if let InputMode::Editing = app.input_mode {
        if app.focus == Focus::Search {
            f.set_cursor_position((
                area.x + 1 + app.input.len() as u16,
                area.y + 1,
            ));
        }
    }
}

fn draw_results(f: &mut Frame, app: &App, area: Rect) {
    if app.is_searching {
        let loading = Paragraph::new("🔍 Searching...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, area);
        return;
    }

    if app.results.is_empty() {
        if app.search_query.is_empty() {
            let empty = Paragraph::new("Start typing to search your documents")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(empty, area);
        } else {
            let empty = Paragraph::new(format!("No results for '{}'", app.search_query))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(empty, area);
        }
        return;
    }

    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, result)| {
            let is_selected = i == app.selected_index;
            
            let score_color = if result.score > 0.9 {
                Color::Green
            } else if result.score > 0.7 {
                Color::Yellow
            } else {
                Color::Gray
            };

            let title = Span::styled(
                format!("{} ", result.title),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

            let score = Span::styled(
                format!("{:.2}", result.score),
                Style::default().fg(score_color),
            );

            let path = Span::styled(
                format!("  {}", result.path),
                Style::default().fg(Color::DarkGray),
            );

            let header_line = Line::from(vec![
                Span::raw(format!("{} ", if is_selected { "▶" } else { " " })),
                title,
                Span::raw(" "),
                score,
                path,
            ]);

            let snippet_text = if result.snippet.len() > 80 {
                let end = result.snippet
                    .char_indices()
                    .nth(80)
                    .map(|(i, _)| i)
                    .unwrap_or(result.snippet.len());
                format!("{}...", &result.snippet[..end])
            } else {
                result.snippet.clone()
            };

            let snippet = Span::styled(
                snippet_text,
                Style::default().fg(Color::Gray),
            );

            let lines = vec![
                header_line,
                Line::from(vec![Span::raw("   "), snippet]),
                Line::from(""),
            ];

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(Text::from(lines)).style(style)
        })
        .collect();

    let border_style = if app.focus == Focus::Results || (!app.show_preview && app.focus == Focus::Preview) {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };

    let results_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(format!(" Results ({}) ", app.results.len()))
        );

    f.render_widget(results_list, area);
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.focus == Focus::Preview {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };

    let content = app.preview_content.as_deref().unwrap_or("No preview available");
    
    // Simple scroll implementation - skip first N lines
    let lines: Vec<&str> = content.lines().skip(app.preview_scroll).collect();
    let visible_content = lines.join("\n");

    let preview = Paragraph::new(visible_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Preview (Ctrl+J/K scroll) ")
        )
        .wrap(Wrap { trim: false })
        .scroll((0, 0));

    f.render_widget(preview, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled("🐉 hoard", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
    ];

    if let Some((ref msg, _)) = app.message {
        spans.push(Span::styled(msg.clone(), Style::default().fg(Color::Yellow)));
    } else {
        spans.push(Span::raw("? help | "));
        spans.push(Span::raw("↑↓ navigate | "));
        spans.push(Span::raw("Enter open | "));
        spans.push(Span::raw("Tab focus | "));
        spans.push(Span::raw("p preview | "));
        spans.push(Span::raw("q quit"));
    }

    let status = Paragraph::new(Line::from(spans));
    f.render_widget(status, area);
}

fn draw_help(f: &mut Frame, _app: &App) {
    let area = centered_rect(60, 80, f.area());

    let help_text = r#"
    🐉 HOARD - Your Knowledge Hoard
    
    NAVIGATION
    ───────────────────────────
    ↑ / k              Move up in results
    ↓ / j              Move down in results
    Tab                Toggle focus (search/results/preview)
    p                  Toggle preview pane
    
    SEARCH
    ───────────────────────────
    Type               Search as you type
    Enter              Open selected result in editor
    Esc                Clear search / exit input mode
    
    PREVIEW
    ───────────────────────────
    Ctrl+J / Ctrl+D    Scroll preview down
    Ctrl+K / Ctrl+U    Scroll preview up
    
    COMMANDS
    ───────────────────────────
    :add <path>        Add collection (not yet impl)
    :update            Re-index files (not yet impl)
    
    GENERAL
    ───────────────────────────
    ?                  Toggle this help
    q / Ctrl+C         Quit
    
    ENVIRONMENT
    ───────────────────────────
    HOARD_EMBED_URL     Embedding API URL
    HOARD_EMBED_MODEL   Model name
    OPENAI_API_KEY      For OpenAI
    
    For more: hoard --help
    "#;

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .wrap(Wrap { trim: true });

    f.render_widget(Clear, area);
    f.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
