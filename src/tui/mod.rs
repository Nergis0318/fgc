use crate::progress::CloneStatus;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct TuiHandle {
    handle: Option<thread::JoinHandle<()>>,
}

impl TuiHandle {
    pub fn spawn(status: Arc<Mutex<CloneStatus>>) -> Self {
        let handle = thread::spawn(move || {
            if let Err(e) = run_loop(status) {
                eprintln!("TUI error: {e}");
            }
        });
        Self {
            handle: Some(handle),
        }
    }

    pub fn join(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_loop(status: Arc<Mutex<CloneStatus>>) -> std::io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        let snap = status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|e| e.into_inner().clone());

        terminal.draw(|frame| render(frame, &snap))?;

        if snap.done {
            thread::sleep(Duration::from_millis(800));
            break;
        }

        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn render(frame: &mut Frame, status: &CloneStatus) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new("fgc — Fast Git Cloner")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).title(" Dashboard "));
    frame.render_widget(title, chunks[0]);

    let info = format!(
        "Strategy: {}  |  Elapsed: {:.0}s  |  Speed: {}",
        status.strategy,
        status.elapsed_secs(),
        if status.speed.is_empty() {
            "-"
        } else {
            &status.speed
        }
    );
    let url_line = format!("URL: {}", truncate(&status.url, 70));
    let info_widget = Paragraph::new(format!("{info}\n{url_line}"))
        .block(Block::default().borders(Borders::ALL).title(" Info "));
    frame.render_widget(info_widget, chunks[1]);

    let phase_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2); 6])
        .split(chunks[2]);

    for (i, phase) in status.phases.iter().enumerate() {
        let color = if phase.done {
            Color::Green
        } else if i == status.current_phase {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let label = format!(
            "[{}/6] {} {}",
            i + 1,
            phase.name,
            if phase.detail.is_empty() {
                String::new()
            } else {
                format!("({})", phase.detail)
            }
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::LEFT))
            .gauge_style(Style::default().fg(color))
            .percent(phase.percent as u16)
            .label(label);
        frame.render_widget(gauge, phase_chunks[i]);
    }

    let footer = Paragraph::new(format!(
        "Status: {}  |  Press 'q' to close dashboard",
        status.message
    ))
    .style(Style::default().fg(Color::Gray))
    .block(Block::default().borders(Borders::ALL).title(" Status "));
    frame.render_widget(footer, chunks[3]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
