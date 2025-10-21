use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame, Terminal,
};
use pulldown_cmark::Parser;
use std::fs;
use std::io;
use clap::Parser as ClapParser;

#[derive(ClapParser)]
#[command(name = "mess")]
#[command(about = "A less-like viewer with markdown support")]
struct Args {
    /// File to view
    file: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ViewMode {
    Rendered,
    Source,
    SideBySide,
}

#[derive(Debug)]
struct AppState {
    content: String,
    rendered_content: String,
    view_mode: ViewMode,
    scroll_offset: usize,
    file_path: String,
    is_markdown: bool,
}

impl AppState {
    fn new(file_path: String) -> Result<Self> {
        // Check if file exists first
        if !std::path::Path::new(&file_path).exists() {
            return Err(color_eyre::eyre::eyre!("File '{}' does not exist", file_path));
        }
        
        let content = fs::read_to_string(&file_path)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to read file '{}': {}", file_path, e))?;
        let is_markdown = file_path.ends_with(".md") || file_path.ends_with(".markdown");
        
        let rendered_content = if is_markdown {
            Self::render_markdown(&content)
        } else {
            content.clone()
        };

        Ok(AppState {
            content,
            rendered_content,
            view_mode: if is_markdown { ViewMode::Rendered } else { ViewMode::Source },
            scroll_offset: 0,
            file_path,
            is_markdown,
        })
    }


    fn render_markdown(content: &str) -> String {
        let parser = Parser::new(content);
        let mut result = String::new();
        
        for event in parser {
            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading { level, .. }) => {
                    result.push('\n');
                    for _ in 0..level as usize {
                        result.push('#');
                    }
                    result.push(' ');
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph) => {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_)) => {
                    result.push_str("\n```\n");
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::CodeBlock) => {
                    result.push_str("\n```\n");
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(_)) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::List(_)) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Item) => {
                    result.push_str("• ");
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Item) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::BlockQuote) => {
                    result.push_str("\n> ");
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::BlockQuote) => {
                    result.push('\n');
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strong) => {
                    result.push_str("**");
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strong) => {
                    result.push_str("**");
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Emphasis) => {
                    result.push('*');
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Emphasis) => {
                    result.push('*');
                }
                pulldown_cmark::Event::Code(text) => {
                    result.push('`');
                    result.push_str(&text);
                    result.push('`');
                }
                pulldown_cmark::Event::Rule => {
                    result.push_str("\n---\n");
                }
                pulldown_cmark::Event::Text(text) => {
                    result.push_str(&text);
                }
                pulldown_cmark::Event::SoftBreak => {
                    result.push('\n');
                }
                pulldown_cmark::Event::HardBreak => {
                    result.push('\n');
                }
                _ => {
                    // Handle other events as needed
                }
            }
        }
        
        // Clean up multiple newlines
        while result.contains("\n\n\n") {
            result = result.replace("\n\n\n", "\n\n");
        }
        
        result.trim().to_string()
    }

    fn toggle_view_mode(&mut self) {
        if !self.is_markdown {
            return; // Only toggle for markdown files
        }
        
        self.view_mode = match self.view_mode {
            ViewMode::Rendered => ViewMode::Source,
            ViewMode::Source => ViewMode::SideBySide,
            ViewMode::SideBySide => ViewMode::Rendered,
        };
        self.scroll_offset = 0; // Reset scroll when changing view
    }

    fn scroll_up(&mut self, lines: usize) {
        if self.scroll_offset > lines {
            self.scroll_offset -= lines;
        } else {
            self.scroll_offset = 0;
        }
    }

    fn scroll_down(&mut self, lines: usize, max_lines: usize) {
        if self.scroll_offset + lines < max_lines {
            self.scroll_offset += lines;
        } else {
            self.scroll_offset = max_lines.saturating_sub(1);
        }
    }

    fn get_content_lines(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::Rendered => self.rendered_content.lines().map(|s| s.to_string()).collect(),
            ViewMode::Source => self.content.lines().map(|s| s.to_string()).collect(),
            ViewMode::SideBySide => {
                // For side-by-side, we render separately in render_side_by_side function
                // but still need to return something for scrollbar calculation
                let rendered_lines: Vec<String> = self.rendered_content.lines().map(|s| s.to_string()).collect();
                let source_lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
                // Return the longer of the two for scrollbar calculation
                if rendered_lines.len() > source_lines.len() {
                    rendered_lines
                } else {
                    source_lines
                }
            }
        }
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    
    let args = Args::parse();
    let app_state = AppState::new(args.file)?;
    
    // Check if we're in an interactive terminal
    if !atty::is(atty::Stream::Stdout) {
        eprintln!("Error: mess requires an interactive terminal");
        std::process::exit(1);
    }
    
    // Initialize terminal using proper Ratatui pattern with alternate screen
    crossterm::terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    
    let result = run(&mut terminal, app_state);
    
    // Restore terminal - this is critical for proper cleanup like "less"
    execute!(io::stdout(), LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;
    
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app_state: AppState) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, &mut app_state))?;
        
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Tab => app_state.toggle_view_mode(),
                KeyCode::Up => app_state.scroll_up(1),
                KeyCode::Down => {
                    let content_lines = app_state.get_content_lines();
                    app_state.scroll_down(1, content_lines.len());
                }
                KeyCode::PageUp => app_state.scroll_up(10),
                KeyCode::PageDown => {
                    let content_lines = app_state.get_content_lines();
                    app_state.scroll_down(10, content_lines.len());
                }
                KeyCode::Home => app_state.scroll_offset = 0,
                KeyCode::End => {
                    let content_lines = app_state.get_content_lines();
                    app_state.scroll_offset = content_lines.len().saturating_sub(1);
                }
                KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    show_help(terminal)?;
                    continue;
                }
                _ => {}
            }
        }
    }
    
    Ok(())
}

