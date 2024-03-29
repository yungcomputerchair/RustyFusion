#![allow(clippy::derivable_impls)]

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

pub mod chunk;
pub mod entity;
pub mod item;
pub mod mission;
pub mod nano;
pub mod path;
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

    pub fn get_unstuck(&self) -> Position {
        const UNSTICK_XY_RANGE: i32 = 200;
        const UNSTICK_Z_BUMP: i32 = 80;
        Position {
            x: self.x + util::rand_range_inclusive(-UNSTICK_XY_RANGE, UNSTICK_XY_RANGE),
            y: self.y + util::rand_range_inclusive(-UNSTICK_XY_RANGE, UNSTICK_XY_RANGE),
            z: self.z + UNSTICK_Z_BUMP,
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
