use serde::{Deserialize, Serialize};

pub struct Sheet {
  mapping: [crate::Point; 4],
  width: u32,
  length: u32,
}

pub struct Join {}

pub struct Navigation {
  sheets: Vec<Sheet>,
}
