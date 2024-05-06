#![allow(clippy::derivable_impls)]

use std::ops::Add;

use vecmath::{vec3_add, vec3_len, vec3_scale, vec3_sub, Vector3};

#[macro_export]
macro_rules! unused {
    () => {
        Default::default()
    };
}

#[macro_export]
macro_rules! placeholder {
    ($val:expr) => {{
        #[cfg(debug_assertions)]
        println!("PLACEHOLDER: {} line {}", file!(), line!());
        $val
    }};
}

pub mod defines;
pub mod enums;
pub mod error;
pub mod helpers;
pub mod net;
pub mod state;
pub mod timer;
pub mod util;

pub mod config;
pub mod database;
pub mod tabledata;

pub mod ai;
pub mod chunk;
pub mod entity;
pub mod item;
pub mod mission;
pub mod nano;
pub mod path;
pub mod skills;
pub mod trade;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
impl Position {
    pub fn distance_to(&self, other: &Position) -> u32 {
        // scaling down for the multiplication helps to avoid overflow here
        const DIST_MATH_SCALE: f32 = 100.0;
        let dx = self.x.abs_diff(other.x) as f32 / DIST_MATH_SCALE;
        let dy = self.y.abs_diff(other.y) as f32 / DIST_MATH_SCALE;
        let dz = self.z.abs_diff(other.z) as f32 / DIST_MATH_SCALE;
        ((dx * dx + dy * dy + dz * dz).sqrt() * DIST_MATH_SCALE) as u32
    }

    pub fn angle_to(&self, other: &Position) -> f32 {
        let dx = (other.x - self.x) as f32;
        let dy = (other.y - self.y) as f32;
        dy.atan2(dx).to_degrees()
    }

    pub fn interpolate(&self, target: &Position, distance: f32) -> (Position, bool) {
        let source = (*self).into();
        let target = (*target).into();
        let delta = vec3_sub(target, source);
        let delta_len = vec3_len(delta);
        if delta_len <= distance {
            (target.into(), true)
        } else {
            let new_pos = vec3_add(source, vec3_scale(delta, distance / delta_len)).into();
            (new_pos, false)
        }
    }

    pub fn get_random_around(&self, x_radius: u32, y_radius: u32, z_radius: u32) -> Position {
        let x_radius = x_radius as i32;
        let y_radius = y_radius as i32;
        let z_radius = z_radius as i32;
        Position {
            x: self.x + util::rand_range_inclusive(-x_radius, x_radius),
            y: self.y + util::rand_range_inclusive(-y_radius, y_radius),
            z: self.z + util::rand_range_inclusive(-z_radius, z_radius),
        }
    }

    pub fn get_unstuck(&self) -> Position {
        const UNSTICK_XY_RANGE: u32 = 200;
        const UNSTICK_Z_BUMP: i32 = 80;
        let mut nudged = self.get_random_around(UNSTICK_XY_RANGE, UNSTICK_XY_RANGE, 0);
        nudged.z += UNSTICK_Z_BUMP;
        nudged
    }

    pub fn get_offset_by_polar_coords(&self, distance: u32, angle_deg: f32) -> Position {
        let angle_rad = angle_deg.to_radians();
        let x_offset = (angle_rad.cos() * distance as f32) as i32;
        let y_offset = (angle_rad.sin() * distance as f32) as i32;
        Position {
            x: self.x + x_offset,
            y: self.y + y_offset,
            z: self.z,
        }
    }
}
impl From<Vector3<f32>> for Position {
    fn from(value: Vector3<f32>) -> Self {
        Self {
            x: value[0] as i32,
            y: value[1] as i32,
            z: value[2] as i32,
        }
    }
}
impl From<Position> for Vector3<f32> {
    fn from(value: Position) -> Self {
        [value.x as f32, value.y as f32, value.z as f32]
    }
}
impl Add<Position> for Position {
    type Output = Position;
    fn add(self, other: Position) -> Position {
        Position {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}
