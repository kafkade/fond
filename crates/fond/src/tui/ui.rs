//! TUI rendering for cook mode.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};

use fond_timeline::duration::format_duration;

use super::app::CookApp;

/// Whether to use color styling (respects NO_COLOR env).
fn use_color() -> bool {
    std::env::var("NO_COLOR").is_err()
}

/// Render the full TUI layout.
pub fn render(frame: &mut Frame, app: &CookApp) {
    let area = frame.area();

    // Minimum usable size
    if area.width < 40 || area.height < 12 {
        let msg = Paragraph::new("Terminal too small\nResize to at least 40×12")
            .style(Style::default().fg(Color::Red));
        frame.render_widget(msg, area);
        return;
    }

    // Outer layout: header, body, footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(frame, app, outer[0]);

    // Body: main panel + optional timeline rail
    let show_timeline = app.schedule.is_some() && area.width >= 80;
    if show_timeline {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);
        render_main_panel(frame, app, body[0]);
        render_timeline(frame, app, body[1]);
    } else {
        render_main_panel(frame, app, outer[1]);
    }

    render_footer(frame, app, outer[2]);

    // Quit confirmation overlay
    if app.quit_confirm {
        render_quit_dialog(frame, area);
    }
}

fn render_header(frame: &mut Frame, app: &CookApp, area: Rect) {
    let colored = use_color();
    let title_style = if colored {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };

    let mut spans = vec![Span::styled(
        format!(" fond cook: {}", app.recipe.title),
        title_style,
    )];

    if let Some(ref sched) = app.schedule {
        let serve_str = format!("  Serve at {}", sched.serve_at.format("%H:%M"));
        spans.push(Span::raw(serve_str));
    }

    let header = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(if colored {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            }),
    );
    frame.render_widget(header, area);
}

