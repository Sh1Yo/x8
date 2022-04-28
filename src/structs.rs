use std::{collections::HashMap, time::Duration};

pub trait DefaultResponse {
    fn default() -> ResponseData;
}

#[derive(Debug)]
pub struct ResponseData {
    pub text: String,
    pub code: u16,
    pub reflected_params: HashMap<String, usize>,
}

impl DefaultResponse for ResponseData {
    fn default() -> ResponseData {
        ResponseData {
            text: String::new(),
            code: 0u16,
            reflected_params: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FuturesData {
    pub remaining_params: Vec<String>,
    pub found_params: HashMap<String, String>,
    pub stats: Statistic
}

#[derive(Debug, Clone)]
pub struct Config {
    pub method: String,
    pub initial_url: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub wordlist: String,
    pub parameter_template: String,
    pub custom_parameters: HashMap<String, Vec<String>>,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub body_type: String,
    pub proxy: String,
    pub output_file: String,
    pub output_format: String,
    pub save_responses: String,
    pub force: bool,
    pub disable_response_correction: bool,
    pub disable_custom_parameters: bool,
    pub disable_progress_bar: bool,
    pub replay_once: bool,
    pub replay_proxy: String,
    pub follow_redirects: bool,
    pub encode: bool,
    pub test: bool,
    pub append: bool,
    pub as_body: bool,
    pub headers_discovery: bool,
    pub within_headers: bool,
    pub verbose: usize,
    pub is_json: bool,
    pub disable_cachebuster: bool,
    pub delay: Duration,
    pub value_size: usize,
    pub learn_requests_count: usize,
    pub max: usize,
    pub concurrency: usize,
    pub verify: bool,
    pub reflected_only: bool
}

#[derive(Debug)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}

#[derive(Debug, Clone)]
pub struct Statistic {
    pub amount_of_requests: usize
}

impl Statistic {
    pub fn merge(&mut self, stats: Statistic) {
        self.amount_of_requests += stats.amount_of_requests;
    }
}