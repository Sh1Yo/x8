pub mod config;
pub mod diff;
pub mod network;
pub mod runner;
pub mod utils;

const RANDOM_CHARSET: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// To ignore pages with size > 25MB. Usually it's some binary things. Can be ignored with --force
const MAX_PAGE_SIZE: usize = 25 * 1024 * 1024;

const DEFAULT_PROGRESS_URL_MAX_LEN: usize = 36;

/// Default random value sizes
const VALUE_LENGTH: usize = 6;
const RANDOM_LENGTH: usize = 5;