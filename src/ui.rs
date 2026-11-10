use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

use crate::{
    agent::AgentKind,
    app::{App, Screen, ScrollPane},
    bd::Issue,
    update::UpdateState,
};

const INK: Color = Color::Rgb(205, 214, 244);
const MUTED: Color = Color::Rgb(127, 132, 156);
const SURFACE: Color = Color::Rgb(49, 50, 68);
const MAUVE: Color = Color::Rgb(203, 166, 247);
const BLUE: Color = Color::Rgb(137, 180, 250);
const GREEN: Color = Color::Rgb(166, 227, 161);
const YELLOW: Color = Color::Rgb(249, 226, 175);
const RED: Color = Color::Rgb(243, 139, 168);

pub fn draw(frame: &mut Frame, app: &mut App, agent: Option<AgentKind>) {
    if app.screen == Screen::Issue {
        draw_issue_screen(frame, app, agent);
    } else {
        draw_browser_screen(frame, app, agent);
    }
    draw_update_notice(frame, app.updates.state());
    if app.update_info_open {
        draw_update_overlay(
            frame,
            app.updates.state(),
            app.updates.remaining_cooldown(),
            &mut app.update_info_scroll,
        );
    }
}

fn draw_browser_screen(frame: &mut Frame, app: &mut App, agent: Option<AgentKind>) {
    let page = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(frame.area());

    draw_header(frame, page[0], app);

    let (list, detail) = browser_panes(page[1]);
    draw_list(frame, list, app);
    draw_detail(frame, detail, app);
    let (status_area, keybindings_area) = footer_areas(page[2]);
    draw_browser_status(frame, status_area, app);
    draw_keybindings(frame, keybindings_area, browser_keybindings(agent));
}

fn draw_issue_screen(frame: &mut Frame, app: &mut App, agent: Option<AgentKind>) {
    let page = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(frame.area());
    let issue_id = app.detail.as_ref().map_or("", |issue| issue.id.as_str());
    let loading = if app.loading_detail {
        " · loading…"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ◈ BEAD ",
                Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                issue_id.to_owned(),
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(loading, Style::default().fg(BLUE)),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(SURFACE)),
        ),
        page[0],
    );

    let (body_area, relationships_area) = issue_panes(page[1]);
    let text = app.detail.as_ref().map_or_else(
        || {
            Text::from(Line::styled(
                "No issue selected",
                Style::default().fg(MUTED),
            ))
        },
        issue_text,
    );
    let body_block = panel(" Issue detail ".to_owned());
    let body_inner = body_block.inner(body_area);
    let body_lines = Paragraph::new(text.clone())
        .wrap(Wrap { trim: false })
        .line_count(body_inner.width);
    app.update_issue_scroll_max(
        body_lines
            .saturating_sub(usize::from(body_inner.height))
            .min(usize::from(u16::MAX)) as u16,
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(body_block)
            .wrap(Wrap { trim: false })
            .scroll((app.issue_scroll, 0)),
        body_area,
    );

    let relationship_block = panel(" Relationships ".to_owned());
    let relationship_inner = relationship_block.inner(relationships_area);
    app.update_relationship_viewport(usize::from(relationship_inner.height / 2).max(1));
    let relationships = app.relationships();
    let items: Vec<ListItem> = relationships
        .iter()
        .enumerate()
        .skip(app.relationship_scroll)
        .map(|(index, (direction, related))| {
            let style = if index == app.relationship_index {
                Style::default()
                    .bg(SURFACE)
                    .fg(INK)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            ListItem::new(vec![
                Line::styled(
                    format!("{} · {}", direction, related.dependency_type),
                    Style::default().fg(BLUE),
                ),
                Line::styled(format!("{}  {}", related.id, related.title), style),
            ])
        })
        .collect();
    let relationship_widget = if items.is_empty() {
        List::new([ListItem::new(Line::styled(
            "No relationships",
            Style::default().fg(MUTED),
        ))])
    } else {
        List::new(items)
    };
    frame.render_widget(
        relationship_widget.block(relationship_block),
        relationships_area,
    );

    let (status_area, keybindings_area) = footer_areas(page[2]);
    let status = if let Some(status) = work_status_line(app) {
        status
    } else if let Some(error) = &app.error {
        Line::styled(
            format!(" error · {error} · r retry "),
            Style::default().fg(RED),
        )
    } else if let Some(error) = &app.auto_refresh_error {
        Line::styled(
            format!(" Auto-refresh: {error} · r still refreshes "),
            Style::default().fg(RED),
        )
    } else if let Some(error) = &app.event_refresh_error {
        event_status_line(error)
    } else {
        Line::default()
    };
    frame.render_widget(Paragraph::new(status), status_area);
    draw_keybindings(frame, keybindings_area, issue_keybindings(agent));
}

