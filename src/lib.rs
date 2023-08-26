use std::{result, error::Error};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub mod net;
