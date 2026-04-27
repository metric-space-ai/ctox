use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::prelude::Alignment;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Tabs;
use ratatui::widgets::Wrap;
use ratatui::Frame;

use crate::context_health::ContextHealthStatus;
use crate::inference::engine;

use super::compact_model_name;
use super::mask_secret;
use super::App;
use super::Page;
use super::SettingsView;

const CONTEXT_BAR_REFERENCE_TOKENS: usize = 262_144;

pub(super) fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        frame.area(),
    );
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(0),
        ])
        .split(area);

    render_header(frame, app, layout[0]);
    render_tabs(frame, app, layout[1]);
    match app.page {
        Page::Chat => render_chat(frame, app, layout[2]),
        Page::Skills => render_skills(frame, app, layout[2]),
        Page::Settings => render_settings(frame, app, layout[2]),
    }
    render_status(frame, app, layout[3]);
}

fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let width = area.width.max(24) as usize;
    let header = header_lines(app, width.saturating_sub(4));
    let widget = Paragraph::new(header)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default().bg(Color::Rgb(8, 8, 8)))
                .title(Span::styled(
                    " CTOX ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_tabs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let selected = match app.page {
        Page::Chat => 0,
        Page::Skills => 1,
        Page::Settings => 2,
    };
    let titles = ["Chat", "Skills", "Settings"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let widget = Tabs::new(titles)
        .select(selected)
        .divider(" ")
        .padding("", "")
        .style(Style::default().fg(Color::DarkGray).bg(Color::Rgb(8, 8, 8)))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(widget, area);
}

fn render_chat(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if area.width < 96 {
        render_chat_narrow(frame, app, area);
        return;
    }
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(body[0]);
    let right = body[1];

    let turn_widget = Paragraph::new(turn_summary_lines(
        app,
        left[0].width.saturating_sub(4) as usize,
        left[0].height.saturating_sub(2) as usize,
    ))
    .block(
        pane_block().borders(Borders::TOP).title(Span::styled(
            " turn ",
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: false });
    frame.render_widget(turn_widget, left[0]);

    let transcript_widget = Paragraph::new(render_transcript_lines(
        app,
        left[1].width.saturating_sub(4) as usize,
        left[1].height.saturating_sub(2) as usize,
    ))
    .block(
        pane_block().borders(Borders::TOP).title(Span::styled(
            " chat ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: false });
    frame.render_widget(transcript_widget, left[1]);

    let sidebar_widget = Paragraph::new(chat_sidebar_lines(
        app,
        right.width.saturating_sub(4) as usize,
        right.height.saturating_sub(2) as usize,
    ))
    .block(sidebar_block())
    .style(Style::default().fg(Color::Gray))
    .wrap(Wrap { trim: false });
    frame.render_widget(sidebar_widget, right);

    let composer_text = if app.chat_input.trim().is_empty() {
        if app.request_in_flight {
            "Type while CTOX is busy. Enter queues the draft.".to_string()
        } else {
            "Type your next instruction.".to_string()
        }
    } else {
        app.chat_input.clone()
    };
    let composer_title_base = if app.request_in_flight {
        " queued draft "
    } else {
        " compose "
    };
    let composer_title = if app.pending_images.is_empty() {
        composer_title_base.to_string()
    } else {
        format!(
            " {} 📎 {} image(s) ",
            composer_title_base.trim(),
            app.pending_images.len()
        )
    };
    let composer = Paragraph::new(composer_text)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(Span::styled(
                    composer_title,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ))
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default().bg(Color::Rgb(20, 20, 20))),
        )
        .style(if app.chat_input.trim().is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .wrap(Wrap { trim: false });
    frame.render_widget(composer, left[2]);
}

fn render_skills(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if area.width < 96 {
        render_skills_narrow(frame, app, area);
        return;
    }
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(area);

    let list_items = skill_list_items(
        app,
        body[0].width.saturating_sub(4) as usize,
        body[0].height.saturating_sub(2) as usize,
    );
    frame.render_widget(
        List::new(list_items).block(
            pane_block().borders(Borders::TOP).title(Span::styled(
                " skills ",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        body[0],
    );

    let details = Paragraph::new(skill_details_lines(
        app,
        body[1].width.saturating_sub(4) as usize,
        body[1].height.saturating_sub(2) as usize,
    ))
    .block(
        sidebar_block().borders(Borders::TOP).title(Span::styled(
            " selected skill ",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: false });
    frame.render_widget(details, body[1]);
}

fn render_settings(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if area.width < 96 {
        render_settings_narrow(frame, app, area);
        return;
    }
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8)])
        .split(area);
    render_settings_view_tabs(frame, app, outer[0]);
    if app.settings_view == SettingsView::Update {
        render_settings_update(frame, app, outer[1]);
        return;
    }
    if app.settings_view == SettingsView::HarnessMining {
        render_settings_harness_mining(frame, app, outer[1]);
        return;
    }
    if app.settings_view == SettingsView::Secrets {
        render_secrets(frame, app, outer[1]);
        return;
    }
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(outer[1]);

    let visible_indices = app.visible_setting_indices();
    let max_rows = body[0].height.saturating_sub(2) as usize;
    let window_indices = settings_window_indices(app, &visible_indices, max_rows);
    let items = window_indices
        .into_iter()
        .filter_map(|idx| app.settings_items.get(idx).map(|item| (idx, item)))
        .map(|(index, item)| {
            let rendered_value = app.rendered_setting_value(item);
            let base = format!("{:18} {}", item.label, truncate_line(&rendered_value, 44));
            let row_style = setting_row_style(item.key, item.value.trim());
            if index == app.settings_selected {
                ListItem::new(base).style(
                    row_style
                        .bg(Color::LightCyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(base).style(row_style)
            }
        })
        .collect::<Vec<_>>();
    let title = if app.settings_dirty {
        " settings * "
    } else {
        " settings "
    };
    let list = List::new(items).block(
        pane_block().borders(Borders::TOP).title(Span::styled(
            title,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )),
    );
    frame.render_widget(list, body[0]);

    let sidebar_style = if app.jami_details_active() {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    let help_widget = Paragraph::new(settings_snapshot_text(
        app,
        body[1].width.saturating_sub(4) as usize,
        body[1].height.saturating_sub(2) as usize,
    ))
    .block(
        sidebar_block().borders(Borders::TOP).title(Span::styled(
            " details ",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(sidebar_style)
    .wrap(Wrap { trim: false });
    frame.render_widget(help_widget, body[1]);
    if let Some(editor) = app.settings_text_editor.as_ref() {
        render_settings_text_editor(frame, editor, area);
    }
}

fn render_settings_update(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(50),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " ctox update ",
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "[c]",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" check  "),
        Span::styled(
            "[u]",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" upgrade  "),
        Span::styled(
            "[e]",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" engine rebuild  "),
        Span::styled(
            "[d]",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" doctor  "),
        Span::styled(
            "[r]",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" refresh"),
    ]));
    frame.render_widget(header, split[0]);

    let info = if app.update_view.info_json.is_empty() {
        "(press [r] to load status)".to_string()
    } else {
        update_install_summary_text(&app.update_view.info_json)
    };
    frame.render_widget(
        Paragraph::new(info)
            .block(
                pane_block().borders(Borders::TOP).title(Span::styled(
                    " install / version ",
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                )),
            )
            .wrap(Wrap { trim: false }),
        split[1],
    );

    let check_body = if app.update_view.check_json.is_empty() {
        "No update check or action has run in this TUI session.\n\nActions:\n  [c] Check for a newer CTOX release\n  [u] Upgrade CTOX and restart the service\n  [e] Rebuild the local model engine only\n  [d] Run doctor diagnostics\n  [r] Refresh installed version information\n\nRelease channel:\n  metric-space-ai/ctox\n\nFork override:\n  ctox update channel set-github --repo <owner/repo>"
            .to_string()
    } else {
        update_remote_summary_text(&app.update_view.check_json)
    };
    frame.render_widget(
        Paragraph::new(check_body)
            .block(
                pane_block().borders(Borders::TOP).title(Span::styled(
                    " remote check ",
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                )),
            )
            .wrap(Wrap { trim: false }),
        split[2],
    );

    let footer = if app.update_view.last_action_line.is_empty() {
        "ready".to_string()
    } else {
        app.update_view.last_action_line.clone()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(footer, Style::default().fg(Color::DarkGray))),
        split[3],
    );
}

fn render_settings_harness_mining(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    use crate::service::harness_mining;
    let snap = harness_mining::ui_snapshot(&app.root);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(err) = &snap.error {
        lines.push(Line::from(Span::styled(
            format!("status: unavailable — {err}"),
            Style::default().fg(Color::Red),
        )));
    } else if !snap.samples_known {
        lines.push(Line::from(Span::styled(
            "status: snapshot unavailable",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(harness_status_line(&snap));
        lines.push(Line::from(""));
        lines.push(harness_metric_line(
            "conformance (preventive)",
            &format!("{:.1}%", snap.preventive_fitness * 100.0),
            snap.preventive_fitness >= 0.95,
        ));
        lines.push(harness_metric_line(
            "conformance (trigger)",
            &format!("{:.1}%", snap.trigger_fitness * 100.0),
            snap.trigger_fitness >= 0.95,
        ));
        lines.push(harness_metric_line(
            "concept drift",
            if snap.drift_detected {
                "detected"
            } else {
                "stable"
            },
            !snap.drift_detected,
        ));
        lines.push(harness_metric_line(
            "stuck cases (≥5 retries)",
            &snap.stuck_case_count.to_string(),
            snap.stuck_case_count == 0,
        ));
        lines.push(harness_metric_line(
            "trace variants",
            &snap.variant_count.to_string(),
            true,
        ));
        if snap.dominant_variant_share > 0.0 {
            lines.push(harness_metric_line(
                "dominant variant share",
                &format!("{:.0}%", snap.dominant_variant_share * 100.0),
                snap.dominant_variant_share < 0.7,
            ));
        }
        if snap.worst_state_p95_seconds > 0.0 {
            lines.push(harness_metric_line(
                "worst state p95 dwell",
                &format_duration(snap.worst_state_p95_seconds),
                snap.worst_state_p95_seconds < 600.0,
            ));
        }
        if !snap.stuck_top_violation_codes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "top violation codes:",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )));
            for code in &snap.stuck_top_violation_codes {
                lines.push(Line::from(Span::styled(
                    format!("  • {code}"),
                    Style::default().fg(Color::White),
                )));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                pane_block().borders(Borders::TOP).title(Span::styled(
                    " harness health ",
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                )),
            )
            .wrap(Wrap { trim: false }),
        body[0],
    );

    let explainer = "Harness mining audits what the agent actually did,\n\
        independent of what it said. Trigger-level events,\n\
        preventive proofs and the declared state machine\n\
        are joined into a small set of health signals.\n\n\
        Green = conformant, no drift, no stuck cases.\n\
        Yellow/red = open the CLI for the full report:\n\n\
            ctox harness-mining stuck-cases\n\
            ctox harness-mining variants --cluster\n\
            ctox harness-mining sojourn\n\
            ctox harness-mining conformance\n\
            ctox harness-mining alignment\n\
            ctox harness-mining causal\n\
            ctox harness-mining drift\n\
            ctox harness-mining multiperspective\n\n\
        No bodies, hashes, or recipients are shown here.\n\
        The CLI returns aggregates only by default.";

    frame.render_widget(
        Paragraph::new(explainer)
            .block(
                sidebar_block().borders(Borders::TOP).title(Span::styled(
                    " what this is ",
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                )),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false }),
        body[1],
    );
}

fn harness_status_line(snap: &crate::service::harness_mining::UiSnapshot) -> Line<'static> {
    let healthy = snap.conformance_ok && !snap.drift_detected && snap.stuck_case_count == 0;
    if healthy {
        Line::from(Span::styled(
            "status: healthy",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
    } else if !snap.conformance_ok || snap.stuck_case_count > 0 {
        Line::from(Span::styled(
            "status: attention required",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(
            "status: drift detected",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    }
}

fn harness_metric_line(label: &str, value: &str, ok: bool) -> Line<'static> {
    let dot_color = if ok { Color::Green } else { Color::Red };
    Line::from(vec![
        Span::styled("● ", Style::default().fg(dot_color)),
        Span::styled(format!("{:<28}", label), Style::default().fg(Color::Gray)),
        Span::styled(
            value.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.0}s", seconds)
    } else if seconds < 3600.0 {
        format!("{:.0}m", seconds / 60.0)
    } else if seconds < 86400.0 {
        format!("{:.1}h", seconds / 3600.0)
    } else {
        format!("{:.1}d", seconds / 86400.0)
    }
}

fn update_install_summary_text(raw: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    let version = json_str(&value, "version").unwrap_or("unknown");
    let install_mode = json_str(&value, "install_mode").unwrap_or("unknown");
    let workspace_root = json_str(&value, "workspace_root").unwrap_or("—");
    let active_root = json_str(&value, "active_root").unwrap_or("—");
    let state_root = json_str(&value, "state_root").unwrap_or("—");
    let cache_root = json_str(&value, "cache_root").unwrap_or("—");
    let current_release = json_str(&value, "current_release").unwrap_or("—");
    let previous_release = json_str(&value, "previous_release").unwrap_or("—");
    let release_kind = value
        .get("release_channel")
        .and_then(|channel| channel.get("kind"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("—");
    let release_repo = value
        .get("release_channel")
        .and_then(|channel| channel.get("repo"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("—");

    [
        format!("version         {version}"),
        format!("install mode    {install_mode}"),
        format!("current release {current_release}"),
        format!("previous        {previous_release}"),
        String::new(),
        format!("channel         {release_kind}"),
        format!("repo            {release_repo}"),
        String::new(),
        format!("workspace root  {workspace_root}"),
        format!("active root     {active_root}"),
        format!("state root      {state_root}"),
        format!("cache root      {cache_root}"),
    ]
    .join("\n")
}

fn update_remote_summary_text(raw: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    let action = json_str(&value, "action").unwrap_or("remote check");
    let update_available = value
        .get("update_available")
        .and_then(serde_json::Value::as_bool);
    let current_version = json_str(&value, "current_version")
        .or_else(|| json_str(&value, "version"))
        .unwrap_or("unknown");
    let latest_version = json_str(&value, "latest_version")
        .or_else(|| json_str(&value, "remote_version"))
        .unwrap_or("unknown");
    let reason = json_str(&value, "reason").unwrap_or("—");

    let status = match update_available {
        Some(true) => "update available",
        Some(false) => "up to date",
        None => "status unknown",
    };

    let mut lines = vec![
        format!("action          {action}"),
        format!("status          {status}"),
        format!("current         {current_version}"),
        format!("latest          {latest_version}"),
        format!("reason          {reason}"),
    ];
    if let Some(output) = json_str(&value, "output").filter(|text| !text.trim().is_empty()) {
        lines.push(String::new());
        lines.push("output".to_string());
        lines.push(output.to_string());
    }
    lines.join("\n")
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn render_skills_narrow(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(38), Constraint::Min(10)])
        .split(area);
    frame.render_widget(
        List::new(skill_list_items(
            app,
            layout[0].width.saturating_sub(4) as usize,
            layout[0].height.saturating_sub(2) as usize,
        ))
        .block(
            pane_block().borders(Borders::TOP).title(Span::styled(
                " skills ",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(skill_details_lines(
            app,
            layout[1].width.saturating_sub(4) as usize,
            layout[1].height.saturating_sub(2) as usize,
        ))
        .block(
            sidebar_block().borders(Borders::TOP).title(Span::styled(
                " details ",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false }),
        layout[1],
    );
}

fn render_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let _ = (frame, app, area);
}

fn render_chat_narrow(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Percentage(30),
            Constraint::Percentage(24),
            Constraint::Min(5),
        ])
        .split(area);

    let turn_widget = Paragraph::new(turn_summary_lines(
        app,
        layout[0].width.saturating_sub(4) as usize,
        layout[0].height.saturating_sub(2) as usize,
    ))
    .block(
        pane_block().borders(Borders::TOP).title(Span::styled(
            " turn ",
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: false });
    frame.render_widget(turn_widget, layout[0]);

    let transcript_widget = Paragraph::new(render_transcript_lines(
        app,
        layout[1].width.saturating_sub(4) as usize,
        layout[1].height.saturating_sub(2) as usize,
    ))
    .block(
        pane_block().borders(Borders::TOP).title(Span::styled(
            " chat ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: false });
    frame.render_widget(transcript_widget, layout[1]);

    let feed = Paragraph::new(
        activity_lines(app, layout[2].width.saturating_sub(4) as usize, 2, true).join("\n"),
    )
    .block(
        sidebar_block().borders(Borders::TOP).title(Span::styled(
            " live ",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )),
    )
    .style(Style::default().fg(Color::Gray))
    .wrap(Wrap { trim: false });
    frame.render_widget(feed, layout[2]);

    let composer_text = if app.chat_input.trim().is_empty() {
        if app.request_in_flight {
            "Type while CTOX is busy. Enter queues."
        } else {
            "Type your next instruction."
        }
    } else {
        app.chat_input.as_str()
    };
    let narrow_composer_title_base = if app.request_in_flight {
        " queued draft "
    } else {
        " compose "
    };
    let narrow_composer_title = if app.pending_images.is_empty() {
        narrow_composer_title_base.to_string()
    } else {
        format!(
            " {} 📎 {} ",
            narrow_composer_title_base.trim(),
            app.pending_images.len()
        )
    };
    let composer = Paragraph::new(composer_text)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(Span::styled(
                    narrow_composer_title,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ))
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default().bg(Color::Rgb(20, 20, 20))),
        )
        .style(if app.chat_input.trim().is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .wrap(Wrap { trim: false });
    frame.render_widget(composer, layout[3]);
}

fn render_settings_narrow(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8)])
        .split(area);
    render_settings_view_tabs(frame, app, outer[0]);
    if app.settings_view == SettingsView::Update {
        render_settings_update(frame, app, outer[1]);
        return;
    }
    if app.settings_view == SettingsView::HarnessMining {
        render_settings_harness_mining(frame, app, outer[1]);
        return;
    }
    if app.settings_view == SettingsView::Secrets {
        render_secrets_narrow(frame, app, outer[1]);
        return;
    }
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(8)])
        .split(outer[1]);
    let visible_indices = app.visible_setting_indices();
    let max_rows = layout[0].height.saturating_sub(2) as usize;
    let window_indices = settings_window_indices(app, &visible_indices, max_rows);
    let items = window_indices
        .into_iter()
        .filter_map(|idx| app.settings_items.get(idx).map(|item| (idx, item)))
        .map(|(index, item)| {
            let rendered_value = app.rendered_setting_value(item);
            let base = format!("{:16} {}", item.label, truncate_line(&rendered_value, 32));
            let row_style = setting_row_style(item.key, item.value.trim());
            if index == app.settings_selected {
                ListItem::new(base).style(
                    row_style
                        .bg(Color::LightCyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(base).style(row_style)
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            pane_block().borders(Borders::TOP).title(Span::styled(
                " settings ",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        layout[0],
    );
    let sidebar_style = if app.jami_details_active() {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    frame.render_widget(
        Paragraph::new(settings_snapshot_text(
            app,
            area.width.saturating_sub(6) as usize,
            layout[1].height.saturating_sub(2) as usize,
        ))
        .block(
            sidebar_block().borders(Borders::TOP).title(Span::styled(
                " details ",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .style(sidebar_style)
        .wrap(Wrap { trim: false }),
        layout[1],
    );
    if let Some(editor) = app.settings_text_editor.as_ref() {
        render_settings_text_editor(frame, editor, area);
    }
}

fn render_settings_text_editor(
    frame: &mut Frame,
    editor_state: &super::SettingsTextEditorState,
    area: ratatui::layout::Rect,
) {
    let popup = centered_rect(area, 88, 78);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::LightCyan))
        .title(Span::styled(
            format!(" {} ", editor_state.label),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Ctrl-X save · Esc cancel ",
            Style::default().fg(Color::Gray),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    editor_state.editor.render(frame, inner);
}

fn centered_rect(
    area: ratatui::layout::Rect,
    width_pct: u16,
    height_pct: u16,
) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage(100 - height_pct - (100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage(100 - width_pct - (100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_secrets(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);
    frame.render_widget(
        List::new(secret_list_items(
            app,
            body[0].height.saturating_sub(2) as usize,
        ))
        .block(
            pane_block().borders(Borders::TOP).title(Span::styled(
                format!(" secrets ({}) ", app.secret_items.len()),
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        body[0],
    );
    frame.render_widget(
        Paragraph::new(secret_details_text(
            app,
            body[1].width.saturating_sub(4) as usize,
            body[1].height.saturating_sub(2) as usize,
        ))
        .block(
            sidebar_block().borders(Borders::TOP).title(Span::styled(
                " secret details ",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false }),
        body[1],
    );
}

fn render_secrets_narrow(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(12)])
        .split(area);
    frame.render_widget(
        List::new(secret_list_items(
            app,
            layout[0].height.saturating_sub(2) as usize,
        ))
        .block(
            pane_block().borders(Borders::TOP).title(Span::styled(
                format!(" secrets ({}) ", app.secret_items.len()),
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(secret_details_text(
            app,
            area.width.saturating_sub(6) as usize,
            layout[1].height.saturating_sub(2) as usize,
        ))
        .block(
            sidebar_block().borders(Borders::TOP).title(Span::styled(
                " secret details ",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false }),
        layout[1],
    );
}

fn render_transcript_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for message in app
        .chat_messages
        .iter()
        .rev()
        .take(18)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let (label, role_label, badge_color, body_color) =
            if message.role.eq_ignore_ascii_case("assistant") {
                (" CTOX ", "assistant", Color::LightGreen, Color::White)
            } else {
                (
                    " YOU ",
                    message.role.as_str(),
                    Color::LightCyan,
                    Color::Gray,
                )
            };
        lines.push(truncate_line_spans(
            vec![
                Span::styled(
                    label.to_string(),
                    Style::default()
                        .fg(Color::Black)
                        .bg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    truncate_line(role_label, width.saturating_sub(8)),
                    Style::default().fg(Color::DarkGray),
                ),
            ],
            width,
        ));
        let rendered_body = wrap_text_lines(&message.content, width.saturating_sub(2));
        for chunk in rendered_body {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(chunk, Style::default().fg(body_color)),
            ]));
        }
        lines.push(Line::from(String::new()));
    }
    if app.request_in_flight {
        lines.push(truncate_line_spans(
            vec![
                Span::styled(
                    " WORKING ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    app.service_status
                        .active_source_label
                        .clone()
                        .unwrap_or_else(|| "turn".to_string()),
                    Style::default().fg(Color::Yellow),
                ),
            ],
            width,
        ));
    }
    if lines.len() > height {
        lines = lines.split_off(lines.len().saturating_sub(height));
    }
    lines
}

fn settings_window_indices(app: &App, visible_indices: &[usize], max_rows: usize) -> Vec<usize> {
    if visible_indices.len() <= max_rows.max(1) {
        return visible_indices.to_vec();
    }
    let selected_pos = visible_indices
        .iter()
        .position(|idx| *idx == app.settings_selected)
        .unwrap_or(0);
    let window = max_rows.max(1);
    let max_start = visible_indices.len().saturating_sub(window);
    let start = selected_pos.min(max_start);
    visible_indices[start..start + window].to_vec()
}

fn skill_list_items(app: &App, width: usize, max_rows: usize) -> Vec<ListItem<'static>> {
    if app.skill_catalog.is_empty() {
        return vec![
            ListItem::new("No skills discovered yet.").style(Style::default().fg(Color::DarkGray))
        ];
    }
    let window = skill_window_indices(app, max_rows.max(1));
    window
        .into_iter()
        .filter_map(|index| app.skill_catalog.get(index).map(|entry| (index, entry)))
        .map(|(index, entry)| {
            // For clustered system skills the second column is the cluster name
            // (more informative than "CTOX Core" repeated 39 times). For
            // non-system entries we fall back to the class label.
            let secondary = if entry.cluster.is_empty() {
                entry.class.label().to_string()
            } else {
                entry.cluster.clone()
            };
            let row = format!(
                "{:18} {}",
                truncate_line(&entry.name, 18),
                truncate_line(&secondary, width.saturating_sub(20))
            );
            let base_style = Style::default().fg(skill_class_color(entry.class.label()));
            if index == app.skills_selected {
                ListItem::new(row).style(
                    base_style
                        .bg(Color::LightYellow)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(row).style(base_style)
            }
        })
        .collect()
}

fn skill_window_indices(app: &App, max_rows: usize) -> Vec<usize> {
    let total = app.skill_catalog.len();
    if total <= max_rows.max(1) {
        return (0..total).collect();
    }
    let window = max_rows.max(1);
    let selected = app.skills_selected.min(total.saturating_sub(1));
    let half = window / 2;
    let mut start = selected.saturating_sub(half);
    let max_start = total.saturating_sub(window);
    if start > max_start {
        start = max_start;
    }
    (start..start + window).collect()
}

fn skill_details_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let Some(entry) = app.skill_catalog.get(app.skills_selected) else {
        return vec![Line::from("No skills found.")];
    };
    let mut header_spans = vec![
        Span::styled(
            entry.name.clone(),
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            entry.class.label(),
            Style::default().fg(skill_class_color(entry.class.label())),
        ),
    ];
    if !entry.cluster.is_empty() {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(
            format!("[{}]", entry.cluster),
            Style::default().fg(Color::LightCyan),
        ));
    }
    lines.push(Line::from(header_spans));
    lines.push(section_title("path", Color::LightBlue, width));
    for chunk in wrap_text_lines(&entry.skill_path.to_string_lossy(), width) {
        lines.push(Line::from(chunk));
    }
    lines.push(Line::from(format!("state {}", entry.state.label())));
    lines.push(Line::from(String::new()));
    lines.push(section_title("summary", Color::LightGreen, width));
    for chunk in wrap_text_lines(&entry.description, width) {
        lines.push(Line::from(chunk));
    }
    lines.push(Line::from(String::new()));
    lines.push(section_title("helper tools", Color::LightMagenta, width));
    if entry.helper_tools.is_empty() {
        lines.push(Line::from("No scripts/ helper tools detected."));
    } else {
        for tool in entry.helper_tools.iter().take(10) {
            lines.push(Line::from(format!(
                "• {}",
                truncate_line(tool, width.saturating_sub(2))
            )));
        }
    }
    lines.push(Line::from(String::new()));
    lines.push(section_title("resources", Color::LightBlue, width));
    if entry.resources.is_empty() {
        lines.push(Line::from("No extra resources detected."));
    } else {
        for resource in entry.resources.iter().take(10) {
            for chunk in wrap_text_lines(resource, width.saturating_sub(2)) {
                lines.push(Line::from(format!("• {chunk}")));
            }
        }
    }
    lines.push(Line::from(String::new()));
    lines.push(Line::from(
        "Up/Down select  E edit in $EDITOR/nano  R reload  Tab next page",
    ));
    if lines.len() > height {
        lines.truncate(height);
    }
    lines
}

fn skill_class_color(label: &str) -> Color {
    match label {
        "CTOX Core" => Color::LightCyan,
        "Codex Core" => Color::Cyan,
        "Installed Packs" => Color::White,
        "Personal" => Color::LightGreen,
        _ => Color::White,
    }
}

fn activity_lines(
    app: &App,
    width: usize,
    channel_limit: usize,
    include_queue: bool,
) -> Vec<String> {
    let visible_activity = app
        .activity_log
        .iter()
        .filter(|line| service_event_visible_in_tui(line))
        .rev()
        .take(4)
        .collect::<Vec<_>>();
    let mut lines = if visible_activity.is_empty() {
        vec!["• Waiting for the next CTOX event.".to_string()]
    } else {
        visible_activity
            .into_iter()
            .rev()
            .map(|line| format!("• {}", truncate_line(line, width)))
            .collect::<Vec<_>>()
    };
    if include_queue {
        if app.draft_queue.is_empty() {
            if !app.service_status.pending_previews.is_empty() {
                lines.push(format!(
                    "queue  {} waiting",
                    app.service_status.pending_count
                ));
                for preview in app.service_status.pending_previews.iter().take(3) {
                    lines.push(format!("• {}", truncate_line(preview, width)));
                }
            } else if app.service_status.pending_count > 0 {
                lines.push(format!(
                    "queue  {} waiting",
                    app.service_status.pending_count
                ));
            } else {
                lines.push("queue  clear".to_string());
            }
        } else {
            lines.push(format!("queue  {} local", app.draft_queue.len()));
            for draft in app.draft_queue.iter().take(2) {
                lines.push(format!("• {}", truncate_line(draft, width)));
            }
        }
    }
    lines.push("inbox".to_string());
    if app.communication_feed.is_empty() {
        lines.push("• No channel traffic yet.".to_string());
    } else {
        for item in app.communication_feed.iter().take(channel_limit) {
            let direction = if item.direction == "inbound" {
                "in"
            } else {
                "out"
            };
            let source = if !item.sender_display.trim().is_empty() {
                item.sender_display.trim()
            } else {
                item.sender_address.trim()
            };
            let text = if item.preview.trim().is_empty() {
                item.subject.as_str()
            } else {
                item.preview.as_str()
            };
            lines.push(format!(
                "• {} {} {}",
                item.channel,
                direction,
                truncate_line(
                    &format!("{source}: {text}"),
                    width.saturating_sub(item.channel.len() + 4)
                )
            ));
        }
    }
    lines
}

fn pane_block() -> Block<'static> {
    Block::default()
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Rgb(10, 10, 10)))
}

fn sidebar_block() -> Block<'static> {
    Block::default()
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Rgb(18, 18, 18)))
}

#[allow(dead_code)]
fn sidebar_fill() -> Block<'static> {
    Block::default().style(Style::default().bg(Color::Rgb(18, 18, 18)))
}

fn sidebar_footer_lines(app: &App) -> Vec<Line<'static>> {
    let spinner = spinner_frame(app.spinner_phase);
    let status = if app.request_in_flight {
        "working"
    } else if runtime_health_is_degraded_for_display(app) {
        "degraded"
    } else if app.service_status.running {
        "ready"
    } else {
        "stopped"
    };
    vec![
        Line::from(vec![
            Span::styled(format!("{spinner} "), Style::default().fg(Color::LightBlue)),
            Span::styled(status, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::White)),
            Span::raw(" interrupt  "),
            Span::styled("Tab", Style::default().fg(Color::White)),
            Span::raw(" next page"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl-C", Style::default().fg(Color::White)),
            Span::raw(" quit"),
        ]),
    ]
}

fn chat_sidebar_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let footer = sidebar_footer_lines(app);
    let footer_reserved = footer.len();
    let active_reserved = if app.service_status.busy {
        6usize
    } else {
        4usize
    };
    let mission_reserved = if app.mission_state.is_some() {
        6usize
    } else {
        4usize
    };
    let queue_reserved = 4usize;
    let live_budget = height
        .saturating_sub(footer_reserved + active_reserved + mission_reserved + queue_reserved)
        .max(4);
    lines.push(section_title("live", Color::LightBlue, width));
    for line in activity_lines(app, width, 2, false)
        .into_iter()
        .take(live_budget.saturating_sub(1))
    {
        lines.push(Line::from(truncate_line(&line, width)));
    }
    lines.push(Line::from(String::new()));

    lines.push(section_title("active", Color::Yellow, width));
    for line in active_lines(app, width)
        .into_iter()
        .take(active_reserved.saturating_sub(1))
    {
        lines.push(Line::from(truncate_line(&line, width)));
    }
    lines.push(Line::from(String::new()));

    lines.push(section_title("missions", Color::LightMagenta, width));
    for line in mission_lines(app, width)
        .into_iter()
        .take(mission_reserved.saturating_sub(1))
    {
        lines.push(Line::from(truncate_line(&line, width)));
    }
    lines.push(Line::from(String::new()));

    let queue_title = if app.draft_queue.is_empty() {
        "queue".to_string()
    } else {
        format!("queue {}", app.draft_queue.len())
    };
    lines.push(section_title(&queue_title, Color::LightYellow, width));
    for line in queue_lines(app, width)
        .into_iter()
        .take(queue_reserved.saturating_sub(1))
    {
        lines.push(Line::from(truncate_line(&line, width)));
    }

    let content_budget = height.saturating_sub(footer_reserved);
    if lines.len() > content_budget {
        lines.truncate(content_budget);
    }
    while lines.len() < content_budget {
        lines.push(Line::from(String::new()));
    }
    lines.extend(footer);
    lines.into_iter().take(height.max(1)).collect()
}

fn active_lines(app: &App, width: usize) -> Vec<String> {
    if !app.service_status.running {
        return vec!["Service offline.".to_string()];
    }
    if runtime_health_is_degraded_for_display(app) && !app.service_status.busy {
        return vec![format!(
            "Degraded: {} down.",
            degraded_components_for_display(app).join("+")
        )];
    }
    if !app.service_status.busy {
        return vec!["Idle.".to_string()];
    }
    let source = app
        .service_status
        .active_source_label
        .clone()
        .unwrap_or_else(|| "turn".to_string());
    let mut lines = vec![format!(
        "{} {} active",
        spinner_frame(app.spinner_phase),
        source
    )];
    if let Some(event) = app
        .service_status
        .recent_events
        .iter()
        .rev()
        .find(|event| service_event_visible_in_tui(event))
    {
        lines.push(truncate_line(event, width));
    }
    if let Some(goal) = app.service_status.current_goal_preview.as_deref() {
        lines.push(format!(
            "goal {}",
            truncate_line(goal, width.saturating_sub(5))
        ));
    }
    if app.service_status.pending_count > 0 {
        lines.push(format!(
            "after this {} queued",
            app.service_status.pending_count
        ));
    }
    lines
}

fn queue_lines(app: &App, width: usize) -> Vec<String> {
    if app.draft_queue.is_empty() {
        if !app.service_status.pending_previews.is_empty() {
            let mut lines = vec![format!("{} waiting", app.service_status.pending_count)];
            for preview in app.service_status.pending_previews.iter().take(3) {
                lines.push(format!(
                    "• {}",
                    truncate_line(preview, width.saturating_sub(2))
                ));
            }
            return lines;
        }
        if app.service_status.pending_count > 0 {
            return vec![format!(
                "{} server-side prompt(s) waiting.",
                app.service_status.pending_count
            )];
        }
        return vec!["No queued drafts.".to_string()];
    }

    app.draft_queue
        .iter()
        .enumerate()
        .map(|(idx, draft)| {
            format!(
                "{}. {}",
                idx + 1,
                truncate_line(draft, width.saturating_sub(3))
            )
        })
        .collect()
}

fn mission_lines(app: &App, width: usize) -> Vec<String> {
    let Some(mission) = app.mission_state.as_ref() else {
        return vec!["No active mission yet.".to_string()];
    };
    let mission_name = if mission.mission.trim().is_empty() {
        "Mission not classified yet.".to_string()
    } else {
        truncate_line(mission.mission.trim(), width)
    };
    let mut lines = vec![mission_name];
    let state = fallback_label(&mission.mission_status, "unknown");
    let mode = fallback_label(&mission.continuation_mode, "continuous");
    lines.push(format!("{} • {}", state, mode));
    if mission.is_open {
        if let Some(next_slice) = non_empty_mission_value(&mission.next_slice) {
            lines.push(format!(
                "next {}",
                truncate_line(next_slice, width.saturating_sub(5))
            ));
        }
        if let Some(blocker) = non_empty_mission_value(&mission.blocker) {
            lines.push(format!(
                "blocker {}",
                truncate_line(blocker, width.saturating_sub(8))
            ));
        } else {
            lines.push("blocker none".to_string());
        }
    } else {
        lines.push("closed".to_string());
    }
    lines
}

fn spinner_frame(phase: usize) -> &'static str {
    let frames = ["⠁", "⠂", "⠄", "⠂"];
    frames[phase % frames.len()]
}

fn section_title(title: &str, color: Color, width: usize) -> Line<'static> {
    let label = format!(" {title} ");
    let dash_count = width.saturating_sub(label.chars().count()).max(1);
    let mut spans = vec![Span::styled(
        label,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::styled(
        "─".repeat(dash_count),
        Style::default().fg(Color::DarkGray),
    ));
    truncate_line_spans(spans, width)
}

fn turn_summary_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let (status_text, status_color) = if !app.service_status.running {
        ("loop stopped", Color::Red)
    } else if runtime_health_is_degraded_for_display(app) {
        ("loop degraded", Color::Yellow)
    } else if app.service_status.busy {
        ("loop working", Color::Yellow)
    } else {
        ("loop ready", Color::Green)
    };
    lines.push(truncate_line_spans(
        vec![
            Span::raw("state "),
            Span::styled(
                status_text.to_string(),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                app.service_status
                    .active_source_label
                    .clone()
                    .unwrap_or_else(|| "idle".to_string()),
                Style::default().fg(Color::LightCyan),
            ),
        ],
        width,
    ));
    lines.push(Line::from(format!(
        "queue {} pending  drafts {}",
        app.service_status.pending_count,
        app.draft_queue.len()
    )));
    if let Some(goal) = app.service_status.current_goal_preview.as_deref() {
        lines.push(Line::from(format!(
            "goal {}",
            truncate_line(goal, width.saturating_sub(5))
        )));
    }
    if let Some(error) = app.service_status.last_error.as_deref() {
        lines.push(Line::from(format!(
            "error {}",
            truncate_line(error, width.saturating_sub(6))
        )));
    } else if let Some(chars) = app.service_status.last_reply_chars {
        lines.push(Line::from(format!("last reply {chars} chars")));
    } else {
        lines.push(Line::from("last reply -"));
    }
    if let Some(completed) = app.service_status.last_completed_at.as_deref() {
        lines.push(Line::from(format!(
            "completed {}",
            truncate_line(completed, width.saturating_sub(10))
        )));
    }
    let visible_events = app
        .service_status
        .recent_events
        .iter()
        .filter(|event| service_event_visible_in_tui(event))
        .rev()
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    for event in visible_events.into_iter().rev() {
        lines.push(Line::from(format!(
            "• {}",
            truncate_line(&event, width.saturating_sub(2))
        )));
    }
    lines.truncate(height);
    lines
}

fn service_event_visible_in_tui(event: &str) -> bool {
    !(event.starts_with("phase ")
        || event.starts_with("Completion review ")
        || event.starts_with("Context health ")
        || event.contains("refresh-budget"))
}

fn wrap_text_lines(value: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for paragraph in value.lines() {
        let words = paragraph.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in words {
            let proposed_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };
            if proposed_len > max_chars && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else if current.is_empty() {
                current = word.to_string();
            } else {
                current.push(' ');
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[allow(dead_code)]
fn value_for_key(app: &App, key: &str) -> String {
    app.settings_items
        .iter()
        .find(|item| item.key == key)
        .map(|item| {
            let value = item.value.trim();
            if value.is_empty() {
                "-".to_string()
            } else {
                truncate_line(value, 30)
            }
        })
        .unwrap_or_else(|| "-".to_string())
}

fn settings_snapshot_text(app: &App, width: usize, height: usize) -> String {
    let Some(item) = app.current_setting() else {
        return String::new();
    };
    if !app
        .visible_setting_indices()
        .contains(&app.settings_selected)
    {
        return "No settings are available in this view.".to_string();
    }
    let mut lines = vec![
        format!(
            "item     {}",
            truncate_line(item.label, width.saturating_sub(9))
        ),
        format!(
            "mode     {}",
            if app.header.estimate_mode {
                "estimate"
            } else {
                "live"
            }
        ),
    ];
    lines.push(String::new());
    push_wrapped_lines(&mut lines, &item.help, width);
    if item.kind == super::SettingKind::Env {
        lines.push(String::new());
        lines.push(format!(
            "saved    {}",
            display_setting_value(item.saved_value.trim(), item.secret)
        ));
        lines.push(format!(
            "{} {}",
            if app.setting_is_dirty(item) {
                "draft   "
            } else {
                "active   "
            },
            display_setting_value(item.value.trim(), item.secret)
        ));
    }
    match item.key {
        "CTOX_API_PROVIDER" | "OPENAI_API_KEY" | "OPENROUTER_API_KEY" => {
            append_provider_details(&mut lines, app, width);
        }
        "CTOX_CHAT_MODEL" | "CTOX_CHAT_LOCAL_PRESET" | "CTOX_CHAT_SKILL_PRESET" => {
            append_chat_runtime_details(&mut lines, app, item.key, width);
        }
        "CTOX_EMBEDDING_MODEL" | "CTOX_STT_MODEL" | "CTOX_TTS_MODEL" => {
            append_aux_model_details(&mut lines, app, item.key, width);
        }
        _ => {
            append_general_setting_details(&mut lines, app, width);
        }
    }
    lines
        .into_iter()
        .take(height.max(1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn secret_list_items(app: &App, max_rows: usize) -> Vec<ListItem<'static>> {
    let count = app.secret_items.len();
    if count == 0 {
        return vec![ListItem::new("No encrypted secrets stored yet.")
            .style(Style::default().fg(Color::DarkGray))];
    }
    let rows = max_rows.max(1);
    let start = app.secrets_selected.saturating_sub(rows.saturating_sub(1));
    let end = (start + rows).min(count);
    app.secret_items[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            let label = format!(
                "{:14} {:18} {}",
                truncate_line(&item.scope, 14),
                truncate_line(&item.name, 18),
                truncate_line(&mask_secret(&item.saved_value), 24)
            );
            if index == app.secrets_selected {
                ListItem::new(label).style(
                    Style::default()
                        .bg(Color::LightCyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(label).style(Style::default().fg(Color::White))
            }
        })
        .collect()
}

fn secret_details_text(app: &App, width: usize, height: usize) -> String {
    let Some(item) = app.current_secret() else {
        return "No encrypted secret selected.\n\nUse this tab to inspect and update values stored in the SQLite secret store.\n\nHotkeys:\n  ↑ ↓ select\n  type edits the value\n  Enter saves\n  r reloads"
            .to_string();
    };
    let mut lines = vec![
        format!(
            "scope    {}",
            truncate_line(&item.scope, width.saturating_sub(9))
        ),
        format!(
            "name     {}",
            truncate_line(&item.name, width.saturating_sub(9))
        ),
        format!(
            "status   {}",
            if app.secret_is_dirty(item) {
                "draft differs from stored value"
            } else {
                "stored value loaded"
            }
        ),
        String::new(),
    ];
    if let Some(description) = item.description.as_deref() {
        lines.push("description".to_string());
        push_wrapped_lines(&mut lines, description, width);
        lines.push(String::new());
    }
    lines.push("stored value (masked)".to_string());
    push_wrapped_lines(&mut lines, &mask_secret(&item.saved_value), width);
    lines.push(String::new());
    lines.push("draft value (masked)".to_string());
    push_wrapped_lines(&mut lines, &mask_secret(&item.value), width);
    if !item.metadata.is_null() && item.metadata != serde_json::json!({}) {
        lines.push(String::new());
        lines.push("metadata".to_string());
        push_wrapped_lines(
            &mut lines,
            &serde_json::to_string_pretty(&item.metadata).unwrap_or_default(),
            width,
        );
    }
    lines.push(String::new());
    lines.push(format!(
        "updated  {}",
        truncate_line(&item.updated_at, width.saturating_sub(9))
    ));
    lines.push(format!(
        "created  {}",
        truncate_line(&item.created_at, width.saturating_sub(9))
    ));
    lines.push(String::new());
    lines.push("hotkeys  ↑ ↓ select · type edit · Enter save · Ctrl-S save · r reload".to_string());
    lines
        .into_iter()
        .take(height.max(1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn degraded_components_for_display(app: &App) -> Vec<&'static str> {
    let mut parts = app.runtime_health.degraded_components();
    if chat_source_is_api(app) {
        parts.retain(|component| *component != "runtime");
    }
    parts
}

fn runtime_health_is_degraded_for_display(app: &App) -> bool {
    !degraded_components_for_display(app).is_empty()
}

fn push_wrapped_lines(lines: &mut Vec<String>, text: &str, width: usize) {
    for chunk in wrap_text_lines(text, width) {
        lines.push(chunk);
    }
}

fn append_provider_details(lines: &mut Vec<String>, app: &App, width: usize) {
    lines.push(String::new());
    lines.push("model pool".to_string());
    let local_pool = engine::SUPPORTED_CHAT_MODELS
        .iter()
        .filter(|model| engine::supports_local_chat_runtime(model))
        .count();
    let api_pool = engine::SUPPORTED_OPENAI_API_CHAT_MODELS.len();
    let openai_token_present = app
        .settings_items
        .iter()
        .find(|item| item.key == "OPENAI_API_KEY")
        .map(|item| !item.value.trim().is_empty())
        .unwrap_or(false);
    let openrouter_token_present = app
        .settings_items
        .iter()
        .find(|item| item.key == "OPENROUTER_API_KEY")
        .map(|item| !item.value.trim().is_empty())
        .unwrap_or(false);
    lines.push(format!("local    {local_pool} models"));
    lines.push(format!(
        "openai   {}",
        if openai_token_present {
            format!("{api_pool} models unlocked")
        } else {
            format!("{api_pool} models locked until token is saved")
        }
    ));
    lines.push(format!(
        "openrouter {}",
        if openrouter_token_present {
            format!(
                "{} models unlocked",
                engine::SUPPORTED_OPENROUTER_API_CHAT_MODELS.len()
            )
        } else {
            format!(
                "{} models locked until token is saved",
                engine::SUPPORTED_OPENROUTER_API_CHAT_MODELS.len()
            )
        }
    ));
    lines.push(format!(
        "merge    {}",
        truncate_line(
            "base + boost use the merged local/API pool once the selected provider is unlocked",
            width.saturating_sub(9)
        )
    ));
}

fn append_chat_runtime_details(lines: &mut Vec<String>, app: &App, item_key: &str, width: usize) {
    lines.push(String::new());
    lines.push("context".to_string());
    lines.push(format!("used     {}k", app.header.current_tokens / 1024));
    lines.push(format!(
        "compact  {}k @{}% >= {}k",
        app.header.compact_at / 1024,
        app.header.compact_percent,
        app.header.compact_min_tokens / 1024
    ));
    lines.push(format!(
        "plan     {}k   live {}k   ref 256k",
        app.header.configured_context / 1024,
        app.header.max_context / 1024
    ));

    let selected_model = app
        .settings_items
        .iter()
        .find(|item| item.key == item_key)
        .map(|item| item.value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            app.settings_items
                .iter()
                .find(|item| item.key == "CTOX_CHAT_MODEL")
                .map(|item| item.value.trim())
                .unwrap_or("")
        });

    if !selected_model.is_empty() {
        lines.push(String::new());
        lines.push("model".to_string());
        lines.push(format!(
            "name     {}",
            truncate_line(selected_model, width.saturating_sub(9))
        ));
        lines.push(format!(
            "kind     {}",
            if app
                .settings_items
                .iter()
                .find(|item| item.key == "CTOX_CHAT_SOURCE")
                .map(|item| item.value.trim().eq_ignore_ascii_case("api"))
                .unwrap_or(false)
                || (app
                    .settings_items
                    .iter()
                    .find(|item| item.key == "CTOX_API_PROVIDER")
                    .map(|item| item.value.trim().to_string())
                    .filter(|provider| !provider.eq_ignore_ascii_case("local"))
                    .is_some_and(|provider| {
                        engine::api_provider_supports_model(&provider, selected_model)
                    }))
            {
                "api"
            } else {
                "local"
            }
        ));
        if let Ok(profile) = engine::model_profile_for_model(selected_model) {
            lines.push(format!("family   {:?}", profile.runtime.family));
            lines.push(format!(
                "max ctx  {}k",
                (profile.family_profile.max_seq_len as usize) / 1024
            ));
            lines.push(format!(
                "default  {} / {}",
                profile.family_profile.isq.as_deref().unwrap_or("native"),
                profile
                    .family_profile
                    .pa_cache_type
                    .as_deref()
                    .unwrap_or("kv off")
            ));
        }
    }

    if let Some(bundle) = &app.chat_preset_bundle {
        lines.push(String::new());
        lines.push("runtime".to_string());
        lines.push(format!("preset   {}", bundle.selected_plan.preset.label()));
        lines.push(format!(
            "weights  {}",
            truncate_line(&bundle.selected_plan.quantization, width.saturating_sub(9))
        ));
        lines.push(format!(
            "runtime  {}",
            truncate_line(
                bundle
                    .selected_plan
                    .runtime_isq
                    .as_deref()
                    .unwrap_or("native"),
                width.saturating_sub(9)
            )
        ));
        lines.push(format!(
            "kv cache {}",
            truncate_line(
                bundle.selected_plan.effective_cache_label(),
                width.saturating_sub(9)
            )
        ));
        lines.push(format!(
            "paged    {}",
            truncate_line(&bundle.selected_plan.paged_attn, width.saturating_sub(9))
        ));
        lines.push(format!(
            "batch    {}   seqs {}",
            bundle.selected_plan.max_batch_size, bundle.selected_plan.max_seqs
        ));
        lines.push(format!(
            "backend  {}",
            if bundle.selected_plan.disable_nccl {
                "device-layers"
            } else {
                "nccl"
            }
        ));
        {
            let mut feature_tags = Vec::new();
            if !bundle.selected_plan.disable_flash_attn {
                feature_tags.push("flash-attn");
            }
            if !bundle.selected_plan.disable_nccl {
                feature_tags.push("nccl");
            }
            if bundle.selected_plan.isq_singlethread {
                feature_tags.push("isq-serial");
            }
            if bundle.selected_plan.force_no_mmap {
                feature_tags.push("no-mmap");
            }
            if let Some(ref moe) = bundle.selected_plan.moe_experts_backend {
                feature_tags.push(if moe == "fast" { "moe-fast" } else { "moe" });
            }
            lines.push(format!(
                "engine   {}",
                if feature_tags.is_empty() {
                    "default".to_string()
                } else {
                    feature_tags.join(" ")
                }
            ));
        }
        lines.push(format!(
            "gpus     {}",
            truncate_line(
                &bundle.selected_plan.cuda_visible_devices,
                width.saturating_sub(9)
            )
        ));
        if let Some(device_layers) = &bundle.selected_plan.device_layers {
            lines.push(format!(
                "layers   {}",
                truncate_line(device_layers, width.saturating_sub(9))
            ));
        }
        lines.push(format!(
            "speed    {:.0} tok/s",
            bundle.selected_plan.expected_tok_s
        ));

        lines.push(String::new());
        lines.push("presets".to_string());
        for plan in &bundle.plans {
            lines.push(format!(
                "• {} {}k {} kv:{} {:.0} tok/s",
                plan.preset.label(),
                plan.max_seq_len / 1024,
                plan.quantization,
                plan.effective_cache_label(),
                plan.expected_tok_s
            ));
        }

        lines.push(String::new());
        lines.push("gpu budget".to_string());
        for allocation in bundle.selected_plan.gpu_allocations.iter().take(4) {
            lines.push(format!(
                "• GPU{} wt {}G kv {}G free {}G",
                allocation.gpu_index,
                allocation.weight_mb / 1024,
                allocation.kv_cache_mb / 1024,
                allocation.free_headroom_mb / 1024
            ));
        }
    }
}

fn append_aux_model_details(lines: &mut Vec<String>, app: &App, item_key: &str, width: usize) {
    let Some(item) = app.settings_items.iter().find(|item| item.key == item_key) else {
        return;
    };
    let selected_model = item.value.trim();
    if selected_model.is_empty() {
        return;
    }
    lines.push(String::new());
    lines.push("runtime".to_string());
    lines.push(format!(
        "name     {}",
        truncate_line(selected_model, width.saturating_sub(9))
    ));
    if let Ok(profile) = engine::model_profile_for_model(selected_model) {
        lines.push(format!("family   {:?}", profile.runtime.family));
        lines.push(format!(
            "weights  {}",
            truncate_line(
                profile.family_profile.isq.as_deref().unwrap_or("native"),
                width.saturating_sub(9)
            )
        ));
        lines.push(format!(
            "kv cache {}",
            truncate_line(
                profile
                    .family_profile
                    .pa_cache_type
                    .as_deref()
                    .unwrap_or("off"),
                width.saturating_sub(9)
            )
        ));
        lines.push(format!(
            "max ctx  {}k",
            (profile.family_profile.max_seq_len as usize) / 1024
        ));
        lines.push(format!(
            "batch    {}   seqs {}",
            profile.runtime.max_batch_size, profile.runtime.max_seqs
        ));
        lines.push(format!(
            "mode     {}",
            truncate_line(
                &profile.family_profile.launcher_mode,
                width.saturating_sub(9)
            )
        ));
    }
    let health = match item_key {
        "CTOX_EMBEDDING_MODEL" => app.runtime_health.embedding_ready,
        "CTOX_STT_MODEL" => app.runtime_health.stt_ready,
        "CTOX_TTS_MODEL" => app.runtime_health.tts_ready,
        _ => None,
    };
    lines.push(format!(
        "health   {}",
        match health {
            Some(true) => "ready",
            Some(false) => "error",
            None => "unknown",
        }
    ));
}

fn append_general_setting_details(lines: &mut Vec<String>, app: &App, width: usize) {
    lines.push(String::new());
    lines.push("runtime".to_string());
    lines.push(format!("loop     {}", loop_status_label(app)));
    lines.push(format!("source   {}", app.header.chat_source));
    if app
        .header
        .model
        .eq_ignore_ascii_case(&app.header.base_model)
    {
        lines.push(format!(
            "model    {}",
            truncate_line(
                &compact_model_name(&app.header.model, width),
                width.saturating_sub(9)
            )
        ));
    } else {
        lines.push(format!(
            "base     {}",
            truncate_line(
                &compact_model_name(&app.header.base_model, width),
                width.saturating_sub(9)
            )
        ));
        lines.push(format!(
            "active   {}",
            truncate_line(
                &compact_model_name(&app.header.model, width),
                width.saturating_sub(9)
            )
        ));
    }

    lines.push(String::new());
    lines.push("inference".to_string());
    if app.header.estimate_mode {
        lines.push(snapshot_line(
            "preview",
            &truncate_line(&app.header.model, width.saturating_sub(9)),
        ));
    } else if chat_source_is_api(app) {
        lines.push(snapshot_line(
            "remote",
            &truncate_line(&app.header.model, width.saturating_sub(9)),
        ));
    } else {
        lines.push(snapshot_line(
            "loaded",
            &truncate_line(&app.header.model, width.saturating_sub(9)),
        ));
    }
    lines.push(snapshot_line(
        "aux",
        if configured_aux_count(app) > 0 {
            "configured"
        } else {
            "off"
        },
    ));
    if !chat_source_is_api(app) || app.header.estimate_mode {
        lines.extend(local_gpu_snapshot_lines(app, width));
    }

    if let Some(bundle) = &app.chat_preset_bundle {
        lines.push(String::new());
        lines.push("plan".to_string());
        lines.push(snapshot_line(
            "cache",
            bundle.selected_plan.effective_cache_label(),
        ));
        for plan in &bundle.plans {
            lines.push(format!(
                "• {} {} {} {}k {:.0} tok/s",
                plan.preset.label(),
                plan.quantization,
                plan.effective_cache_label(),
                plan.max_seq_len / 1024,
                plan.expected_tok_s
            ));
        }
    }
}

fn chat_source_is_api(app: &App) -> bool {
    app.header.chat_source.eq_ignore_ascii_case("api")
}

fn snapshot_line(label: &str, value: &str) -> String {
    format!("{label:<10}{value}")
}

fn loop_status_label(app: &App) -> &'static str {
    if !app.header.service_running {
        "stopped"
    } else if runtime_health_is_degraded_for_display(app) {
        "degraded"
    } else {
        "running"
    }
}

fn local_gpu_snapshot_lines(app: &App, width: usize) -> Vec<String> {
    if app.header.gpu_cards.is_empty()
        && app.header.gpu_loading_cards.is_empty()
        && app.header.gpu_error_cards.is_empty()
        && app.header.gpu_target_cards.is_empty()
    {
        return vec!["gpu      unavailable".to_string()];
    }
    let mut lines = Vec::new();
    for (gpu_index, live_card, loading_card, error_card, target_card) in
        gpu_display_rows(app).into_iter().take(4)
    {
        let summary = gpu_allocation_summary(live_card, loading_card, error_card, target_card, 4);
        if summary.is_empty() {
            lines.push(format!("gpu{}     idle", gpu_index));
            continue;
        }
        lines.push(format!(
            "gpu{}     {}",
            gpu_index,
            truncate_line(&summary, width.saturating_sub(9))
        ));
    }
    lines
}

#[allow(dead_code)]
fn setting_details_text(app: &App) -> String {
    let Some(item) = app.current_setting() else {
        return String::new();
    };
    let mut lines = vec![app.selected_setting_help()];
    if item.kind == super::SettingKind::Env {
        lines.push(String::new());
        lines.push(format!(
            "saved   {}",
            display_setting_value(item.saved_value.trim(), item.secret)
        ));
        if app.setting_is_dirty(item) {
            lines.push(format!(
                "draft   {}",
                display_setting_value(item.value.trim(), item.secret)
            ));
            lines.push("Enter saves this change.".to_string());
        } else {
            lines.push(format!(
                "active  {}",
                display_setting_value(item.value.trim(), item.secret)
            ));
        }
    }
    if app.settings_menu_open && !item.choices.is_empty() {
        lines.push(String::new());
        lines.push("choose with ↑ ↓ and Enter".to_string());
        for (index, choice) in item.choices.iter().enumerate() {
            let marker = if app.settings_menu_open && app.settings_menu_index == index {
                "›"
            } else if item.value.trim().eq_ignore_ascii_case(choice) {
                "•"
            } else {
                " "
            };
            lines.push(format!("{marker} {choice}"));
        }
    }
    for line in setting_detail_footer_lines(item) {
        lines.push(line.to_string());
    }
    lines.join("\n")
}

#[allow(dead_code)]
fn setting_details_lines(app: &App) -> Vec<Line<'static>> {
    let Some(item) = app.current_setting() else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(app.selected_setting_help())];
    if item.kind == super::SettingKind::Env {
        lines.push(Line::from(String::new()));
        lines.push(Line::from(format!(
            "saved   {}",
            display_setting_value(item.saved_value.trim(), item.secret)
        )));
        if app.setting_is_dirty(item) {
            lines.push(Line::from(format!(
                "draft   {}",
                display_setting_value(item.value.trim(), item.secret)
            )));
            lines.push(Line::from("Enter saves this change.".to_string()));
        } else {
            lines.push(Line::from(format!(
                "active  {}",
                display_setting_value(item.value.trim(), item.secret)
            )));
        }
    }
    if app.settings_menu_open && !item.choices.is_empty() {
        lines.push(Line::from(String::new()));
        lines.push(Line::from("choose with ↑ ↓ and Enter".to_string()));
        for (index, choice) in item.choices.iter().enumerate() {
            let marker = if app.settings_menu_index == index {
                "›"
            } else {
                " "
            };
            let style = if app.settings_menu_index == index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else if item.value.trim().eq_ignore_ascii_case(choice) {
                Style::default().fg(Color::LightGreen)
            } else {
                Style::default().fg(Color::Gray)
            };
            lines.push(Line::from(vec![Span::styled(
                format!("{marker} {choice}"),
                style,
            )]));
        }
    }
    lines.extend(setting_detail_footer_lines(item));
    lines
}

#[allow(dead_code)]
fn setting_detail_footer_lines(item: &super::SettingItem) -> Vec<Line<'static>> {
    match item.key {
        "CTO_EMAIL_PROVIDER" => vec![
            Line::from(String::new()),
            Line::from("Protocols: imap, graph, ews".to_string()),
        ],
        "CTO_JAMI_ACCOUNT_ID" | "CTO_JAMI_PROFILE_NAME" => vec![
            Line::from(String::new()),
            Line::from("mobile  Jami in App Store / Google Play".to_string()),
        ],
        "CTOX_OWNER_PREFERRED_CHANNEL" => vec![
            Line::from(String::new()),
            Line::from("Applies the channel block below this row.".to_string()),
        ],
        _ => Vec::new(),
    }
}

#[allow(dead_code)]
fn signed_delta(delta: i64) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}

fn header_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let allocation_line = combined_gpu_allocation_line(app, width);
    let has_allocation_line = allocation_line
        .spans
        .iter()
        .any(|span| !span.content.trim().is_empty());
    let mut lines = vec![
        combined_gpu_bar_line(app, width),
        combined_gpu_label_line(app, width),
    ];
    if has_allocation_line {
        lines.push(allocation_line);
    }
    lines.push(model_mode_line(app, width));
    lines.push(context_window_line(app, width));
    lines.push(context_label_line(app, width));
    if !has_allocation_line {
        lines.push(context_debug_line(app, width));
    }
    lines
}

fn configured_aux_count(app: &App) -> usize {
    ["CTOX_EMBEDDING_MODEL", "CTOX_STT_MODEL", "CTOX_TTS_MODEL"]
        .iter()
        .filter(|key| {
            app.settings_items
                .iter()
                .find(|item| item.key == **key)
                .is_some_and(|item| !item.value.trim().is_empty())
        })
        .count()
}

fn model_mode_line(app: &App, width: usize) -> Line<'static> {
    let name_width = width.clamp(16, 32) / 3;
    let active_model = compact_model_name(&app.header.model, name_width);
    let base_model = compact_model_name(&app.header.base_model, name_width);
    let boost_model = app
        .header
        .boost_model
        .as_deref()
        .map(|value| compact_model_name(value, name_width))
        .filter(|value| !value.is_empty());
    let mut text = if active_model.eq_ignore_ascii_case(&base_model) {
        format!("source {}  model {active_model}", app.header.chat_source)
    } else {
        format!(
            "source {}  base {base_model}  active {active_model}",
            app.header.chat_source
        )
    };
    if app.header.boost_active {
        if let Some(boost_model) = boost_model {
            text.push_str(&format!("  boost {boost_model}"));
        } else {
            text.push_str("  boost on");
        }
        if let Some(remaining_seconds) = app.header.boost_remaining_seconds {
            let remaining_minutes = (remaining_seconds + 59) / 60;
            text.push_str(&format!("  {}m left", remaining_minutes));
        }
        if let Some(reason) = app.header.boost_reason.as_deref() {
            let room = width.saturating_sub(text.chars().count() + 2);
            if room > 8 {
                text.push_str(&format!("  {}", truncate_line(reason, room)));
            }
        }
    } else if let Some(boost_model) = boost_model {
        text.push_str(&format!("  boost {boost_model} idle"));
    }
    Line::from(truncate_line(&text, width))
}

fn render_settings_view_tabs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let selected = match app.settings_view {
        SettingsView::Model => 0,
        SettingsView::Communication => 1,
        SettingsView::Secrets => 2,
        SettingsView::Paths => 3,
        SettingsView::Update => 4,
        SettingsView::HarnessMining => 5,
    };
    let titles = [
        "Model",
        "Communication",
        "Secrets",
        "Paths",
        "Update",
        "Harness",
    ]
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    let widget = Tabs::new(titles)
        .select(selected)
        .divider(" ")
        .padding("", "")
        .style(Style::default().fg(Color::DarkGray).bg(Color::Rgb(8, 8, 8)))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(widget, area);
}

fn combined_gpu_bar_line(app: &App, width: usize) -> Line<'static> {
    if app.header.gpu_cards.is_empty()
        && app.header.gpu_loading_cards.is_empty()
        && app.header.gpu_error_cards.is_empty()
        && app.header.gpu_target_cards.is_empty()
    {
        return Line::from(truncate_line("GPU telemetry unavailable", width));
    }
    let per_gpu_width = gpu_segment_width(app, width);
    let mut spans = Vec::new();
    for (idx, (_gpu_index, live_card, loading_card, error_card, target_card)) in
        gpu_display_rows(app).into_iter().enumerate()
    {
        if idx > 0 {
            spans.push(Span::raw("  "));
        }
        spans.extend(gpu_usage_bar_spans(
            app,
            live_card,
            loading_card,
            error_card,
            target_card,
            per_gpu_width,
        ));
    }
    truncate_line_spans(spans, width)
}

fn combined_gpu_label_line(app: &App, width: usize) -> Line<'static> {
    if app.gpu_cards.is_empty()
        && app.header.gpu_loading_cards.is_empty()
        && app.header.gpu_error_cards.is_empty()
        && app.header.gpu_target_cards.is_empty()
    {
        return Line::from(String::new());
    }
    let per_gpu_width = gpu_segment_width(app, width);
    let mut text = String::new();
    for (idx, (gpu_index, live_card, loading_card, error_card, target_card)) in
        gpu_display_rows(app).into_iter().enumerate()
    {
        if idx > 0 {
            text.push_str("  ");
        }
        let card = app
            .gpu_cards
            .iter()
            .find(|candidate| candidate.index == gpu_index)
            .or(live_card)
            .or(loading_card)
            .or(error_card)
            .or(target_card);
        let Some(card) = card else {
            continue;
        };
        let segment = format!(
            "GPU{} {}/{}G {}%",
            card.index,
            card.used_mb / 1024,
            card.total_mb / 1024,
            card.utilization
        );
        text.push_str(&pad_to_width(&segment, per_gpu_width));
    }
    if let Some(avg_tps) = app.header.avg_tokens_per_second {
        let suffix = format!(" {:>3.0} tok/s", avg_tps);
        if text.chars().count() + suffix.chars().count() <= width {
            text.push_str(&suffix);
        }
    }
    Line::from(truncate_line(&text, width))
}

fn combined_gpu_allocation_line(app: &App, width: usize) -> Line<'static> {
    if app.header.gpu_cards.is_empty()
        && app.header.gpu_loading_cards.is_empty()
        && app.header.gpu_error_cards.is_empty()
        && app.header.gpu_target_cards.is_empty()
    {
        return Line::from(String::new());
    }
    let text = gpu_display_rows(app)
        .into_iter()
        .map(
            |(gpu_index, live_card, loading_card, error_card, target_card)| {
                let summary =
                    gpu_allocation_summary(live_card, loading_card, error_card, target_card, 3);
                if summary.is_empty() {
                    format!("GPU{} idle", gpu_index)
                } else {
                    format!("GPU{} {}", gpu_index, summary)
                }
            },
        )
        .collect::<Vec<_>>()
        .join("  ");
    Line::from(truncate_line(&text, width))
}

fn gpu_segment_width(app: &App, width: usize) -> usize {
    let gpu_count = gpu_display_rows(app).len().max(1);
    ((width.saturating_sub(gpu_count.saturating_sub(1) * 2)) / gpu_count).clamp(10, 30)
}

fn gpu_display_rows(
    app: &App,
) -> Vec<(
    usize,
    Option<&super::GpuCardState>,
    Option<&super::GpuCardState>,
    Option<&super::GpuCardState>,
    Option<&super::GpuCardState>,
)> {
    let mut rows = Vec::new();
    let mut seen = Vec::new();

    for target_card in &app.header.gpu_target_cards {
        if seen.contains(&target_card.index) {
            continue;
        }
        seen.push(target_card.index);
        let live_card = app
            .header
            .gpu_cards
            .iter()
            .find(|candidate| candidate.index == target_card.index);
        let loading_card = app
            .header
            .gpu_loading_cards
            .iter()
            .find(|candidate| candidate.index == target_card.index);
        let error_card = app
            .header
            .gpu_error_cards
            .iter()
            .find(|candidate| candidate.index == target_card.index);
        rows.push((
            target_card.index,
            live_card,
            loading_card,
            error_card,
            Some(target_card),
        ));
    }

    for loading_card in &app.header.gpu_loading_cards {
        if seen.contains(&loading_card.index) {
            continue;
        }
        seen.push(loading_card.index);
        let live_card = app
            .header
            .gpu_cards
            .iter()
            .find(|candidate| candidate.index == loading_card.index);
        let error_card = app
            .header
            .gpu_error_cards
            .iter()
            .find(|candidate| candidate.index == loading_card.index);
        rows.push((
            loading_card.index,
            live_card,
            Some(loading_card),
            error_card,
            None,
        ));
    }

    for error_card in &app.header.gpu_error_cards {
        if seen.contains(&error_card.index) {
            continue;
        }
        seen.push(error_card.index);
        let live_card = app
            .header
            .gpu_cards
            .iter()
            .find(|candidate| candidate.index == error_card.index);
        rows.push((error_card.index, live_card, None, Some(error_card), None));
    }

    for live_card in &app.header.gpu_cards {
        if seen.contains(&live_card.index) {
            continue;
        }
        seen.push(live_card.index);
        rows.push((live_card.index, Some(live_card), None, None, None));
    }

    rows.sort_unstable_by_key(|(index, _, _, _, _)| *index);
    rows
}

fn pad_to_width(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        truncate_line(text, width)
    } else {
        format!("{text}{:width$}", "", width = width - len)
    }
}

fn context_window_line(app: &App, width: usize) -> Line<'static> {
    let bar_width = width.saturating_sub(2).max(16);
    let reference_total = CONTEXT_BAR_REFERENCE_TOKENS.max(app.header.max_context.max(1));
    let fill_index = ratio_index(app.header.current_tokens, reference_total, bar_width);
    let compact_index = ratio_index(app.header.compact_at, reference_total, bar_width);
    let configured_index = ratio_index(app.header.configured_context, reference_total, bar_width);
    let runtime_index = ratio_index(app.header.max_context, reference_total, bar_width);
    let chat_color = role_color("chat");
    let mut spans = vec![Span::raw("▕")];
    for idx in 0..bar_width {
        let in_runtime_window = idx < runtime_index.max(1);
        let bg = if idx < fill_index {
            chat_color
        } else if in_runtime_window {
            Color::Rgb(32, 32, 32)
        } else {
            Color::Rgb(10, 10, 10)
        };
        if idx == compact_index.min(bar_width.saturating_sub(1)) {
            spans.push(Span::styled("◆", Style::default().fg(Color::White).bg(bg)));
        } else if idx == runtime_index.min(bar_width.saturating_sub(1)) && runtime_index < bar_width
        {
            spans.push(Span::styled(
                "│",
                Style::default().fg(Color::LightCyan).bg(bg),
            ));
        } else if idx == configured_index.min(bar_width.saturating_sub(1)) {
            spans.push(Span::styled("▲", Style::default().fg(chat_color).bg(bg)));
        } else {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
    }
    spans.push(Span::raw("▏"));
    truncate_line_spans(spans, width)
}

fn context_label_line(app: &App, width: usize) -> Line<'static> {
    let budget_suffix = refresh_budget_suffix(app);
    let text = if let Some(health) = app.context_health.as_ref() {
        let status = health.status.as_str();
        let hint = context_health_hint(app);
        let warning_suffix = if health.warnings.is_empty() {
            String::new()
        } else {
            format!("  {} warning(s)", health.warnings.len())
        };
        format!(
            "context {status} {}  {hint}{warning_suffix}{budget_suffix}",
            health.overall_score
        )
    } else {
        format!(
            "context pending  {}k used of {}k live{budget_suffix}",
            app.header.current_tokens / 1024,
            app.header.max_context / 1024
        )
    };
    Line::from(truncate_line(&text, width))
}

/// Compact suffix for the context status line showing how much of the
/// output-refresh budget is currently used. Returns an empty string when
/// there is nothing to show (fresh conversation, budget disabled).
fn refresh_budget_suffix(app: &App) -> String {
    let pct = 15;
    if pct == 0 {
        return String::new();
    }
    let snapshot = crate::execution::agent::turn_loop::refresh_budget_snapshot(
        crate::execution::agent::turn_loop::CHAT_CONVERSATION_ID,
        app.header.max_context as u64,
        pct,
    );
    if snapshot.output_chars_since_refresh == 0 {
        return String::new();
    }
    format!("  • refresh {}%/{}%", snapshot.used_pct.min(999), pct)
}

fn context_health_hint(app: &App) -> String {
    let Some(health) = app.context_health.as_ref() else {
        return "waiting for the next context sample".to_string();
    };
    match health.status {
        ContextHealthStatus::Healthy => {
            if let Some(mission) = app.mission_state.as_ref().filter(|mission| mission.is_open) {
                if mission.allow_idle {
                    "stable and safely idling between mission triggers".to_string()
                } else {
                    "stable and on track".to_string()
                }
            } else {
                "stable and ready".to_string()
            }
        }
        ContextHealthStatus::Watch => "watch drift and refresh focus soon".to_string(),
        ContextHealthStatus::Degraded => "repair focus before more risky retries".to_string(),
        ContextHealthStatus::Critical => "stop and rebuild mission context".to_string(),
    }
}

fn context_debug_line(app: &App, width: usize) -> Line<'static> {
    let Some(breakdown) = app.prompt_context_breakdown.as_ref() else {
        return Line::from(String::new());
    };
    let mut parts = vec![
        format!(
            "ctx dbg sys {}",
            compact_debug_size(breakdown.system_prompt_chars)
        ),
        format!("state {}", compact_debug_size(breakdown.continuity_chars())),
        format!(
            "evid {}",
            compact_debug_size(breakdown.verified_evidence_chars + breakdown.workflow_state_chars)
        ),
        format!(
            "gov+hlth {}",
            compact_debug_size(breakdown.governance_chars + breakdown.context_health_chars)
        ),
        format!("conv {}", compact_debug_size(breakdown.conversation_chars)),
        format!(
            "user {}",
            compact_debug_size(breakdown.latest_user_turn_chars)
        ),
        format!("wrap {}", compact_debug_size(breakdown.wrapper_chars)),
        format!(
            "ctox {}",
            compact_debug_size(breakdown.total_ctox_prompt_chars)
        ),
    ];
    if let Some(input_tokens) = app.header.last_input_tokens {
        let approx_prompt_chars = (input_tokens as usize).saturating_mul(4);
        let residual = approx_prompt_chars.saturating_sub(breakdown.total_ctox_prompt_chars);
        parts.push(format!("codex/tool~ {}", compact_debug_size(residual)));
    }
    if breakdown.omitted_context_items > 0 {
        parts.push(format!("omit {}", breakdown.omitted_context_items));
    }
    Line::from(truncate_line(&parts.join("  "), width))
}

fn compact_debug_size(chars: usize) -> String {
    if chars >= 10_000 {
        format!("{:.1}k", (chars as f64) / 1000.0)
    } else {
        chars.to_string()
    }
}

fn non_empty_mission_value(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed.eq_ignore_ascii_case("n/a")
        || trimmed.eq_ignore_ascii_case("na")
    {
        None
    } else {
        Some(trimmed)
    }
}

fn fallback_label<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

fn gpu_usage_bar_spans(
    app: &App,
    live_card: Option<&super::GpuCardState>,
    loading_card: Option<&super::GpuCardState>,
    error_card: Option<&super::GpuCardState>,
    target_card: Option<&super::GpuCardState>,
    width: usize,
) -> Vec<Span<'static>> {
    let bar_width = width.saturating_sub(2).clamp(8, 24);
    let total_mb = live_card
        .map(|card| card.total_mb)
        .unwrap_or(0)
        .max(loading_card.map(|card| card.total_mb).unwrap_or(0))
        .max(error_card.map(|card| card.total_mb).unwrap_or(0))
        .max(target_card.map(|card| card.total_mb).unwrap_or(0));
    if total_mb == 0 {
        return vec![Span::styled(
            "[no-gpu-data]",
            Style::default().fg(Color::DarkGray),
        )];
    }

    let mut glyphs = (0..bar_width)
        .map(|_| ('.', Style::default().fg(Color::DarkGray)))
        .collect::<Vec<_>>();

    paint_gpu_layer(&mut glyphs, target_card, total_mb, |allocation, _idx| {
        (
            allocation_bar_char(&allocation.short_label, false),
            Style::default().fg(dim_color(model_color(&allocation.model))),
        )
    });
    paint_gpu_layer(&mut glyphs, live_card, total_mb, |allocation, _idx| {
        ('█', Style::default().fg(model_color(&allocation.model)))
    });
    paint_gpu_layer(&mut glyphs, loading_card, total_mb, |allocation, idx| {
        let blink_on = (app.spinner_phase + idx) % 4 < 2;
        let glyph = if blink_on { '▓' } else { '▒' };
        (glyph, Style::default().fg(model_color(&allocation.model)))
    });
    paint_gpu_layer(&mut glyphs, error_card, total_mb, |_allocation, idx| {
        let blink_on = (app.spinner_phase + idx) % 4 < 2;
        let glyph = if blink_on { 'E' } else { 'e' };
        (glyph, Style::default().fg(Color::Red))
    });

    let mut spans = Vec::new();
    spans.push(Span::styled("[", Style::default().fg(Color::Gray)));
    for (glyph, style) in glyphs {
        spans.push(Span::styled(glyph.to_string(), style));
    }
    spans.push(Span::styled("]", Style::default().fg(Color::Gray)));
    spans
}

fn gpu_allocation_summary(
    live_card: Option<&super::GpuCardState>,
    loading_card: Option<&super::GpuCardState>,
    error_card: Option<&super::GpuCardState>,
    target_card: Option<&super::GpuCardState>,
    max_items: usize,
) -> String {
    if let Some(error_card) = error_card.filter(|card| !card.allocations.is_empty()) {
        let mut parts = error_card
            .allocations
            .iter()
            .take(max_items)
            .map(|allocation| format!("{} error {}M", allocation.short_label, allocation.used_mb))
            .collect::<Vec<_>>();
        if error_card.allocations.len() > max_items {
            parts.push(format!("+{}", error_card.allocations.len() - max_items));
        }
        return parts.join(" + ");
    }

    if let Some(loading_card) = loading_card.filter(|card| !card.allocations.is_empty()) {
        let mut parts = loading_card
            .allocations
            .iter()
            .take(max_items)
            .map(|allocation| format!("{} ~{}M", allocation.short_label, allocation.used_mb))
            .collect::<Vec<_>>();
        if loading_card.allocations.len() > max_items {
            parts.push(format!("+{}", loading_card.allocations.len() - max_items));
        }
        return parts.join(" + ");
    }

    if let Some(target_card) = target_card.filter(|card| !card.allocations.is_empty()) {
        let mut parts = target_card
            .allocations
            .iter()
            .take(max_items)
            .map(|allocation| {
                let live_used = live_card
                    .and_then(|card| {
                        card.allocations
                            .iter()
                            .find(|candidate| candidate.model == allocation.model)
                    })
                    .map(|usage| usage.used_mb)
                    .unwrap_or(0);
                if live_used == allocation.used_mb {
                    format!("{} {}M", allocation.short_label, allocation.used_mb)
                } else {
                    format!(
                        "{} {}/{}M",
                        allocation.short_label, live_used, allocation.used_mb
                    )
                }
            })
            .collect::<Vec<_>>();
        if target_card.allocations.len() > max_items {
            parts.push(format!("+{}", target_card.allocations.len() - max_items));
        }
        return parts.join(" + ");
    }

    let Some(live_card) = live_card else {
        return String::new();
    };
    let mut parts = live_card
        .allocations
        .iter()
        .take(max_items)
        .map(|allocation| format!("{} {}M", allocation.short_label, allocation.used_mb))
        .collect::<Vec<_>>();
    if live_card.allocations.len() > max_items {
        parts.push(format!("+{}", live_card.allocations.len() - max_items));
    }
    parts.join(" + ")
}

fn paint_gpu_layer<F>(
    glyphs: &mut [(char, Style)],
    card: Option<&super::GpuCardState>,
    total_mb: u64,
    mut painter: F,
) where
    F: FnMut(&super::GpuModelUsage, usize) -> (char, Style),
{
    let Some(card) = card else {
        return;
    };
    if card.allocations.is_empty() || total_mb == 0 || glyphs.is_empty() {
        return;
    }
    let bar_width = glyphs.len();
    let mut painted = 0usize;
    for allocation in &card.allocations {
        let seg =
            ((allocation.used_mb as f64 / total_mb as f64) * bar_width as f64).round() as usize;
        let seg = seg.max(1).min(bar_width.saturating_sub(painted));
        if seg == 0 {
            continue;
        }
        for offset in 0..seg {
            let cell_index = painted + offset;
            let (glyph, style) = painter(allocation, cell_index);
            glyphs[cell_index] = (glyph, style);
        }
        painted = painted.saturating_add(seg);
        if painted >= bar_width {
            break;
        }
    }
}

fn dim_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_div(2).saturating_add(24),
            g.saturating_div(2).saturating_add(24),
            b.saturating_div(2).saturating_add(24),
        ),
        Color::Yellow => Color::Rgb(140, 140, 24),
        Color::LightMagenta => Color::Rgb(140, 24, 140),
        Color::LightGreen => Color::Rgb(24, 140, 24),
        Color::Cyan | Color::LightCyan => Color::Rgb(24, 120, 120),
        _ => Color::Rgb(80, 80, 80),
    }
}

fn allocation_bar_char(label: &str, loaded: bool) -> char {
    let base = match label.trim().to_ascii_lowercase().as_str() {
        "embed" => 'e',
        "stt" => 's',
        "tts" => 't',
        "chat" => 'c',
        other => other.chars().next().unwrap_or('m'),
    };
    if loaded {
        base.to_ascii_uppercase()
    } else {
        base
    }
}

fn model_color(model: &str) -> Color {
    role_color(model_role(model))
}

fn model_role(model: &str) -> &'static str {
    let lower = model.to_ascii_lowercase();
    if lower.contains("embedding") {
        "embed"
    } else if lower.contains("voxtral") || lower.contains("stt") {
        "stt"
    } else if lower.contains("tts") {
        "tts"
    } else {
        "chat"
    }
}