fn draw_update_notice(frame: &mut Frame, state: &UpdateState) {
    if let UpdateState::Available(notice) = state {
        let area = frame.area();
        if area.height >= 3 {
            frame.render_widget(
                Paragraph::new(format!(" u · btui {} available ", notice.available))
                    .style(Style::default().fg(GREEN)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }
}

fn update_text(state: &UpdateState, cooldown: std::time::Duration) -> Text<'static> {
    let mut lines = vec![Line::from(format!(
        "Installed: {}",
        env!("CARGO_PKG_VERSION")
    ))];
    match state {
        UpdateState::Disabled => lines.push(Line::from(
            "Checks disabled: BTUI_UPDATE_CHECK=0 or this is a prerelease/development build.",
        )),
        UpdateState::Checking => {
            lines.push(Line::from("Checking GitHub for the latest stable release…"))
        }
        UpdateState::UpToDate => lines.push(Line::from("No newer stable release is available.")),
        UpdateState::Available(notice) => {
            lines[0] = Line::from(format!("Installed: {}", notice.installed));
            lines.push(Line::styled(
                format!("Available: {}", notice.available),
                Style::default().fg(GREEN),
            ));
            lines.push(Line::from(notice.url.clone()));
        }
        UpdateState::Failed(error) => {
            lines.push(Line::styled(
                "Update check failed; browsing remains available.",
                Style::default().fg(YELLOW),
            ));
            lines.push(Line::from(error.clone()));
        }
    }
    lines.push(Line::default());
    if !matches!(state, UpdateState::Disabled | UpdateState::Checking) {
        lines.push(Line::from(if cooldown.is_zero() {
            "r checks again".to_owned()
        } else {
            format!(
                "Recheck available in {} min (15 min between attempts).",
                cooldown.as_secs().div_ceil(60)
            )
        }));
    }
    lines.extend([
        Line::from("One startup check; no periodic checks or persistent cache."),
        Line::default(),
        Line::styled("Update manually", Style::default().fg(MAUVE).add_modifier(Modifier::BOLD)),
        Line::from("Releases: https://github.com/AndreasDellrud/beads-tui/releases/latest"),
        Line::from("Binary: download the matching archive and checksum, verify SHA-256, then replace your installed btui after exiting."),
        Line::from("Source: obtain the desired release checkout, then run cargo install --path . --locked there."),
        Line::from("Package manager: use the same manager that installed btui; availability may lag GitHub."),
        Line::from("btui never downloads or installs an update. Disable checks with BTUI_UPDATE_CHECK=0."),
    ]);
    Text::from(lines)
}

fn draw_update_overlay(
    frame: &mut Frame,
    state: &UpdateState,
    cooldown: std::time::Duration,
    scroll: &mut u16,
) {
    let area = frame.area();
    // Keep the information reachable even when the terminal cannot fit the whole panel.
    let width = area.width.min(88);
    let height = area.height.min(24);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = panel(" btui updates ".to_owned());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let [content, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let paragraph = Paragraph::new(update_text(state, cooldown))
        .style(Style::default().fg(INK))
        .wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(content.width)
        .saturating_sub(usize::from(content.height))
        .min(usize::from(u16::MAX)) as u16;
    *scroll = (*scroll).min(max_scroll);
    frame.render_widget(paragraph.scroll((*scroll, 0)), content);
    frame.render_widget(
        Paragraph::new("u/esc close · r recheck · ↑/↓ scroll · q quit")
            .style(Style::default().fg(MUTED)),
        help,
    );
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let count = format!(
        " {}/{} issue{} · {} · {}{} ",
        app.visible.len(),
        app.issues.len(),
        if app.issues.len() == 1 { "" } else { "s" },
        app.view.label(),
        app.sort.label(),
        if app.showing_refresh {
            " · refreshing…"
        } else {
            ""
        }
    );
    let title = Line::from(vec![
        Span::styled(
            " ◈ ",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "BEADS",
            Style::default().fg(INK).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  task browser", Style::default().fg(MUTED)),
        Span::styled(count, Style::default().fg(BLUE)),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(SURFACE)),
        ),
        area,
    );
}

fn draw_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = panel(if app.filter.is_empty() {
        " Issues ".to_owned()
    } else {
        format!(" Issues · /{} · x clear ", app.filter)
    });
    let inner = block.inner(area);
    app.update_list_viewport(usize::from(inner.height));
    let selected = app.list_state.selected();
    let items = app
        .visible
        .iter()
        .enumerate()
        .skip(app.list_scroll)
        .map(|(position, index)| {
            let issue = &app.issues[*index];
            let is_selected = Some(position) == selected;
            let selected_style = if is_selected {
                Style::default()
                    .bg(SURFACE)
                    .fg(INK)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(if is_selected { "▌" } else { " " }, selected_style),
                Span::styled(
                    format!("P{} ", issue.priority),
                    selected_row_style(priority_style(issue.priority), is_selected),
                ),
                Span::styled(
                    status_symbol(&issue.status),
                    selected_row_style(
                        Style::default().fg(status_color(&issue.status)),
                        is_selected,
                    ),
                ),
                Span::styled(
                    format!(" {}  ", issue.id),
                    selected_row_style(Style::default().fg(MUTED), is_selected),
                ),
                Span::styled(&issue.title, selected_style),
            ]))
        });

    frame.render_widget(List::new(items).block(block), area);
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &mut App) {
    let text = match &app.detail {
        Some(issue) => issue_text(issue),
        None if app.error.is_some() => Text::from(Line::styled(
            "Beads is unavailable",
            Style::default().fg(RED),
        )),
        None => Text::from(Line::styled(
            "No issues match this view",
            Style::default().fg(MUTED),
        )),
    };
    let block = panel(" Details ".to_owned());
    let inner = block.inner(area);
    let lines = Paragraph::new(text.clone())
        .wrap(Wrap { trim: false })
        .line_count(inner.width);
    app.update_preview_scroll_max(
        lines
            .saturating_sub(usize::from(inner.height))
            .min(usize::from(u16::MAX)) as u16,
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0)),
        area,
    );
}

