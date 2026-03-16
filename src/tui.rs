use std::time::{Duration, Instant};

use ratatui::{
    prelude::*,
    widgets::{Block, Gauge, Padding, Paragraph, Wrap},
};

use crate::{
    config::config_get,
    error::{Severity, BACKLOG},
    state::{LoginServerState, ServerState, ShardServerState},
    util,
};

const STATS_CACHE_INTERVAL: Duration = Duration::from_secs(1);

struct ShardStatsCache {
    last_updated: Instant,
    total_instance_count: usize,
    base_instance_count: usize,
    total_chunk_count: usize,
    loaded_chunk_count: usize,
    loaded_entity_count: usize,
    tickable_entity_count: usize,
}
impl ShardStatsCache {
    fn new() -> Self {
        Self {
            last_updated: Instant::now() - STATS_CACHE_INTERVAL,
            total_instance_count: 0,
            base_instance_count: 0,
            total_chunk_count: 0,
            loaded_chunk_count: 0,
            loaded_entity_count: 0,
            tickable_entity_count: 0,
        }
    }

    fn refresh_if_needed(&mut self, shard_state: &ShardServerState) {
        if self.last_updated.elapsed() >= STATS_CACHE_INTERVAL {
            self.total_instance_count = shard_state.entity_map.get_num_instances();
            self.base_instance_count = shard_state.entity_map.get_num_base_instances();
            self.total_chunk_count = shard_state.entity_map.get_num_chunks();
            self.loaded_chunk_count = shard_state.entity_map.get_num_loaded_chunks();
            self.loaded_entity_count = shard_state.entity_map.get_num_loaded_entities();
            self.tickable_entity_count = shard_state.entity_map.get_tickable_ids().count();
            self.last_updated = Instant::now();
        }
    }
}

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
            .constraints([Constraint::Percentage(70), Constraint::Fill(1)].as_ref())
            .split(frame.area());

        let log_widget = LogWidget { state: &self.state };
        frame.render_widget(log_widget, layout[0]);

        let server_state = server_state.as_login();
        let shard_list_widget = ShardListWidget {
            login_state: server_state,
        };
        frame.render_widget(shard_list_widget, layout[1]);
    }
}

struct ShardListWidget<'a> {
    login_state: &'a LoginServerState,
}
impl<'a> Widget for ShardListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" Shards ").bold().centered();
        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .title(title);

        let mut shard_ids = self.login_state.get_reserved_shard_ids();
        shard_ids.sort();

        let gauges: Vec<Gauge> = shard_ids
            .iter()
            .map(|sid| {
                let Some((current, max)) = self.login_state.get_current_and_max_pop_for_shard(*sid)
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

                let mut block = Block::bordered().title(format!(
                    "[#{}] {} ",
                    sid,
                    self.login_state.get_shard_public_addr(*sid).unwrap()
                ));

                if let Some(city) = self.login_state.get_shard_city(*sid) {
                    block = block.title(Line::from(format!(" {} ", city)).right_aligned());
                }

                Gauge::default()
                    .block(block)
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

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                shard_ids
                    .iter()
                    .map(|_| Constraint::Length(3))
                    .collect::<Vec<Constraint>>(),
            )
            .split(block.inner(area));

        for (i, gauge) in gauges.iter().enumerate() {
            gauge.render(areas[i], buf);
        }
        block.render(area, buf);
    }
}

pub struct ShardTui {
    pub state: TuiState,
    stats_cache: ShardStatsCache,
}
impl Default for ShardTui {
    fn default() -> Self {
        Self {
            state: TuiState::new(format!(
                " RustyFusion v{} Shard Server ",
                env!("CARGO_PKG_VERSION")
            )),
            stats_cache: ShardStatsCache::new(),
        }
    }
}
impl Tui for ShardTui {
    fn render(&mut self, frame: &mut Frame, server_state: &ServerState) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Fill(1)].as_ref())
            .split(frame.area());

        let inner_layout_left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Fill(1)].as_ref())
            .split(outer_layout[0]);

        let log_widget = LogWidget { state: &self.state };
        frame.render_widget(log_widget, inner_layout_left[0]);

        let server_state = server_state.as_shard();
        let player_list_widget = PlayerListWidget {
            shard_state: server_state,
        };
        frame.render_widget(player_list_widget, outer_layout[1]);

        self.stats_cache.refresh_if_needed(server_state);
        let shard_stats_widget = ShardStatsWidget {
            shard_state: server_state,
            stats_cache: &self.stats_cache,
        };
        frame.render_widget(shard_stats_widget, inner_layout_left[1]);
    }
}

struct LogWidget<'a> {
    state: &'a TuiState,
}
impl<'a> Widget for LogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(self.state.title.as_str())
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

        if let ScrollMode::Scroll(offset) = self.state.scroll_mode {
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
            .line_count(area.width)
            .saturating_sub(area.height as usize)
            .saturating_sub(self.state.get_scroll_offset());

        let pg = pg.scroll((lines_to_scroll as u16, 0));
        pg.render(area, buf);
    }
}

struct PlayerListWidget<'a> {
    shard_state: &'a ShardServerState,
}
impl<'a> Widget for PlayerListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" Players ").bold().centered();
        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .title(title);

        let player_ids = self
            .shard_state
            .entity_map
            .get_player_ids()
            .collect::<Vec<i32>>();

        let player_names: Vec<Line> = player_ids
            .iter()
            .map(|pid| {
                let player = self.shard_state.get_player(*pid).unwrap();
                Line::from(format!("{}", player))
            })
            .collect();

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                player_ids
                    .iter()
                    .map(|_| Constraint::Length(3))
                    .collect::<Vec<Constraint>>(),
            )
            .split(block.inner(area));

        for (i, line) in player_names.iter().enumerate() {
            line.render(areas[i], buf);
        }
        block.render(area, buf);
    }
}

struct ShardStatsWidget<'a> {
    shard_state: &'a ShardServerState,
    stats_cache: &'a ShardStatsCache,
}
impl<'a> Widget for ShardStatsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let footer = Line::from(" Stats ").bold().centered();
        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .title_bottom(footer);

        let config = config_get();

        let current_pop = self.shard_state.entity_map.get_player_ids().count();
        let max_pop = config.shard.max_channel_pop.get() * config.shard.num_channels.get() as usize;

        let cache = self.stats_cache;

        let stats_lines = [
            if let Some(uuid) = self.shard_state.login_server_conn_id {
                Line::from(format!("Login server connected ({})", uuid)).green()
            } else {
                Line::from("Login server disconnected").red()
            },
            Line::from(format!("Population: {} / {}", current_pop, max_pop)),
            Line::from(format!(
                "Instances: {} (base) + {} (transient)",
                cache.base_instance_count,
                cache.total_instance_count - cache.base_instance_count
            )),
            Line::from(format!(
                "Chunks: {} loaded / {} total",
                cache.loaded_chunk_count, cache.total_chunk_count
            )),
            Line::from(format!(
                "Entities: {} ticking, {} chunk-loaded",
                cache.tickable_entity_count, cache.loaded_entity_count
            )),
        ];

        let inner = block.inner(area);
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                stats_lines
                    .iter()
                    .map(|_| Constraint::Length(1))
                    .collect::<Vec<Constraint>>(),
            )
            .split(inner);

        for (i, line) in stats_lines.iter().enumerate() {
            line.render(areas[i], buf);
        }
        block.render(area, buf);
    }
}
