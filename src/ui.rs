use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Padding, Paragraph, Wrap},
};

use crate::{
    app::{App, Screen, ScrollPane},
    bd::Issue,
};

const INK: Color = Color::Rgb(205, 214, 244);
const MUTED: Color = Color::Rgb(127, 132, 156);
const SURFACE: Color = Color::Rgb(49, 50, 68);
const MAUVE: Color = Color::Rgb(203, 166, 247);
const BLUE: Color = Color::Rgb(137, 180, 250);
const GREEN: Color = Color::Rgb(166, 227, 161);
const YELLOW: Color = Color::Rgb(249, 226, 175);
const RED: Color = Color::Rgb(243, 139, 168);

pub fn draw(frame: &mut Frame, app: &mut App) {
    if app.screen == Screen::Issue {
        draw_issue_screen(frame, app);
        return;
    }
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
    draw_footer(frame, page[2], app);
}

fn draw_issue_screen(frame: &mut Frame, app: &mut App) {
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

    let footer = if let Some(error) = &app.error {
        Line::styled(
            format!(" error · {error} · r retry · esc back "),
            Style::default().fg(RED),
        )
    } else {
        Line::styled(
            " wheel/↑↓/jk scroll detail   tab/shift-tab relationships   enter follow   r reload   esc back   q quit ",
            Style::default().fg(MUTED),
        )
    };
    frame.render_widget(Paragraph::new(footer), page[2]);
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
        format!(" Issues · /{} · c clear ", app.filter)
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

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(error) = &app.error {
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
        Line::styled(
            " ↑↓/jk navigate   1 active  2 ready  3 closed   s sort   / filter   c clear   r refresh   q quit ",
            Style::default().fg(MUTED),
        )
    };
    frame.render_widget(Paragraph::new(line), area);
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
