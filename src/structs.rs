use std::{
    collections::HashMap,
};
use serde::Serialize;

pub enum DataType {
    Json,
    Urlencoded
}

#[derive(Debug, Clone, PartialEq, Serialize, Copy)]
pub enum InjectionPlace {
    Path,
    Body,
    Headers,
    HeaderValue
}

#[derive(Debug, Clone)]
pub struct Config {
    //default url without any changes (except from when used from request file, maybe change this logic TODO)
    pub url: String,

    //user supplied wordlist file
    pub wordlist: String,

    //proxy server with schema or http:// by default.
    pub proxy: String,

    //file to output
    pub output_file: String,
    //whether to append to the output file or overwrite
    pub append: bool,

    //output format for file & stdout outputs
    pub output_format: String,

    //a directory for saving request & responses with found parameters
    pub save_responses: String,

    //ignore errors like 'Page is too huge'
    pub force: bool,

    //only report parameteres with different "diffs"
    //in case a few parameters change the same part of a page - only one of them will be saved
    pub strict: bool,

    //custom parameters to check like <admin, [true, 1, false, ..]>
    pub custom_parameters: HashMap<String, Vec<String>>,
    pub disable_custom_parameters: bool,

    //disable progress bar for high verbosity
    pub disable_progress_bar: bool,

    //proxy to resend requests with found parameter
    pub replay_proxy: String,
    //whether to resend the request once with all parameters or once per every parameter
    pub replay_once: bool,

    //print request & response and exit.
    //Can be useful for checking whether the program parsed the input parameters successfully
    pub test: bool,

    //0 - print only critical errors and output
    //1 - print intermediate results and progress bar
    //2 - print all response changes
    pub verbose: usize,

    //determines how much learning requests should be made on the start
    //doesn't include first two requests made for cookies and initial response
    pub learn_requests_count: usize,

    //check the same list of parameters with the found parameters until there are no new parameters to be found.
    //conflicts with --verify for now. Will be changed in the future.
    pub recursion_depth: usize,

    //amount of concurrent requests
    pub concurrency: usize,

    //http request timeout in seconds
    pub timeout: usize,

    //whether the verify found parameters one time more.
    //in future - check for _false_potives like when every parameter that starts with _ is found
    pub verify: bool,

    //check only for reflected parameters in order to decrease the amount of requests
    //usually makes 2+learn_request_count+words/max requests
    //but in rare cases its number may be higher
    pub reflected_only: bool,

    //http version. 1.1 or 2
    //TODO replace with enum
    pub http: String,

    pub follow_redirects: bool,
}

#[derive(Debug, Default)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}