pub fn scroll_target(area: Rect, screen: Screen, column: u16, row: u16) -> Option<ScrollPane> {
    let page = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    let position = Position::new(column, row);
    let (first, second) = match screen {
        Screen::Browser => browser_panes(page[1]),
        Screen::Issue => issue_panes(page[1]),
    };
    if first.contains(position) {
        Some(match screen {
            Screen::Browser => ScrollPane::IssueList,
            Screen::Issue => ScrollPane::IssueBody,
        })
    } else if second.contains(position) {
        Some(match screen {
            Screen::Browser => ScrollPane::Preview,
            Screen::Issue => ScrollPane::Relationships,
        })
    } else {
        None
    }
}

fn browser_panes(area: Rect) -> (Rect, Rect) {
    split_panes(area, 42)
}

fn issue_panes(area: Rect) -> (Rect, Rect) {
    split_panes(area, 70)
}

fn split_panes(area: Rect, first_percentage: u16) -> (Rect, Rect) {
    let direction = if area.width >= 100 {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let panes = Layout::default()
        .direction(direction)
        .constraints([
            Constraint::Percentage(first_percentage),
            Constraint::Percentage(100 - first_percentage),
        ])
        .split(area);
    (panes[0], panes[1])
}

fn issue_text(issue: &Issue) -> Text<'static> {
    let mut lines = vec![
        Line::styled(
            issue.title.clone(),
            Style::default().fg(INK).add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled(
                format!(" {} ", issue.status.replace('_', " ")),
                Style::default()
                    .bg(status_color(&issue.status))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("P{}", issue.priority),
                priority_style(issue.priority),
            ),
            Span::styled(
                format!("  {}  {}", issue.issue_type, issue.id),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::default(),
    ];

    section(&mut lines, "Description", &issue.description);
    section(&mut lines, "Acceptance", &issue.acceptance_criteria);
    section(&mut lines, "Design", &issue.design);
    section(&mut lines, "Notes", &issue.notes);

    if !issue.labels.is_empty() {
        section(&mut lines, "Labels", &issue.labels.join(" · "));
    }
    if !issue.assignee.is_empty() {
        section(&mut lines, "Assignee", &issue.assignee);
    }
    if !issue.owner.is_empty() {
        section(&mut lines, "Owner", &issue.owner);
    }

    if issue.comments.is_empty() {
        lines.push(Line::styled(
            "Comments",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled("No comments", Style::default().fg(MUTED)));
        lines.push(Line::default());
    } else {
        lines.push(Line::styled(
            "Comments",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));
        for comment in &issue.comments {
            lines.push(Line::styled(
                format!("{} · {}", comment.author, comment.created_at),
                Style::default().fg(MUTED),
            ));
            lines.push(Line::raw(comment.text.clone()));
            lines.push(Line::default());
        }
    }

    lines.push(Line::styled(
        format!(
            "{} dependencies · {} dependents · {} comments",
            issue.dependency_count, issue.dependent_count, issue.comment_count
        ),
        Style::default().fg(MUTED),
    ));
    if !issue.updated_at.is_empty() {
        lines.push(Line::styled(
            format!("Updated {}", issue.updated_at),
            Style::default().fg(MUTED),
        ));
    }
    if !issue.created_at.is_empty() {
        lines.push(Line::styled(
            format!("Created {}", issue.created_at),
            Style::default().fg(MUTED),
        ));
    }
    if !issue.closed_at.is_empty() {
        lines.push(Line::styled(
            format!("Closed {}", issue.closed_at),
            Style::default().fg(MUTED),
        ));
    }

    Text::from(lines)
}

fn section(lines: &mut Vec<Line<'static>>, heading: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    lines.push(Line::styled(
        heading.to_owned(),
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::raw(body.to_owned()));
    lines.push(Line::default());
}

fn footer_areas(area: Rect) -> (Rect, Rect) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    (rows[0], rows[1])
}

fn work_status_line(app: &App) -> Option<Line<'static>> {
    let pending = app.pending_work_message();
    let error = app.work_error().map(|error| {
        if app.work_error_count() > 1 {
            format!("{} errors · {error}", app.work_error_count())
        } else {
            error.to_owned()
        }
    });
    let outcome = error.as_deref().or(app.work_message.as_deref());
    let message = match (pending.as_deref(), outcome) {
        (Some(pending), Some(outcome)) => Some(format!("{pending} · {outcome}")),
        (Some(pending), None) => Some(pending.to_owned()),
        (None, Some(outcome)) => Some(outcome.to_owned()),
        (None, None) => None,
    };
    if error.is_some() {
        launch_status_line(None, message.as_deref())
    } else {
        launch_status_line(message.as_deref(), None)
    }
}

