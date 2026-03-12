use ratatui::{
    prelude::*,
    widgets::{Block, Gauge, Padding, Paragraph, Wrap},
};

use crate::{
    error::{Severity, BACKLOG},
    state::ServerState,
    util,
};

enum ScrollMode {
    Follow,
    Scroll(usize), // scroll offset from the end, in lines
}

pub struct TuiState {
    title: String,
    scroll_mode: ScrollMode,
}
impl TuiState {
    fn new(title: String) -> Self {
        Self {
            title,
            scroll_mode: ScrollMode::Follow,
        }
    }

    fn get_scroll_offset(&self) -> usize {
        match self.scroll_mode {
            ScrollMode::Follow => 0,
            ScrollMode::Scroll(offset) => offset,
        }
    }

    pub fn scroll(&mut self, mut amount: isize) {
        match self.scroll_mode {
            ScrollMode::Follow => {
                if amount < 0 {
                    amount = 0;
                }
                self.scroll_mode = ScrollMode::Scroll(amount as usize);
            }
            ScrollMode::Scroll(ref mut offset) => {
                // will floor to 0
                *offset = offset.saturating_add_signed(amount);
            }
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_mode = ScrollMode::Follow;
    }
}

pub trait Tui {
    fn render(&mut self, frame: &mut Frame, server_state: &ServerState);
}

pub struct LoginTui {
    pub state: TuiState,
}
impl Default for LoginTui {
    fn default() -> Self {
        Self {
            state: TuiState::new(format!(
                " RustyFusion v{} Login Server ",
                env!("CARGO_PKG_VERSION")
            )),
        }
    }
}
impl Tui for LoginTui {
    fn render(&mut self, frame: &mut Frame, server_state: &ServerState) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
            .split(frame.area());

        let area = frame.area();
        let log_widget = get_log_widget(&self.state, area.width, area.height);
        frame.render_widget(log_widget, layout[0]);

        let server_state = server_state.as_login();

        let title2 = Line::from(" Shards ").bold().centered();
        let block2 = Block::bordered()
            .padding(Padding::horizontal(1))
            .title(title2);

        let mut shard_ids = server_state.get_shard_ids();
        // fill in any gaps
        if !shard_ids.is_empty() {
            let max = *shard_ids.iter().max().unwrap();
            for sid in 1..=max {
                if !shard_ids.contains(&sid) {
                    shard_ids.push(sid);
                }
            }
        } else {
            shard_ids.push(1);
        }
        shard_ids.sort();

        let gauges: Vec<Gauge> = shard_ids
            .iter()
            .map(|sid| {
                let Some((current, max)) = server_state.get_current_and_max_pop_for_shard(*sid)
                else {
                    return Gauge::default()
                        .block(Block::bordered().title(format!("[#{}]", sid)))
                        .gauge_style(
                            Style::default()
                                .fg(Color::DarkGray)
                                .bg(Color::Black)
                                .add_modifier(Modifier::BOLD),
                        )
                        .ratio(0.0)
                        .label("offline");
                };

                let ratio = if max == 0 {
                    0.0
                } else {
                    current as f64 / max as f64
                };
                let color = if ratio > 1.0 {
                    Color::Red
                } else if ratio >= 0.5 {
                    Color::Yellow
                } else {
                    Color::Green
                };
                Gauge::default()
                    .block(Block::bordered().title(format!(
                        "[#{}] {}",
                        sid,
                        server_state.get_shard_name(*sid).unwrap_or("")
                    )))
                    .gauge_style(
                        Style::default()
                            .fg(color)
                            .bg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    )
                    .ratio(if ratio > 1.0 { 1.0 } else { ratio })
                    .label(format!("{} / {}", current, max))
            })
            .collect();
        for (i, gauge) in gauges.iter().enumerate() {
            let area = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    shard_ids
                        .iter()
                        .map(|_| Constraint::Length(3))
                        .collect::<Vec<Constraint>>(),
                )
                .split(block2.inner(layout[1]))[i];
            frame.render_widget(gauge.clone(), area);
        }
        frame.render_widget(block2, layout[1]);
    }
}

pub struct ShardTui {
    pub state: TuiState,
}
impl Default for ShardTui {
    fn default() -> Self {
        Self {
            state: TuiState::new(format!(
                " RustyFusion v{} Shard Server ",
                env!("CARGO_PKG_VERSION")
            )),
        }
    }
}
impl Tui for ShardTui {
    fn render(&mut self, frame: &mut Frame, server_state: &ServerState) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
            .split(frame.area());

        let area = frame.area();
        let log_widget = get_log_widget(&self.state, area.width, area.height);
        frame.render_widget(log_widget, layout[0]);

        let server_state = server_state.as_shard();

        let title2 = Line::from(" Players ").bold().centered();
        let block2 = Block::bordered()
            .padding(Padding::horizontal(1))
            .title(title2);

        let player_ids = server_state
            .entity_map
            .get_player_ids()
            .collect::<Vec<i32>>();
        let player_names: Vec<Line> = player_ids
            .iter()
            .map(|pid| {
                let player = server_state.get_player(*pid).unwrap();
                Line::from(format!("{}", player))
            })
            .collect();
        for (i, gauge) in player_names.iter().enumerate() {
            let area = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    player_ids
                        .iter()
                        .map(|_| Constraint::Length(3))
                        .collect::<Vec<Constraint>>(),
                )
                .split(block2.inner(layout[1]))[i];
            frame.render_widget(gauge.clone(), area);
        }
        frame.render_widget(block2, layout[1]);
    }
}

fn get_log_widget(state: &'_ TuiState, width: u16, height: u16) -> Paragraph<'_> {
    let title = Line::from(state.title.as_str())
        .light_red()
        .bold()
        .centered();
    let footer = Line::from(" Press CTRL+C to stop the server ").centered();
    let events = BACKLOG.get().unwrap().lock().unwrap();
    let lines: Vec<Line> = events
        .iter()
        .map(|fe| {
            let ts = util::get_timestamp_str(fe.get_timestamp());
            let text = fe.get_msg().to_string();
            let severity = fe.get_severity();
            let sev_span = Span::from(format!("[{}] ", severity));
            Line::from(vec![
                Span::from(format!("[{}] ", ts)).dark_gray(),
                match severity {
                    Severity::Info => sev_span.green(),
                    Severity::Warning => sev_span.yellow(),
                    Severity::Fatal => sev_span.red(),
                    Severity::Debug => sev_span.cyan(),
                },
                Span::from(text).white(),
            ])
        })
        .collect();

    let mut block = Block::bordered()
        .padding(Padding::horizontal(1))
        .title(title)
        .title_bottom(footer);

    if let ScrollMode::Scroll(offset) = state.scroll_mode {
        let scroll_title = Line::from(format!(" Scrolling ({} / {}) ", offset, events.len()))
            .yellow()
            .bold()
            .right_aligned();
        block = block.title_top(scroll_title);
    }

    let pg = Paragraph::new(lines)
        .block(block)
        .left_aligned()
        .wrap(Wrap { trim: true });
    let lines_to_scroll = pg
        .line_count(width)
        .saturating_sub(height as usize)
        .saturating_sub(state.get_scroll_offset());
    let pg = pg.scroll((lines_to_scroll as u16, 0));
    pg
}
