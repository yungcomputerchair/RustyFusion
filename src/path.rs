use crate::{defines::SHARD_TICKS_PER_SECOND, Position};

#[derive(Debug, Copy, Clone)]
pub struct PathPoint {
    pub pos: Position,
    pub speed: i32, // from previous point
    pub stop_ticks: usize,
}
impl PartialEq for PathPoint {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}
impl Eq for PathPoint {}

#[derive(Debug, Clone)]
pub enum PathState {
    Pending,
    Moving,
    Waiting(usize),
    Done,
}

#[derive(Debug, Clone)]
pub struct Path {
    points: Vec<PathPoint>,
    cycle: bool,
    idx: usize,
    state: PathState,
}
impl Path {
    pub fn new(points: Vec<PathPoint>, cycle: bool) -> Self {
        Self {
            points,
            cycle,
            idx: 0,
            state: PathState::Pending,
        }
    }

    pub fn get_points(&self) -> &[PathPoint] {
        &self.points
    }

    pub fn get_total_length(&self) -> u32 {
        let mut total_length = 0;
        for i in 0..self.points.len() - 1 {
            total_length += self.points[i].pos.distance_to(&self.points[i + 1].pos);
        }
        if self.cycle {
            total_length += self
                .points
                .last()
                .unwrap()
                .pos
                .distance_to(&self.points[0].pos);
        }
        total_length
    }

    pub fn get_target_pos(&self) -> Position {
        self.points[self.idx].pos
    }

    pub fn get_speed(&self) -> i32 {
        match self.state {
            PathState::Moving => self.points[self.idx].speed,
            _ => 0,
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, PathState::Done)
    }

    pub fn is_waiting(&self) -> bool {
        matches!(self.state, PathState::Waiting(_))
    }

    pub fn advance(&mut self) {
        self.idx += 1;
        if self.idx == self.points.len() {
            if self.cycle {
                self.idx = 0;
            } else {
                self.idx -= 1; // hold last point as target
                self.state = PathState::Done;
            }
        }
    }

    pub fn tick(&mut self, pos: &mut Position) -> bool {
        match self.state {
            PathState::Pending => {
                self.state = PathState::Moving;
            }
            PathState::Moving => {
                let dist = self.points[self.idx].speed as f32 / SHARD_TICKS_PER_SECOND as f32;
                let target_point = self.points[self.idx];
                let target_pos = target_point.pos;
                let source_pos = *pos;
                let (new_pos, snap) = source_pos.interpolate(&target_pos, dist);
                *pos = new_pos;
                if snap {
                    // reached target
                    if target_point.stop_ticks > 0 {
                        self.state =
                            PathState::Waiting(target_point.stop_ticks * SHARD_TICKS_PER_SECOND);
                    } else {
                        self.advance();
                        return true;
                    }
                }
            }
            PathState::Waiting(ticks_left) => {
                if ticks_left == 1 {
                    self.state = PathState::Moving;
                    self.advance();
                } else {
                    self.state = PathState::Waiting(ticks_left - 1);
                }
            }
            PathState::Done => {}
        };
        false
    }
}