fn launch_status_line(message: Option<&str>, error: Option<&str>) -> Option<Line<'static>> {
    if let Some(error) = error {
        Some(Line::from(vec![
            Span::styled(
                " Work error ",
                Style::default()
                    .bg(RED)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {error} · esc dismiss · w retry · shift-w retry here "),
                Style::default().fg(RED),
            ),
        ]))
    } else {
        message.map(|message| {
            Line::from(vec![
                Span::styled(
                    " Work ",
                    Style::default()
                        .bg(GREEN)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {message} "), Style::default().fg(GREEN)),
            ])
        })
    }
}

fn event_status_line(error: &str) -> Line<'static> {
    Line::styled(
        format!(" Event feed: {error} · periodic refresh continues "),
        Style::default().fg(RED),
    )
}

fn draw_browser_status(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(status) = work_status_line(app) {
        status
    } else if let Some(error) = &app.error {
        Line::from(vec![
            Span::styled(
                " error ",
                Style::default()
                    .bg(RED)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {error}"), Style::default().fg(RED)),
        ])
    } else if let Some(error) = &app.auto_refresh_error {
        Line::from(vec![
            Span::styled(
                " auto-refresh paused ",
                Style::default()
                    .bg(RED)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {error} · r still refreshes "),
                Style::default().fg(RED),
            ),
        ])
    } else if let Some(error) = &app.event_refresh_error {
        event_status_line(error)
    } else if app.filtering {
        Line::from(vec![
            Span::styled(
                " / ",
                Style::default()
                    .bg(MAUVE)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}_", app.filter), Style::default().fg(INK)),
            Span::styled("   enter accept · esc close", Style::default().fg(MUTED)),
        ])
    } else if app.showing_refresh {
        Line::from(vec![
            Span::styled(
                " refreshing ",
                Style::default()
                    .bg(BLUE)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " bd list is loading in the background · navigation and q remain available ",
                Style::default().fg(MUTED),
            ),
        ])
    } else {
        Line::default()
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_keybindings(frame: &mut Frame, area: Rect, line: Line<'static>) {
    frame.render_widget(Paragraph::new(line), area);
}

// Labels are shared by rendering and hit testing, including Unicode cell widths.
fn controls(screen: Screen, agent: Option<AgentKind>) -> Vec<(String, KeyCode)> {
    let agent = agent.map_or("no agent", AgentKind::display_name);
    let mut controls = match screen {
        Screen::Browser => vec![
            ("enter details".into(), KeyCode::Enter),
            ("1 active".into(), KeyCode::Char('1')),
            ("2 ready".into(), KeyCode::Char('2')),
            ("3 closed".into(), KeyCode::Char('3')),
            ("s sort".into(), KeyCode::Char('s')),
            ("/ filter".into(), KeyCode::Char('/')),
            ("x clear".into(), KeyCode::Char('x')),
        ],
        Screen::Issue => vec![
            ("esc back".into(), KeyCode::Esc),
            ("enter follow".into(), KeyCode::Enter),
            ("tab next".into(), KeyCode::Tab),
            ("shift-tab previous".into(), KeyCode::BackTab),
        ],
    };
    controls.extend([
        (format!("w start {agent}"), KeyCode::Char('w')),
        ("a/A agent".into(), KeyCode::Char('a')),
        ("r refresh".into(), KeyCode::Char('r')),
        ("q quit".into(), KeyCode::Char('q')),
        ("u updates".into(), KeyCode::Char('u')),
    ]);
    controls
}

fn keybindings(screen: Screen, agent: Option<AgentKind>) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (label, _)) in controls(screen, agent).into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::raw(label));
    }
    Line::from(spans).style(Style::default().fg(MUTED))
}

