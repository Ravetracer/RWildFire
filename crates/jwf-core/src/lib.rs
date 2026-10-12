//! Core of the JWildfire Rust port: flame model, file IO, variations and the CPU renderer.

pub mod gradient;
pub mod io;
pub mod math;
pub mod model;
pub mod random;
pub mod render;
pub mod rng;
pub mod variations;

pub use gradient::Gradient;
pub use model::{Affine, ColorType, DrawMode, Flame, Layer, VariationInstance, XForm};
