pub mod client;
pub mod config;
pub mod model;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
