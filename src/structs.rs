use std::{
    collections::HashMap, time::Duration,
};
use regex::Regex;
use serde::Serialize;
use lazy_static::lazy_static;
use crate::utils::random_line;

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Json,
    Urlencoded,

    //that's from parsed request's content-type header
    //needs to be ignored in case the injection points not within the body
    //to exclude false positive /?{"ZXxZPLN":"ons9XDZ", ..} or Cookie: {"ZXxZPLN":"ons9XDZ", ..} queries
    //it still can be bypassed with the --data-type argument
    ProbablyJson,
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
    pub urls: Vec<String>,

    //a list of methods to check parameters with
    pub methods: Vec<String>,

    //custom user supplied headers or default ones
    pub custom_headers: Vec<(String, String)>,

    //how much to sleep between requests in millisecs
    pub delay: Duration,

    //user supplied wordlist file
    pub wordlist: String,

    //max amount of parameters to send per request. Can be specified by user otherwise detects automatically based on request method
    pub max: Option<usize>,

    //parameter template, for example {k}={v}
    pub template: Option<String>,

    //how to join parameters, for example '&'
    pub joiner: Option<String>,

    //whether to encode the query like param1=value1&param2=value2 -> param1%3dvalue1%26param2%3dvalue2
    pub encode: bool,

    //default body
    pub body: String,

    //Json type handles differently because values like null, true, ints needs to be sent without quotes
    pub data_type: Option<DataType>,

    //whether to include parameters like debug=true to the list
    pub disable_custom_parameters: bool,

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

    //amount of concurrent requests per url
    pub concurrency: usize,

    //amount of concurrent url checks
    pub threads: usize,

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

    //by default parameters are sent within the body only in case PUT or POST methods are used.
    //it's possible to overwrite this behaviour by specifying this option
    pub invert: bool,

    //true in case the injection points is within the header or the headers are injection point itself
    pub headers_discovery: bool,

    pub follow_redirects: bool,
}

#[derive(Debug, Default)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}

pub enum ParamPatterns {
    //_anything
    SpecialPrefix(char),

    //anything1243124
    //from example: (string = anything, usize=7)
    HasNumbersPostfix(String, usize),

    //any!thing
    ContainsSpecial(char),

    //password_anything
    BeforeUnderscore(String),

    //anything_password
    AfterUnderscore(String),

    //password-anything
    BeforeDash(String),

    //anything-password
    AfterDash(String)
}

impl ParamPatterns {

    /// returns check parameter to determine whether the prediction is correct
    pub fn turn_into_string(self) -> String {
        match self {
            ParamPatterns::SpecialPrefix(c) => format!("{}anything", c),
            ParamPatterns::ContainsSpecial(c) => format!("anyth{}ng", c),
            ParamPatterns::BeforeUnderscore(s) => format!("{}_anything", s),
            ParamPatterns::AfterUnderscore(s) => format!("anything_{}", s),
            ParamPatterns::BeforeDash(s) => format!("{}-anything", s),
            ParamPatterns::AfterDash(s) => format!("anything-{}", s),
            ParamPatterns::HasNumbersPostfix(s, u) => format!("{}{}", s, "1".repeat(u)),
        }
    }

    // in case 2 patterns match like sth1-sth2 == check all the patterns.
    // In case nothing confirms -- leave sth1-sth2
    // In case all confirms ¯\_(ツ)_/¯
    pub fn get_patterns(param: &str) -> Vec<Self> {
        lazy_static! {
            static ref RE_NUMBER_PREFIX: Regex = Regex::new(r"^([^\d]+)(\d+)$").unwrap();
        };

        let mut patterns = Vec::new();
        let param_chars: Vec<char> = param.chars().collect();

        if param_chars[0].is_ascii_punctuation() {
            patterns.push(ParamPatterns::SpecialPrefix(param_chars[0]))
        }

        let special_chars: Vec<&char> = param_chars.iter().filter(|x| x.is_ascii_punctuation() && x != &&'-' && x != &&'_').collect();
        if special_chars.len() == 1 {
            patterns.push(ParamPatterns::ContainsSpecial(*special_chars[0]));
        }

        if param_chars.contains(&'-') {
            //we're treating as if there's only one '-' for now.
            //maybe needs to be changed in future
            let mut splitted = param.split("-");

            patterns.push(ParamPatterns::BeforeDash(splitted.next().unwrap().to_string()));
            patterns.push(ParamPatterns::AfterDash(splitted.next().unwrap().to_string()));
        }

        if param_chars.contains(&'_') {
            //we're treating as if there's only one '_' for now.
            //maybe needs to be changed in future
            let mut splitted = param.split("_");

            patterns.push(ParamPatterns::BeforeUnderscore(splitted.next().unwrap().to_string()));
            patterns.push(ParamPatterns::AfterUnderscore(splitted.next().unwrap().to_string()));
        }

        if let Some(caps) = RE_NUMBER_PREFIX.captures(param) {
            let (word, digits) = (caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str());
            patterns.push(ParamPatterns::HasNumbersPostfix(word.to_string(), digits.len()))
        }

        patterns
    }
}