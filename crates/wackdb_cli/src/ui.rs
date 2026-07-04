use crate::state::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use ratatui_textarea::TextArea;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::DiskManager;

pub fn render_ui<D: DiskManager<8192>>(
    f: &mut Frame,
    state: &mut AppState,
    textarea: &TextArea,
    bpm: &BufferPoolManager<8192, D>,
) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1), // Header
                Constraint::Min(1),    // Main content
                Constraint::Length(3), // Input
            ]
            .as_ref(),
        )
        .split(size);

    // Header
    let header = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            "WackDB Demo | ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        ),
        Span::raw("type \\help"),
    ])])
    .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(header, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(chunks[1]);

    // Logs
    let content_len = state.logs.len();
    let view_height = main_chunks[0].height.saturating_sub(2) as usize;
    let max_scroll = content_len.saturating_sub(view_height);
    if state.vertical_scroll > max_scroll {
        state.vertical_scroll = max_scroll;
    }

    let messages = state.logs.join("\n");
    let logs_widget = Paragraph::new(messages)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Output Log")
                .style(Style::default().fg(Color::White)),
        )
        .scroll((state.vertical_scroll as u16, 0));
    f.render_widget(logs_widget, main_chunks[0]);

    let mut scrollbar_state = ScrollbarState::default()
        .content_length(max_scroll)
        .position(state.vertical_scroll);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("▲"))
        .end_symbol(Some("▼"));
    f.render_stateful_widget(
        scrollbar,
        main_chunks[0].inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut scrollbar_state,
    );

    // Metrics
    let table_status = match &state.active_table {
        Some(name) => name.clone(),
        None => "None".to_string(),
    };

    let root_status = match state.root_id {
        Some(id) => format!("File ID: {}, Page: {}", id.file_id, id.page_num),
        None => "None (Empty Tree)".to_string(),
    };

    let metrics_text = format!(
        "Active Table: {}\n\nBuffer Pool:\nHit Rate: {:.2}%\nTotal Hits: {}\nTotal Misses: {}\n\nCurrent Tree Root:\n{}",
        table_status,
        bpm.get_hit_rate() * 100.0,
        bpm.get_hits(),
        bpm.get_misses(),
        root_status
    );

    let metrics_widget = Paragraph::new(metrics_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Status & Metrics")
            .style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(metrics_widget, main_chunks[1]);

    // Input
    f.render_widget(textarea, chunks[2]);
}