fn browser_keybindings(agent: Option<AgentKind>) -> Line<'static> {
    keybindings(Screen::Browser, agent)
}

fn issue_keybindings(agent: Option<AgentKind>) -> Line<'static> {
    keybindings(Screen::Issue, agent)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    Issue(usize),
    Relationship(usize),
    Key(KeyCode),
}

pub fn click_target(
    area: Rect,
    app: &App,
    agent: Option<AgentKind>,
    column: u16,
    row: u16,
) -> Option<ClickTarget> {
    let page = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    let position = Position::new(column, row);
    let (_, footer) = footer_areas(page[2]);
    if footer.contains(position) {
        let mut x = footer.x.saturating_add(1);
        for (label, key) in controls(app.screen, agent) {
            let width = Span::raw(label).width() as u16;
            let end = x.saturating_add(width);
            // A clipped control must not activate a partially hidden action.
            if end <= footer.right() && column >= x && column < end {
                return Some(ClickTarget::Key(key));
            }
            x = end.saturating_add(3);
        }
        return None;
    }
    let (pane, offset, count, height) = match app.screen {
        Screen::Browser => (
            browser_panes(page[1]).0,
            app.list_scroll,
            app.visible.len(),
            1,
        ),
        Screen::Issue => (
            issue_panes(page[1]).1,
            app.relationship_scroll,
            app.relationships().len(),
            2,
        ),
    };
    let inner = panel(String::new()).inner(pane);
    if !inner.contains(position) {
        return None;
    }
    let visible_row = (row - inner.y) / height;
    if visible_row >= inner.height / height {
        return None;
    }
    let index = offset + usize::from(visible_row);
    if index >= count {
        return None;
    }
    Some(match app.screen {
        Screen::Browser => ClickTarget::Issue(index),
        Screen::Issue => ClickTarget::Relationship(index),
    })
}

fn panel(title: String) -> Block<'static> {
    Block::bordered()
        .title(title)
        .title_style(Style::default().fg(MAUVE).add_modifier(Modifier::BOLD))
        .border_style(Style::default().fg(SURFACE))
        .padding(Padding::horizontal(1))
}

fn status_symbol(status: &str) -> &'static str {
    match status {
        "in_progress" => "◆",
        "blocked" => "■",
        "closed" => "✓",
        "deferred" => "◷",
        _ => "●",
    }
}

fn status_color(status: &str) -> Color {
    match status {
        "in_progress" => BLUE,
        "blocked" => RED,
        "closed" => GREEN,
        "deferred" => MUTED,
        _ => YELLOW,
    }
}