fn role_color(role: &str) -> Color {
    match role {
        "embed" => Color::Yellow,
        "stt" => Color::LightMagenta,
        "tts" => Color::LightGreen,
        _ => Color::Cyan,
    }
}

fn setting_row_style(key: &str, _value: &str) -> Style {
    let color = match key {
        "CTOX_CHAT_SOURCE"
        | "CTOX_API_PROVIDER"
        | "CTOX_CHAT_MODEL"
        | "CTOX_CHAT_LOCAL_PRESET"
        | "CTOX_CHAT_SKILL_PRESET" => role_color("chat"),
        "CTOX_EMBEDDING_MODEL" => role_color("embed"),
        "CTOX_STT_MODEL" => role_color("stt"),
        "CTOX_TTS_MODEL" => role_color("tts"),
        "CTOX_OWNER_NAME"
        | "CTOX_OWNER_PREFERRED_CHANNEL"
        | "CTO_EMAIL_ADDRESS"
        | "CTO_EMAIL_PROVIDER"
        | "CTO_EMAIL_IMAP_HOST"
        | "CTO_EMAIL_IMAP_PORT"
        | "CTO_EMAIL_SMTP_HOST"
        | "CTO_EMAIL_SMTP_PORT"
        | "CTO_EMAIL_GRAPH_USER"
        | "CTO_EMAIL_EWS_URL"
        | "CTO_EMAIL_EWS_AUTH_TYPE"
        | "CTO_EMAIL_EWS_USERNAME"
        | "CTO_JAMI_PROFILE_NAME"
        | "CTO_JAMI_ACCOUNT_ID"
        | "CTO_TEAMS_USERNAME"
        | "CTO_TEAMS_PASSWORD"
        | "CTO_TEAMS_TENANT_ID"
        | "CTO_TEAMS_TEAM_ID"
        | "CTO_TEAMS_CHANNEL_ID" => Color::LightBlue,
        _ => Color::White,
    };
    Style::default().fg(color)
}