fn render_main_panel(frame: &mut Frame, app: &CookApp, area: Rect) {
    let colored = use_color();

    // Split main panel: step info, step body, timer, ingredients
    let step = app.recipe.steps.get(app.current_step);
    let has_timer = app.current_step_timer().is_some()
        || (app.current_step_has_timer() && app.current_step_timer().is_none());

    let constraints = if has_timer {
        vec![
            Constraint::Length(2), // step counter
            Constraint::Min(4),    // step body
            Constraint::Length(4), // timer
        ]
    } else {
        vec![
            Constraint::Length(2), // step counter
            Constraint::Min(6),    // step body
        ]
    };

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Step counter
    let step_num = app.current_step + 1;
    let section_info = step
        .and_then(|s| s.section.as_deref())
        .map(|s| format!(" — {s}"))
        .unwrap_or_default();
    let counter_style = if colored {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let counter = Paragraph::new(Line::from(vec![Span::styled(
        format!("  Step {step_num} of {}{section_info}", app.total_steps),
        counter_style,
    )]));
    frame.render_widget(counter, inner[0]);

    // Step body
    let body_text = step.map(|s| s.body.as_str()).unwrap_or("");
    let body_block = Block::default()
        .borders(Borders::ALL)
        .title(" Instructions ")
        .border_style(if colored {
            Style::default().fg(Color::Blue)
        } else {
            Style::default()
        });
    let body = Paragraph::new(body_text)
        .block(body_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(body, inner[1]);

    // Timer bar (if applicable)
    if has_timer && inner.len() > 2 {
        render_timer_section(frame, app, inner[2]);
    }
}

fn render_timer_section(frame: &mut Frame, app: &CookApp, area: Rect) {
    let colored = use_color();

    if let Some(timer) = app.current_step_timer() {
        // Active timer — show countdown gauge
        let remaining = timer.remaining_secs();
        let mins = remaining / 60;
        let secs = remaining % 60;
        let status = if timer.is_finished() {
            " DONE!".to_string()
        } else if timer.is_paused() {
            format!(" {mins:02}:{secs:02} PAUSED")
        } else {
            format!(" {mins:02}:{secs:02} remaining")
        };

        let gauge_color = if timer.is_finished() {
            Color::Green
        } else if timer.is_paused() {
            Color::Yellow
        } else {
            Color::Cyan
        };

        let label = format!("{} ({})", timer.label, format_duration(timer.total_seconds));

        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Timer: {label} "))
                    .border_style(if colored {
                        Style::default().fg(gauge_color)
                    } else {
                        Style::default()
                    }),
            )
            .gauge_style(if colored {
                Style::default().fg(gauge_color)
            } else {
                Style::default()
            })
            .ratio(timer.progress())
            .label(Span::raw(status));

        frame.render_widget(gauge, area);
    } else {
        // Timer available but not started
        let hint = if colored {
            Paragraph::new("  Press Space to start timer")
                .style(Style::default().fg(Color::DarkGray))
        } else {
            Paragraph::new("  Press Space to start timer")
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Timer ")
            .border_style(if colored {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            });
        frame.render_widget(hint.block(block), area);
    }
}

fn render_timeline(frame: &mut Frame, app: &CookApp, area: Rect) {
    let colored = use_color();

    let sched = match &app.schedule {
        Some(s) => s,
        None => return,
    };

    let items: Vec<ListItem> = sched
        .nodes
        .iter()
        .map(|sn| {
            let node = &sn.node;
            let is_current = node.step_index as usize == app.current_step;
            let is_completed = (node.step_index as usize) < app.completed_steps.len()
                && app.completed_steps[node.step_index as usize];

            let time_str = sn.scheduled_start.format("%H:%M").to_string();
            let dur_str = node
                .duration
                .as_ref()
                .map(|d| format_duration(d.seconds))
                .unwrap_or_default();

            let marker = if is_current {
                "▶"
            } else if is_completed {
                "✓"
            } else {
                " "
            };

            // Truncate label to fit
            let max_label = (area.width as usize).saturating_sub(18);
            let label = if node.label.len() > max_label {
                format!("{}…", &node.label[..max_label.saturating_sub(1)])
            } else {
                node.label.clone()
            };

            let line_text = if dur_str.is_empty() {
                format!("{marker} {time_str} {label}")
            } else {
                format!("{marker} {time_str} {label} ({dur_str})")
            };

            let style = if is_current && colored {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_completed && colored {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(Span::styled(line_text, style)))
        })
        .collect();

    let timeline_block = Block::default()
        .borders(Borders::ALL)
        .title(" Timeline ")
        .border_style(if colored {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        });

    let list = List::new(items).block(timeline_block);
    frame.render_widget(list, area);
}

fn render_footer(frame: &mut Frame, app: &CookApp, area: Rect) {
    let colored = use_color();

    let elapsed = app.elapsed();
    let elapsed_mins = elapsed.as_secs() / 60;
    let elapsed_secs = elapsed.as_secs() % 60;

    // Running timers indicator
    let active_timers = app
        .running_timers
        .iter()
        .filter(|t| !t.is_finished() && !t.is_paused())
        .count();
    let timer_indicator = if active_timers > 0 {
        format!(" | {active_timers} timer(s) running")
    } else {
        String::new()
    };

    let key_style = if colored {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };

    let spans = vec![
        Span::styled(" ←/→", key_style),
        Span::raw(" navigate  "),
        Span::styled("Space", key_style),
        Span::raw(" timer  "),
        Span::styled("q", key_style),
        Span::raw(" quit"),
        Span::raw(format!(
            "      Elapsed: {elapsed_mins:02}:{elapsed_secs:02}{timer_indicator}"
        )),
    ];

    let footer = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(if colored {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            }),
    );
    frame.render_widget(footer, area);
}

fn render_quit_dialog(frame: &mut Frame, area: Rect) {
    let colored = use_color();

    // Center a dialog box
    let dialog_width = 40.min(area.width.saturating_sub(4));
    let dialog_height = 5.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear background
    let clear = Paragraph::new("").style(Style::default().bg(Color::Black));
    frame.render_widget(clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Quit? ")
        .border_style(if colored {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        });

    let msg = Paragraph::new(" Press Y to quit, any other key to cancel")
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(msg, dialog_area);
}
