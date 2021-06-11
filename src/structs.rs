use std::{collections::HashMap, time::Duration};

/*pub trait DefaultResponse {
    fn default() -> ResponseData;
}*/

#[derive(Debug)]
pub struct ResponseData {
    pub text: String,
    pub code: u16,
    pub reflected_params: Vec<String>,
}

/*impl DefaultResponse for ResponseData {
    fn default() -> ResponseData {
        ResponseData {
            text: String::new(),
            code: 0u16,
            reflected_params: vec![],
        }
    }
}*/

#[derive(Debug, Clone)]
pub struct Config {
    pub method: String,
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
    pub save_responses: String,
    pub tmp_directory: String,
    pub force: bool,
    pub disable_response_correction: bool,
    pub disable_custom_parameters: bool,
    pub disable_progress_bar: bool,
    pub replay_once: bool,
    pub replay_proxy: String,
    pub follow_redirects: bool,
    pub encode: bool,
    pub test: bool,
    pub as_body: bool,
    pub verbose: u8,
    pub is_json: bool,
    pub disable_cachebuster: bool,
    //pub verify: bool,
    pub delay: Duration,
    pub diff_location: String,
    pub external_diff: bool,
    pub value_size: usize,
    pub learn_requests_count: usize,
    pub max: usize
}

#[derive(Debug)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}