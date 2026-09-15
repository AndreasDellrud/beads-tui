use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use beads_tui::{
    agent::{AgentKind, AgentSelection},
    app::{App, Screen, ScrollPane},
    bd::ListView,
    ui,
    work::{LaunchReport, LaunchTarget, WorkLauncher, WorkRequest},
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
    let mut agents = AgentSelection::load();
    if let Some(diagnostic) = agents.take_diagnostic() {
        app.report_work_started(diagnostic, None);
    }
    let launcher = WorkLauncher::default();

    loop {
        app.poll();
        app.tick(Instant::now());
        terminal.draw(|frame| ui::draw(frame, &mut app, agents.selected()))?;

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
            app.clear_work_success();
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
        app.clear_work_success();
        if key.code == KeyCode::Esc && app.dismiss_work_error() {
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
                KeyCode::Char('w') => {
                    start_work(&mut app, terminal, &launcher, agents.selected(), false)?
                }
                KeyCode::Char('W') => {
                    start_work(&mut app, terminal, &launcher, agents.selected(), true)?
                }
                KeyCode::Char('a') => shift_agent(&mut app, &mut agents, true),
                KeyCode::Char('A') => shift_agent(&mut app, &mut agents, false),
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
            KeyCode::Char('w') => {
                start_work(&mut app, terminal, &launcher, agents.selected(), false)?
            }
            KeyCode::Char('W') => {
                start_work(&mut app, terminal, &launcher, agents.selected(), true)?
            }
            KeyCode::Char('a') => shift_agent(&mut app, &mut agents, true),
            KeyCode::Char('A') => shift_agent(&mut app, &mut agents, false),
            KeyCode::Char('x') => app.clear_filter(),
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

fn start_work(
    app: &mut App,
    terminal: &mut ratatui::DefaultTerminal,
    launcher: &WorkLauncher,
    agent: Option<AgentKind>,
    force_foreground: bool,
) -> Result<()> {
    app.clear_work_feedback();
    let Some(agent) = agent else {
        app.report_work_error("no supported agent found; install `codex` or `claude`");
        return Ok(());
    };
    let Some(issue) = app.work_issue().cloned() else {
        app.report_work_error("no issue is selected");
        return Ok(());
    };
    let repository =
        std::env::current_dir().context("could not determine the current workspace")?;
    let request = match WorkRequest::for_issue(&issue, repository) {
        Ok(request) => request,
        Err(error) => {
            app.report_work_error(error);
            return Ok(());
        }
    };

    let target = if force_foreground {
        LaunchTarget::Foreground
    } else {
        LaunchTarget::detect()
    };
    let result = match target {
        LaunchTarget::Foreground => launch_foreground(terminal, launcher, &request, agent),
        LaunchTarget::Herdr { workspace_id } => launcher
            .launch_herdr(&request, agent, &workspace_id)
            .map_err(|error| anyhow::anyhow!("{error:#}")),
    };
    match result {
        Ok(LaunchReport { message, warning }) => {
            app.report_work_started(message, warning);
            app.refresh();
        }
        Err(error) => app.report_work_error(format!("{error:#}")),
    }
    Ok(())
}

fn launch_foreground(
    terminal: &mut ratatui::DefaultTerminal,
    launcher: &WorkLauncher,
    request: &WorkRequest,
    agent: AgentKind,
) -> Result<LaunchReport> {
    while_terminal_suspended(
        || {
            execute!(std::io::stdout(), DisableMouseCapture)
                .context("could not release mouse capture before starting the agent")?;
            ratatui::restore();
            Ok(())
        },
        || launcher.launch_foreground(request, agent),
        || {
            *terminal = ratatui::init();
            execute!(std::io::stdout(), EnableMouseCapture)
                .context("could not restore mouse capture after the agent exited")?;
            Ok(())
        },
    )
}

fn shift_agent(app: &mut App, agents: &mut AgentSelection, forward: bool) {
    app.clear_work_feedback();
    let result = if forward {
        agents.next_agent()
    } else {
        agents.previous_agent()
    };
    match result {
        Ok(Some(agent)) => {
            app.report_work_started(format!("Default agent: {}", agent.display_name()), None)
        }
        Ok(None) => app.report_work_error("no supported agent found; install `codex` or `claude`"),
        Err(error) => app.report_work_error(format!("could not save default agent: {error}")),
    }
}

fn while_terminal_suspended<T>(
    suspend: impl FnOnce() -> Result<()>,
    operation: impl FnOnce() -> Result<T>,
    resume: impl FnOnce() -> Result<()>,
) -> Result<T> {
    suspend()?;
    let result = operation();
    resume()?;
    result
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::bail;

    use super::while_terminal_suspended;

    #[test]
    fn terminal_is_resumed_when_the_child_cannot_start() {
        let suspended = Cell::new(false);
        let resumed = Cell::new(false);

        let result: anyhow::Result<()> = while_terminal_suspended(
            || {
                suspended.set(true);
                Ok(())
            },
            || bail!("spawn failed"),
            || {
                resumed.set(true);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert!(suspended.get());
        assert!(resumed.get());
    }
}
