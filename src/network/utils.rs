use std::error::Error;

use lazy_static::lazy_static;
use percent_encoding::{AsciiSet, CONTROLS};
use serde::Serialize;

use crate::{config::structs::Config, utils::random_line};

use super::response::Response;

lazy_static! {
    /// characters to encode in case --encode option provided
    pub static ref FRAGMENT: AsciiSet = CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'&')
        .add(b'#')
        .add(b';')
        .add(b'/')
        .add(b'=')
        .add(b'%');
}

/// enum mainly created for the correct json parsing
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    /// we need a different data type for json because some json values can be used without quotes (numbers, booleans, ..)
    /// and therefore this type should be treated differently
    Json,
    Urlencoded,

    /// that's from parsed request's content-type header
    /// needs to be ignored in case the injection points not within the body
    /// to exclude false positive /?{"ZXxZPLN":"ons9XDZ", ..} or Cookie: {"ZXxZPLN":"ons9XDZ", ..} queries
    // it still can be bypassed with the correct --data-type argument
    ProbablyJson,
}

/// where to insert parameters
#[derive(Debug, Clone, PartialEq, Serialize, Copy, Default)]
pub enum InjectionPlace {
    #[default]
    Path,
    Body,
    Headers,
    HeaderValue,
}

pub trait Headers {
    fn contains_key(&self, key: &str) -> bool;
    fn get_value(&self, key: &str) -> Option<String>;
    fn get_value_case_insensitive(&self, key: &str) -> Option<String>;
}

impl Headers for Vec<(String, String)> {
    fn contains_key(&self, key: &str) -> bool {
        for (k, _) in self.iter() {
            if k == key {
                return true;
            }
        }
        false
    }

    fn get_value(&self, key: &str) -> Option<String> {
        for (k, v) in self.iter() {
            if k == key {
                return Some(v.to_owned());
            }
        }
        None
    }

    fn get_value_case_insensitive(&self, key: &str) -> Option<String> {
        let key = key.to_lowercase();
        for (k, v) in self.iter() {
            if k.to_lowercase() == key {
                return Some(v.to_owned());
            }
        }
        None
    }
}

/// writes request and response to a file
/// return file location
pub(super) fn save_request(
    config: &Config,
    response: &Response,
    param_key: &str,
) -> Result<String, Box<dyn Error>> {
    let output = response.print();

    let filename = format!(
        "{}/{}-{}-{}-{}",
        &config.save_responses,
        &response.request.as_ref().unwrap().defaults.host,
        response
            .request
            .as_ref()
            .unwrap()
            .defaults
            .method
            .to_lowercase(),
        param_key,
        random_line(3) //nonce to prevent overwrites
    );

    std::fs::write(&filename, output)?;

    Ok(filename)
}