fn priority_style(priority: u8) -> Style {
    let color = match priority {
        0 => RED,
        1 => Color::Rgb(250, 179, 135),
        2 => YELLOW,
        3 => BLUE,
        _ => MUTED,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn selected_row_style(style: Style, selected: bool) -> Style {
    if selected { style.bg(SURFACE) } else { style }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_updates(state: &UpdateState, width: u16, height: u16, scroll: &mut u16) -> String {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw_update_overlay(frame, state, std::time::Duration::from_secs(900), scroll)
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn overlay_explains_every_checker_state_without_an_install_action() {
        let cases = [
            (UpdateState::Disabled, "Checks disabled"),
            (UpdateState::Checking, "Checking GitHub"),
            (UpdateState::UpToDate, "No newer stable"),
            (
                UpdateState::Failed("network offline".into()),
                "network offline",
            ),
            (
                UpdateState::Available(crate::update::ReleaseNotice {
                    installed: "0.4.3".into(),
                    available: "0.5.0".into(),
                    url: "https://github.com/AndreasDellrud/beads-tui/releases/tag/v0.5.0".into(),
                }),
                "Available: 0.5.0",
            ),
        ];
        for (state, expected) in cases {
            let text = rendered_updates(&state, 100, 30, &mut 0);
            assert!(text.contains(expected), "missing {expected}");
            assert!(text.contains("Installed:"));
            assert!(text.contains("Update manually"));
            assert!(text.contains("u/esc close"));
            assert!(!text.contains("press i"));
        }
    }

    #[test]
    fn available_release_link_and_recheck_cooldown_are_visible() {
        let url = "https://github.com/AndreasDellrud/beads-tui/releases/tag/v0.5.0";
        let state = UpdateState::Available(crate::update::ReleaseNotice {
            installed: "0.4.3".into(),
            available: "0.5.0".into(),
            url: url.into(),
        });
        let text = rendered_updates(&state, 100, 30, &mut 0);
        assert!(text.contains(url));
        assert!(text.contains("Recheck available in 15 min"));
    }

    #[test]
    fn short_overlay_scroll_reaches_manual_guidance_and_clamps() {
        let state = UpdateState::Failed("offline".into());
        let mut scroll = u16::MAX;
        let text = rendered_updates(&state, 55, 12, &mut scroll);
        assert!(text.contains("BTUI_UPDATE_CHECK=0"));
        assert!(scroll > 0 && scroll < u16::MAX);
        for (width, height) in [(1, 1), (10, 3), (40, 8)] {
            rendered_updates(&state, width, height, &mut scroll);
        }
    }

    #[test]
    fn only_available_releases_show_a_header_notice() {
        for state in [
            UpdateState::Failed("offline".into()),
            UpdateState::UpToDate,
            UpdateState::Available(crate::update::ReleaseNotice {
                installed: "0.4.3".into(),
                available: "0.5.0".into(),
                url: "https://example.invalid".into(),
            }),
        ] {
            let backend = ratatui::backend::TestBackend::new(70, 20);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| draw_update_notice(frame, &state))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert_eq!(
                text.contains("u · btui 0.5.0 available"),
                matches!(state, UpdateState::Available(_))
            );
        }
    }

    #[test]
    fn launch_status_is_separate_from_persistent_keybindings() {
        let success = launch_status_line(Some("Started Codex"), None).unwrap();
        let error = launch_status_line(None, Some("could not focus")).unwrap();

        assert!(success.to_string().contains("Started Codex"));
        assert!(error.to_string().contains("esc dismiss"));
        assert!(
            browser_keybindings(Some(AgentKind::Claude))
                .to_string()
                .contains("w start Claude Code")
        );
        assert!(
            browser_keybindings(None)
                .to_string()
                .contains("w start no agent")
        );
        assert!(
            issue_keybindings(Some(AgentKind::Codex))
                .to_string()
                .contains("esc back")
        );
    }

    #[test]
    fn footer_rows_stay_inside_short_terminals() {
        let area = Rect::new(0, 0, 60, 1);
        let (status, keybindings) = footer_areas(area);

        assert!(status.bottom() <= area.bottom());
        assert!(keybindings.bottom() <= area.bottom());
    }

    #[test]
    fn mouse_target_tracks_wide_browser_panes() {
        let area = Rect::new(0, 0, 120, 40);

        assert_eq!(
            scroll_target(area, Screen::Browser, 10, 10),
            Some(ScrollPane::IssueList)
        );
        assert_eq!(
            scroll_target(area, Screen::Browser, 100, 10),
            Some(ScrollPane::Preview)
        );
        assert_eq!(scroll_target(area, Screen::Browser, 10, 1), None);
    }

    #[test]
    fn mouse_target_tracks_stacked_narrow_issue_panes() {
        let area = Rect::new(0, 0, 70, 40);

        assert_eq!(
            scroll_target(area, Screen::Issue, 10, 8),
            Some(ScrollPane::IssueBody)
        );
        assert_eq!(
            scroll_target(area, Screen::Issue, 10, 32),
            Some(ScrollPane::Relationships)
        );
    }
}