fn render_single_view(frame: &mut Frame, app_state: &AppState, area: ratatui::layout::Rect) {
    let content_lines = app_state.get_content_lines();
    let visible_lines = area.height as usize;
    
    let start_line = app_state.scroll_offset;
    let end_line = (start_line + visible_lines).min(content_lines.len());
    
    // Create visible content - apply styling only in Rendered mode
    let visible_text = if start_line < content_lines.len() {
        let lines: Vec<Line> = content_lines[start_line..end_line]
            .iter()
            .map(|line| {
                // Only apply styling for Rendered view
                if matches!(app_state.view_mode, ViewMode::Rendered) {
                    // Apply basic styling for markdown elements
                    let mut spans = Vec::new();
                    let mut remaining = line.as_str();
                    
                    while !remaining.is_empty() {
                        if remaining.starts_with("**") {
                            // Bold text
                            if let Some(end) = remaining[2..].find("**") {
                                let text = &remaining[2..end + 2];
                                spans.push(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
                                remaining = &remaining[end + 4..];
                            } else {
                                spans.push(Span::raw(remaining));
                                break;
                            }
                        } else if remaining.starts_with("*") {
                            // Italic text
                            if let Some(end) = remaining[1..].find("*") {
                                let text = &remaining[1..end + 1];
                                spans.push(Span::styled(text, Style::default().add_modifier(Modifier::ITALIC)));
                                remaining = &remaining[end + 2..];
                            } else {
                                spans.push(Span::raw(remaining));
                                break;
                            }
                        } else if remaining.starts_with("`") {
                            // Code text
                            if let Some(end) = remaining[1..].find("`") {
                                let text = &remaining[1..end + 1];
                                spans.push(Span::styled(text, Style::default().fg(Color::Yellow)));
                                remaining = &remaining[end + 2..];
                            } else {
                                spans.push(Span::raw(remaining));
                                break;
                            }
                        } else if remaining.starts_with("#") {
                            // Headers
                            let header_level = remaining.chars().take_while(|&c| c == '#').count();
                            if header_level > 0 && remaining.len() > header_level && remaining.chars().nth(header_level) == Some(' ') {
                                let text = &remaining[header_level + 1..];
                                spans.push(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
                                remaining = "";
                            } else {
                                spans.push(Span::raw(remaining));
                                break;
                            }
                        } else {
                            // Regular text
                            let next_special = remaining.find(|c| c == '*' || c == '`' || c == '#').unwrap_or(remaining.len());
                            spans.push(Span::raw(&remaining[..next_special]));
                            remaining = &remaining[next_special..];
                        }
                    }
                    
                    Line::from(spans)
                } else {
                    // For Source view, show raw text without styling
                    Line::from(line.as_str())
                }
            })
            .collect();
        Text::from(lines)
    } else {
        Text::default()
    };

    let paragraph = Paragraph::new(visible_text)
        .block(Block::default().borders(Borders::ALL))
        .wrap(ratatui::widgets::Wrap { trim: true });

    frame.render_widget(paragraph, area);

    // Scrollbar
    let total_lines = content_lines.len();
    let mut scrollbar_state = ScrollbarState::new(total_lines)
        .position(app_state.scroll_offset);
    
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"));
    
    frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

fn render_side_by_side(frame: &mut Frame, app_state: &AppState, area: ratatui::layout::Rect) {
    // Split the content area into two columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);
    
    let rendered_lines: Vec<String> = app_state.rendered_content.lines().map(|s| s.to_string()).collect();
    let source_lines: Vec<String> = app_state.content.lines().map(|s| s.to_string()).collect();
    
    let visible_lines = area.height as usize;
    let start_line = app_state.scroll_offset;
    let end_line_rendered = (start_line + visible_lines).min(rendered_lines.len());
    let end_line_source = (start_line + visible_lines).min(source_lines.len());
    
    // Left panel - Rendered view with styling
    let left_text = if start_line < rendered_lines.len() {
        let lines: Vec<Line> = rendered_lines[start_line..end_line_rendered]
            .iter()
            .map(|line| {
                // Apply styling for rendered view
                let mut spans = Vec::new();
                let mut remaining = line.as_str();
                
                while !remaining.is_empty() {
                    if remaining.starts_with("**") {
                        if let Some(end) = remaining[2..].find("**") {
                            let text = &remaining[2..end + 2];
                            spans.push(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
                            remaining = &remaining[end + 4..];
                        } else {
                            spans.push(Span::raw(remaining));
                            break;
                        }
                    } else if remaining.starts_with("*") {
                        if let Some(end) = remaining[1..].find("*") {
                            let text = &remaining[1..end + 1];
                            spans.push(Span::styled(text, Style::default().add_modifier(Modifier::ITALIC)));
                            remaining = &remaining[end + 2..];
                        } else {
                            spans.push(Span::raw(remaining));
                            break;
                        }
                    } else if remaining.starts_with("`") {
                        if let Some(end) = remaining[1..].find("`") {
                            let text = &remaining[1..end + 1];
                            spans.push(Span::styled(text, Style::default().fg(Color::Yellow)));
                            remaining = &remaining[end + 2..];
                        } else {
                            spans.push(Span::raw(remaining));
                            break;
                        }
                    } else if remaining.starts_with("#") {
                        let header_level = remaining.chars().take_while(|&c| c == '#').count();
                        if header_level > 0 && remaining.len() > header_level && remaining.chars().nth(header_level) == Some(' ') {
                            let text = &remaining[header_level + 1..];
                            spans.push(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
                            remaining = "";
                        } else {
                            spans.push(Span::raw(remaining));
                            break;
                        }
                    } else {
                        let next_special = remaining.find(|c| c == '*' || c == '`' || c == '#').unwrap_or(remaining.len());
                        spans.push(Span::raw(&remaining[..next_special]));
                        remaining = &remaining[next_special..];
                    }
                }
                
                Line::from(spans)
            })
            .collect();
        Text::from(lines)
    } else {
        Text::default()
    };
    
    // Right panel - Source view (raw text)
    let right_text = if start_line < source_lines.len() {
        Text::from(source_lines[start_line..end_line_source].join("\n"))
    } else {
        Text::default()
    };
    
    let left_paragraph = Paragraph::new(left_text)
        .block(Block::default().borders(Borders::ALL).title("Rendered"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    
    let right_paragraph = Paragraph::new(right_text)
        .block(Block::default().borders(Borders::ALL).title("Source"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    
    frame.render_widget(left_paragraph, columns[0]);
    frame.render_widget(right_paragraph, columns[1]);
    
    // Scrollbar for the whole area
    let max_lines = rendered_lines.len().max(source_lines.len());
    let mut scrollbar_state = ScrollbarState::new(max_lines)
        .position(app_state.scroll_offset);
    
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"));
    
    frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

fn render(frame: &mut Frame, app_state: &mut AppState) {
    let area = frame.area();
    
    // Create layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(1),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(area);

    // Header
    let header_text = match app_state.view_mode {
        ViewMode::Rendered => "RENDERED VIEW",
        ViewMode::Source => "SOURCE VIEW", 
        ViewMode::SideBySide => "SIDE-BY-SIDE VIEW",
    };
    
    let header = Paragraph::new(Line::from(header_text))
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(format!("mess - {}", app_state.file_path)));
    
    frame.render_widget(header, chunks[0]);
    
    // Check if we're in side-by-side mode - if so, render differently
    if matches!(app_state.view_mode, ViewMode::SideBySide) {
        render_side_by_side(frame, app_state, chunks[1]);
    } else {
        render_single_view(frame, app_state, chunks[1]);
    }

    // Footer
    let footer_text = match app_state.view_mode {
        ViewMode::Rendered => "TAB: Source | ↑↓: Scroll | q: Quit | Ctrl+h: Help",
        ViewMode::Source => "TAB: Side-by-side | ↑↓: Scroll | q: Quit | Ctrl+h: Help",
        ViewMode::SideBySide => "TAB: Rendered | ↑↓: Scroll | q: Quit | Ctrl+h: Help",
    };
    
    let footer = Paragraph::new(Line::from(footer_text))
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    
    frame.render_widget(footer, chunks[2]);
}

fn show_help(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    terminal.draw(|f| {
        let area = f.area();
        let help_text = vec![
            "mess - A less-like viewer with markdown support",
            "Version: 0.1.0",
            "",
            "Keyboard Shortcuts:",
            "  TAB          - Toggle view mode (rendered/source/side-by-side)",
            "  ↑/↓          - Scroll up/down one line",
            "  Page Up/Down - Scroll up/down 10 lines",
            "  Home/End     - Go to beginning/end of file",
            "  q/Esc        - Quit",
            "  Ctrl+h       - Show this help",
            "",
            "View Modes (for markdown files):",
            "  Rendered     - Shows rendered markdown",
            "  Source       - Shows raw markdown source",
            "  Side-by-side - Shows both rendered and source",
            "",
            "Press any key to continue...",
        ];
        
        let help_items: Vec<ListItem> = help_text
            .iter()
            .map(|line| ListItem::new(Line::from(*line)))
            .collect();
        
        let help_list = List::new(help_items)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        
        f.render_widget(help_list, area);
    })?;
    
    // Wait for any key press
    loop {
        if let Event::Key(_) = event::read()? {
            break;
        }
    }
    
    Ok(())
}