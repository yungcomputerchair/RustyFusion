#[macro_use]
extern crate num_derive;

use std::{error::Error, result};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub const CN_PACKET_BUFFER_SIZE: usize = 4096;

pub mod error;
pub mod net;
pub mod util;