fn ratio_index(value: usize, total: usize, width: usize) -> usize {
    if total == 0 || width == 0 {
        0
    } else {
        ((value.min(total) as f64 / total as f64) * width as f64).floor() as usize
    }
}

#[allow(dead_code)]
fn header_preview_line(app: &App, width: usize) -> Line<'static> {
    let model_item = app
        .settings_items
        .iter()
        .find(|item| item.key == "CTOX_CHAT_MODEL");
    let preset_item = app
        .settings_items
        .iter()
        .find(|item| item.key == "CTOX_CHAT_LOCAL_PRESET");
    let channel_item = app
        .settings_items
        .iter()
        .find(|item| item.key == "CTOX_OWNER_PREFERRED_CHANNEL");
    if let Some(model_item) = model_item {
        if app.setting_is_dirty(model_item) {
            let loaded = engine::model_profile_for_model(&model_item.saved_value).ok();
            let draft = engine::model_profile_for_model(&model_item.value).ok();
            if let (Some(loaded), Some(draft)) = (loaded, draft) {
                let draft_compact = app.header.compact_at;
                let ctx_delta = draft.family_profile.max_seq_len as i64
                    - loaded.family_profile.max_seq_len as i64;
                let compact_delta = draft_compact as i64 - app.header.compact_at as i64;
                let loaded_tps = app
                    .model_perf_stats
                    .get(model_item.saved_value.trim())
                    .map(|stats| stats.avg_tokens_per_second);
                let draft_tps = app
                    .model_perf_stats
                    .get(model_item.value.trim())
                    .map(|stats| stats.avg_tokens_per_second);
                return truncate_line_spans(
                    vec![
                        Span::styled(
                            "draft ",
                            Style::default()
                                .fg(Color::LightYellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            compact_model_name(&model_item.value, 18),
                            Style::default().fg(Color::LightCyan),
                        ),
                        Span::raw("  ctx "),
                        Span::raw(draft.family_profile.max_seq_len.to_string()),
                        delta_span(ctx_delta),
                        Span::raw("  compact "),
                        Span::raw(draft_compact.to_string()),
                        delta_span(compact_delta),
                        Span::raw("  avg tok/s "),
                        Span::styled(
                            draft_tps
                                .map(|value| format!("{value:.1}"))
                                .unwrap_or_else(|| "-".to_string()),
                            Style::default().fg(Color::White),
                        ),
                        delta_float_span(
                            draft_tps
                                .zip(loaded_tps)
                                .map(|(draft, loaded)| draft - loaded),
                        ),
                    ],
                    width,
                );
            }
        }
    }
    if let Some(preset_item) = preset_item {
        if app.setting_is_dirty(preset_item) {
            let cache_label = app
                .chat_preset_bundle
                .as_ref()
                .map(|bundle| bundle.selected_plan.effective_cache_label().to_string())
                .unwrap_or_else(|| "auto".to_string());
            return truncate_line_spans(
                vec![
                    Span::styled(
                        "draft ",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("preset "),
                    Span::raw(preset_item.value.clone()),
                    Span::raw("  cache "),
                    Span::raw(cache_label),
                    Span::raw("  compact "),
                    Span::raw(format!(
                        "{}% >= {}k",
                        app.header.compact_percent,
                        app.header.compact_min_tokens / 1024
                    )),
                ],
                width,
            );
        }
    }
    if let Some(channel_item) = channel_item {
        if app.setting_is_dirty(channel_item) {
            return truncate_line_spans(
                vec![
                    Span::styled(
                        "draft ",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("channel "),
                    Span::styled(
                        channel_item.value.trim().to_string(),
                        Style::default().fg(Color::LightCyan),
                    ),
                ],
                width,
            );
        }
    }
    Line::from(vec![Span::styled(
        "loaded state",
        Style::default().fg(Color::DarkGray),
    )])
}

#[allow(dead_code)]
fn delta_span(delta: i64) -> Span<'static> {
    let style = if delta > 0 {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else if delta < 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Span::styled(format!(" {}", signed_delta(delta)), style)
}

#[allow(dead_code)]
fn delta_float_span(delta: Option<f64>) -> Span<'static> {
    match delta {
        Some(value) if value > 0.0 => Span::styled(
            format!(" +{value:.1}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Some(value) if value < 0.0 => Span::styled(
            format!(" {value:.1}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Some(_) => Span::styled(" 0.0".to_string(), Style::default().fg(Color::DarkGray)),
        None => Span::styled(" -".to_string(), Style::default().fg(Color::DarkGray)),
    }
}

fn truncate_line_spans(spans: Vec<Span<'static>>, max_chars: usize) -> Line<'static> {
    if max_chars == 0 {
        return Line::from(String::new());
    }
    let text = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    if text.chars().count() <= max_chars {
        return Line::from(spans);
    }
    let truncated = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…";
    Line::from(truncated)
}

fn display_setting_value(value: &str, secret: bool) -> String {
    if value.is_empty() {
        "-".to_string()
    } else if secret {
        "********".to_string()
    } else {
        value.to_string()
    }
}

fn truncate_line(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::runtime_plan;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_app() -> App {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut db_path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        db_path.push(format!("ctox-tui-render-{stamp}.db"));
        let mut app = App::new(root, db_path);
        app.page = Page::Chat;
        app
    }

    fn test_plan(
        model: &str,
        preset: runtime_plan::ChatPreset,
        cache: Option<&str>,
    ) -> runtime_plan::ChatRuntimePlan {
        runtime_plan::ChatRuntimePlan {
            model: model.to_string(),
            preset,
            quantization: "Q4_K_M".to_string(),
            runtime_isq: Some("Q4_K_M".to_string()),
            max_seq_len: 16_384,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: false,
            paged_attn: "auto".to_string(),
            pa_cache_type: cache.map(str::to_string),
            pa_memory_fraction: None,
            pa_context_len: None,
            disable_nccl: false,
            tensor_parallel_backend: Some("nccl".to_string()),
            mn_local_world_size: Some(3),
            max_batch_size: 64,
            max_seqs: 16,
            cuda_visible_devices: "0,1,2".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: None,
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: Some(16),
            expected_tok_s: 92.0,
            hardware_fingerprint: "test-gpu".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 20_480,
                kv_budget_cap_mb: 8_192,
                kv_budget_fraction_milli: 800,
                weight_residency_mb: 4_096,
                kv_cache_mb: 2_048,
                fixed_runtime_base_overhead_mb: 0,
                backend_runtime_overhead_mb: 256,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                safety_headroom_mb: 0,
                required_effective_total_budget_mb: 7_680,
                required_total_mb: 8_192,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![],
            moe_cache: None,
        }
    }

    fn test_bundle(model: &str, cache: Option<&str>) -> runtime_plan::ChatPresetBundle {
        let selected_plan = test_plan(model, runtime_plan::ChatPreset::Quality, cache);
        runtime_plan::ChatPresetBundle {
            model: model.to_string(),
            hardware: runtime_plan::HardwareProfile {
                gpus: vec![runtime_plan::HardwareGpu {
                    index: 0,
                    name: "RTX A4500".to_string(),
                    total_mb: 20_480,
                }],
                gpu0_desktop_reserve_mb: 1_024,
                fingerprint: "test-gpu".to_string(),
            },
            selected_preset: runtime_plan::ChatPreset::Quality,
            selected_plan: selected_plan.clone(),
            plans: vec![
                selected_plan,
                test_plan(model, runtime_plan::ChatPreset::Performance, cache),
            ],
        }
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area;
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn chat_sidebar_footer_visible_in_24_rows() {
        let backend = TestBackend::new(150, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = test_app();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("interrupt"), "{text}");
        assert!(text.contains("next page"), "{text}");
        assert!(text.contains("quit"), "{text}");
    }

    #[test]
    fn chat_view_shows_turn_state_and_structured_roles() {
        let backend = TestBackend::new(140, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.service_status.running = true;
        app.service_status.busy = true;
        app.runtime_health = crate::tui::RuntimeHealthState {
            runtime_ready: true,
            embedding_ready: Some(true),
            stt_ready: Some(true),
            tts_ready: Some(true),
        };
        app.service_status.active_source_label = Some("queue".to_string());
        app.service_status.current_goal_preview =
            Some("Installiere Nextcloud sauber und pruefe danach den Login.".to_string());
        app.service_status.pending_count = 2;
        app.service_status.last_reply_chars = Some(534);
        app.service_status.recent_events = vec![
            "phase queue compaction-check".to_string(),
            "phase queue invoke-model".to_string(),
            "Started queued queue prompt".to_string(),
            "Completed queue reply with 318 chars".to_string(),
        ];
        app.context_health = Some(crate::context_health::ContextHealthSnapshot {
            conversation_id: 1,
            overall_score: 96,
            status: crate::context_health::ContextHealthStatus::Healthy,
            summary: "healthy".to_string(),
            repair_recommended: false,
            dimensions: Vec::new(),
            warnings: Vec::new(),
        });
        app.mission_state = Some(crate::lcm::MissionStateRecord {
            conversation_id: 1,
            mission: "Build and operate the Airbnb clone.".to_string(),
            mission_status: "active".to_string(),
            continuation_mode: "continuous".to_string(),
            trigger_intensity: "hot".to_string(),
            blocker: "none".to_string(),
            next_slice: "Implement host onboarding.".to_string(),
            done_gate: "Keep capability audit open.".to_string(),
            closure_confidence: "low".to_string(),
            is_open: true,
            allow_idle: false,
            focus_head_commit_id: "focus-1".to_string(),
            last_synced_at: "2026-03-31T00:00:00Z".to_string(),
            watcher_last_triggered_at: None,
            watcher_trigger_count: 0,
        });
        app.chat_messages.push(crate::lcm::MessageRecord {
            message_id: 1,
            conversation_id: 1,
            seq: 1,
            role: "user".to_string(),
            content: "Installiere Nextcloud.".to_string(),
            created_at: "2026-03-26T10:00:00Z".to_string(),
            token_count: 10,
        });
        app.chat_messages.push(crate::lcm::MessageRecord {
            message_id: 2,
            conversation_id: 1,
            seq: 2,
            role: "assistant".to_string(),
            content: "Nextcloud bleibt blockiert.".to_string(),
            created_at: "2026-03-26T10:00:01Z".to_string(),
            token_count: 12,
        });
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("turn"), "{text}");
        assert!(text.contains("loop working"), "{text}");
        assert!(text.contains("queue"), "{text}");
        assert!(text.contains("active"), "{text}");
        assert!(text.contains("missions"), "{text}");
        assert!(
            text.contains("Build and operate the Airbnb clone"),
            "{text}"
        );
        assert!(text.contains("context healthy 96"), "{text}");
        assert!(text.contains("goal"), "{text}");
        assert!(text.contains("invoke-model"), "{text}");
        assert!(text.contains("CTOX"), "{text}");
        assert!(text.contains("YOU"), "{text}");
    }

    #[test]
    fn internal_follow_up_turns_are_labeled_as_auto() {
        let backend = TestBackend::new(140, 34);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.chat_messages.push(crate::lcm::MessageRecord {
            message_id: 1,
            conversation_id: 1,
            seq: 1,
            role: "user".to_string(),
            content: "Review the blocked owner-visible task without losing continuity.\n\nGoal:\nInstall Redis cleanly\n\nThe latest attempt failed or stalled with this blocker:\nexecution timed out after 180s\n".to_string(),
            created_at: "2026-03-26T10:00:00Z".to_string(),
            token_count: 40,
        });

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("AUTO"), "{text}");
        assert!(text.contains("system follow-up"), "{text}");
        assert!(text.contains("goal Install Redis cleanly"), "{text}");
        assert!(
            text.contains("blocker execution timed out after 180s"),
            "{text}"
        );
    }

    #[test]
    fn chat_header_shows_boost_state() {
        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.header.chat_source = "local".to_string();
        app.header.model = "gpt-5.4".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.boost_model = Some("gpt-5.4".to_string());
        app.header.boost_active = true;
        app.header.boost_remaining_seconds = Some(7 * 60);
        app.header.boost_reason = Some("stuck in repair loop".to_string());
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("base gpt-5.4-mini"), "{text}");
        assert!(text.contains("boost gpt-5.4"), "{text}");
        assert!(text.contains("7m left"), "{text}");
    }

    #[test]
    fn chat_header_collapses_identical_base_and_active_models() {
        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("source api model gpt-5.4-mini"), "{text}");
        assert!(
            !text.contains("base gpt-5.4-mini  active gpt-5.4-mini"),
            "{text}"
        );
    }

    #[test]
    fn settings_view_renders_model_and_communication_tabs() {
        let backend = TestBackend::new(140, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.page = Page::Settings;

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("Model"), "{text}");
        assert!(text.contains("Communication"), "{text}");
    }

    #[test]
    fn chat_header_shows_api_source_and_gpu_allocations() {
        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 1536,
            total_mb: 20_480,
            utilization: 8,
            allocations: vec![super::super::GpuModelUsage {
                model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                short_label: "embed".to_string(),
                used_mb: 512,
            }],
        }];

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("source api"), "{text}");
        assert!(text.contains("GPU0 embed 512M"), "{text}");
    }

    #[test]
    fn settings_snapshot_distinguishes_remote_chat_from_local_load() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 1024,
            total_mb: 20_480,
            utilization: 3,
            allocations: vec![super::super::GpuModelUsage {
                model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                short_label: "embed".to_string(),
                used_mb: 512,
            }],
        }];

        let text = settings_snapshot_text(&app, 80, 40);

        assert!(text.contains("remote   gpt-5.4-mini"), "{text}");
        assert!(text.contains("aux config"), "{text}");
        assert!(text.contains("local load"), "{text}");
        assert!(text.contains("gpu0     embed 512M"), "{text}");
    }

    #[test]
    fn settings_snapshot_keeps_preview_wording_in_estimate_mode() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.header.estimate_mode = true;
        app.header.chat_source = "local".to_string();
        app.header.model = "Qwen/Qwen3.6-35B-A3B".to_string();
        app.header.base_model = "Qwen/Qwen3.6-35B-A3B".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 12_288,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![super::super::GpuModelUsage {
                model: "Qwen/Qwen3.6-35B-A3B".to_string(),
                short_label: "qwen36".to_string(),
                used_mb: 11_264,
            }],
        }];

        let text = settings_snapshot_text(&app, 80, 40);

        assert!(text.contains("preview  Qwen/Qwen3.6-35B-A3B"), "{text}");
        assert!(text.contains("local est"), "{text}");
        assert!(text.contains("gpu0     qwen36 11264M"), "{text}");
    }

    #[test]
    fn settings_snapshot_lists_aux_gpu_roles_explicitly() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 6_700,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![
                super::super::GpuModelUsage {
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    short_label: "embed".to_string(),
                    used_mb: 1_100,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    short_label: "stt".to_string(),
                    used_mb: 3_200,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                    short_label: "tts".to_string(),
                    used_mb: 2_400,
                },
            ],
        }];

        let text = settings_snapshot_text(&app, 120, 40);

        assert!(
            text.contains("gpu0     embed 1100M + stt 3200M + tts 2400M"),
            "{text}"
        );
    }

    #[test]
    fn settings_snapshot_shows_loop_and_aux_targets_with_zero_progress() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.header.service_running = true;
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 0,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![],
        }];
        app.header.gpu_target_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 6_700,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![
                super::super::GpuModelUsage {
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    short_label: "embed".to_string(),
                    used_mb: 1_100,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    short_label: "stt".to_string(),
                    used_mb: 4_200,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                    short_label: "tts".to_string(),
                    used_mb: 1_400,
                },
            ],
        }];

        let text = settings_snapshot_text(&app, 120, 40);

        assert!(text.contains("loop     running"), "{text}");
        assert!(
            text.contains("gpu0     embed 0/1100M + stt 0/4200M + tts 0/1400M"),
            "{text}"
        );
    }

    #[test]
    fn chat_header_shows_targeted_aux_progress_ranges() {
        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 0,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![],
        }];
        app.header.gpu_target_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 6_700,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![
                super::super::GpuModelUsage {
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    short_label: "embed".to_string(),
                    used_mb: 1_100,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    short_label: "stt".to_string(),
                    used_mb: 4_200,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                    short_label: "tts".to_string(),
                    used_mb: 1_400,
                },
            ],
        }];

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("GPU0 embed 0/1100M"), "{text}");
        assert!(!text.contains("warmup expected"), "{text}");
    }

    #[test]
    fn chat_header_overlays_target_ranges_on_live_idle_gpus() {
        let backend = TestBackend::new(180, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.header.chat_source = "api".to_string();
        app.header.model = "gpt-5.4-mini".to_string();
        app.header.base_model = "gpt-5.4-mini".to_string();
        app.header.gpu_cards = vec![
            super::super::GpuCardState {
                index: 0,
                name: "RTX A4500".to_string(),
                used_mb: 0,
                total_mb: 20_480,
                utilization: 0,
                allocations: vec![],
            },
            super::super::GpuCardState {
                index: 1,
                name: "RTX A4500".to_string(),
                used_mb: 0,
                total_mb: 20_480,
                utilization: 0,
                allocations: vec![],
            },
            super::super::GpuCardState {
                index: 2,
                name: "RTX A4500".to_string(),
                used_mb: 0,
                total_mb: 20_480,
                utilization: 0,
                allocations: vec![],
            },
        ];
        app.header.gpu_target_cards = vec![super::super::GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 6_700,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![
                super::super::GpuModelUsage {
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    short_label: "embed".to_string(),
                    used_mb: 1_100,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    short_label: "stt".to_string(),
                    used_mb: 4_200,
                },
                super::super::GpuModelUsage {
                    model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                    short_label: "tts".to_string(),
                    used_mb: 1_400,
                },
            ],
        }];

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("GPU0 embed 0/1100M"), "{text}");
        assert!(text.contains("GPU1 idle"), "{text}");
        assert!(text.contains("GPU2 idle"), "{text}");
    }

    #[test]
    fn settings_snapshot_shows_effective_cache_type_in_plan() {
        let mut app = test_app();
        app.chat_preset_bundle = Some(test_bundle("openai/gpt-oss-20b", Some("f8e4m3")));

        let text = settings_snapshot_text(&app, 80, 40);

        assert!(text.contains("cache    f8e4m3"), "{text}");
        assert!(
            text.contains("• Quality Q4_K_M f8e4m3 16k 92 tok/s"),
            "{text}"
        );
    }

    #[test]
    fn settings_snapshot_uses_off_when_plan_disables_paged_attention() {
        let mut app = test_app();
        let mut bundle = test_bundle("Qwen/Qwen3.6-35B-A3B", None);
        bundle.selected_plan.paged_attn = "off".to_string();
        bundle.plans[0].paged_attn = "off".to_string();
        bundle.plans[1].paged_attn = "off".to_string();
        app.chat_preset_bundle = Some(bundle);

        let text = settings_snapshot_text(&app, 80, 40);

        assert!(text.contains("cache    off"), "{text}");
    }

    #[test]
    fn skills_view_shows_skill_details_and_resources() {
        let backend = TestBackend::new(140, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.page = Page::Skills;
        app.skill_catalog = vec![super::super::SkillCatalogEntry {
            name: "service-deployment".to_string(),
            class: super::super::SkillClass::CtoxCore,
            state: super::super::SkillState::Stable,
            cluster: "host_ops".to_string(),
            skill_path: PathBuf::from("/tmp/service-deployment/SKILL.md"),
            description: "Use when CTOX needs to install, configure, start and verify software."
                .to_string(),
            helper_tools: vec!["deployment_bootstrap.py".to_string()],
            resources: vec![
                "references: deployment-rules.md, install-patterns.md".to_string(),
                "agents: openai.yaml".to_string(),
            ],
        }];
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Skills"), "{text}");
        assert!(text.contains("service-deployment"), "{text}");
        assert!(text.contains("deployment_bootstrap.py"), "{text}");
        assert!(text.contains("install-patterns.md"), "{text}");
    }

    #[test]
    fn skills_view_keeps_selected_skill_inside_visible_window() {
        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.page = Page::Skills;
        app.skill_catalog = (0..30)
            .map(|index| super::super::SkillCatalogEntry {
                name: format!("skill-{index:02}"),
                class: super::super::SkillClass::InstalledPacks,
                state: super::super::SkillState::Stable,
                cluster: String::new(),
                skill_path: PathBuf::from(format!("/tmp/skill-{index:02}/SKILL.md")),
                description: format!("Description for skill-{index:02}."),
                helper_tools: vec![],
                resources: vec![],
            })
            .collect();
        app.skills_selected = 20;

        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("skill-20"), "{text}");
        assert!(!text.contains("skill-00"), "{text}");
        assert!(!text.contains("skill-29"), "{text}");
    }

    #[test]
    fn secret_details_text_masks_stored_and_draft_values() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.settings_view = SettingsView::Secrets;
        app.secret_items = vec![super::super::SecretItem {
            scope: "credentials".to_string(),
            name: "OPENAI_API_KEY".to_string(),
            description: Some("test secret".to_string()),
            metadata: serde_json::json!({}),
            created_at: "2026-04-19T12:00:00Z".to_string(),
            updated_at: "2026-04-19T12:00:01Z".to_string(),
            value: "sk-draft-secret".to_string(),
            saved_value: "sk-stored-secret".to_string(),
        }];

        let text = secret_details_text(&app, 80, 30);

        assert!(text.contains("stored value (masked)"), "{text}");
        assert!(text.contains("draft value (masked)"), "{text}");
        assert!(!text.contains("sk-stored-secret"), "{text}");
        assert!(!text.contains("sk-draft-secret"), "{text}");
    }

    #[test]
    fn settings_snapshot_reports_empty_view_for_hidden_selection() {
        let mut app = test_app();
        app.page = Page::Settings;
        app.settings_view = SettingsView::Paths;
        app.settings_selected = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTO_JAMI_PROFILE_NAME")
            .unwrap();

        let text = settings_snapshot_text(&app, 80, 20);

        assert_eq!(text, "No settings are available in this view.");
    }

    #[test]
    fn update_install_summary_text_formats_version_info_human_readably() {
        let raw = serde_json::json!({
            "version": "0.3.6-11-g965770b-dirty",
            "install_mode": "managed",
            "workspace_root": "/Users/test/.local/lib/ctox/current",
            "active_root": "/Users/test/.local/lib/ctox/current",
            "state_root": "/Users/test/.local/state/ctox",
            "cache_root": "/Users/test/.cache/ctox",
            "current_release": "v0.3.6-11-g965770b-dirty",
            "previous_release": "v0.3.6-10-g22560b0-dirty",
            "release_channel": {
                "kind": "github",
                "repo": "metric-space-ai/ctox"
            }
        })
        .to_string();

        let text = update_install_summary_text(&raw);

        assert!(
            text.contains("version         0.3.6-11-g965770b-dirty"),
            "{text}"
        );
        assert!(text.contains("install mode    managed"), "{text}");
        assert!(
            text.contains("repo            metric-space-ai/ctox"),
            "{text}"
        );
        assert!(!text.contains("\"version\""), "{text}");
    }

    #[test]
    fn update_remote_summary_text_formats_remote_check_human_readably() {
        let raw = serde_json::json!({
            "action": "update check",
            "update_available": true,
            "current_version": "0.3.6",
            "latest_version": "0.3.7",
            "reason": "new release available"
        })
        .to_string();

        let text = update_remote_summary_text(&raw);

        assert!(text.contains("action          update check"), "{text}");
        assert!(text.contains("status          update available"), "{text}");
        assert!(text.contains("current         0.3.6"), "{text}");
        assert!(text.contains("latest          0.3.7"), "{text}");
        assert!(!text.contains("\"latest_version\""), "{text}");
    }
}
