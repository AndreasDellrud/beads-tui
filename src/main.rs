use std::time::{Duration, Instant};

use anyhow::Result;
use beads_tui::{
    app::{App, Screen, ScrollPane},
    bd::ListView,
    ui,
};
use ratatui::{
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
            MouseEventKind,
        },
        execute,
    },
    layout::Rect,
};

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(error.into());
    }
    let result = run(&mut terminal);
    let mouse_result = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result?;
    mouse_result?;
    Ok(())
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    let mut app = App::load();

    loop {
        app.poll();
        app.tick(Instant::now());
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let input = event::read()?;
        if let Event::Mouse(mouse) = input {
            let delta: i16 = match mouse.kind {
                MouseEventKind::ScrollDown => 3,
                MouseEventKind::ScrollUp => -3,
                _ => continue,
            };
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            if let Some(pane) = ui::scroll_target(area, app.screen, mouse.column, mouse.row) {
                let delta = if pane == ScrollPane::Relationships {
                    delta.signum()
                } else {
                    delta
                };
                app.scroll(pane, delta);
            }
            continue;
        }

        let Event::Key(key) = input else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if app.screen == Screen::Issue {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Esc | KeyCode::Backspace => app.close_issue(),
                KeyCode::Down | KeyCode::Char('j') => app.scroll(ScrollPane::IssueBody, 2),
                KeyCode::Up | KeyCode::Char('k') => app.scroll(ScrollPane::IssueBody, -2),
                KeyCode::PageDown => app.scroll(ScrollPane::IssueBody, 8),
                KeyCode::PageUp => app.scroll(ScrollPane::IssueBody, -8),
                KeyCode::Tab => app.select_next_relationship(),
                KeyCode::BackTab => app.select_previous_relationship(),
                KeyCode::Enter => app.open_selected_relationship(),
                KeyCode::Char('r') => app.reload_issue(),
                _ => {}
            }
            continue;
        }

        if app.filtering {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => app.filtering = false,
                KeyCode::Backspace => app.pop_filter(),
                KeyCode::Char(character) => app.push_filter(character),
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('/') => app.filtering = true,
            KeyCode::Char('r') => app.refresh(),
            KeyCode::Enter => app.open_selected_issue(),
            KeyCode::Char('c') => app.clear_filter(),
            KeyCode::Char('s') => app.cycle_sort(),
            KeyCode::Char('1') => app.set_view(ListView::Active),
            KeyCode::Char('2') => app.set_view(ListView::Ready),
            KeyCode::Char('3') => app.set_view(ListView::Closed),
            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
            KeyCode::Home | KeyCode::Char('g') => app.select_first(),
            KeyCode::End | KeyCode::Char('G') => app.select_last(),
            KeyCode::PageDown => app.scroll(ScrollPane::Preview, 8),
            KeyCode::PageUp => app.scroll(ScrollPane::Preview, -8),
            _ => {}
        }
    }
}
