use std::{error::Error, collections::HashMap, io::{self, Write}, iter::FromIterator};

use colored::Colorize;
use itertools::Itertools;
use regex::Regex;
use lazy_static::lazy_static;

use crate::{structs::Config, diff::diff, utils::save_request, runner::found_parameters::ReasonKind};

use super::{request::Request, headers::Headers};

#[derive(Debug, Clone, Default)]
pub struct Response<'a> {
    //time from the sent request to response headers
    pub time: u128,

    pub code: u16,

    pub headers: Vec<(String, String)>,

    //headers + body
    pub text: String,

    //<parameter, amount of reflections>
    pub reflected_parameters: HashMap<String, usize>,

    //loooks like it's being used only as a cachebuster so far? TODO maybe I need to remove it or make an option of it at least?
    pub additional_parameter: String,

    //None only in initial_request due to some lifetime issues
    pub request: Option<Request<'a>>,

    //None only when the request failed
    pub http_version: Option<http::Version>,
}

//Owo
unsafe impl Send for Response<'_> {}

#[derive(PartialEq, Eq)]
pub enum Status {
    Ok,             //2xx
    Redirect,       //3xx
    UserFault,      //4xx
    ServerFault,    //5xx
    Other,
}

impl<'a> Response<'a> {

    /// count how many times we can see the string in the response
    pub fn count(&self, string: &str) -> usize {
        self.text.to_lowercase().matches(string).count()
    }

    /// calls check_diffs & returns code and found diffs
    pub fn compare(&self, initial_response: &'a Response<'a>, old_diffs: &Vec<String>) -> Result<(bool, Vec<String>), Box<dyn Error>> {

        let mut is_code_diff: bool = false;
        let mut diffs: Vec<String> = Vec::new();

        if initial_response.code != self.code {
            is_code_diff = true
        }

        //just push every found diff to the vector of diffs
        for diff in diff(
            &self.print(),
            &initial_response.print(),
        )? {
            if !diffs.contains(&diff) && !old_diffs.contains(&diff) {
                diffs.push(diff);
            } else if !old_diffs.contains(&diff) {
                let mut c = 1;
                while diffs.contains(&[&diff, "(", &c.to_string(), ")"].concat()) {
                    c += 1
                }
                diffs.push([&diff, " (", &c.to_string(), ")"].concat());
            }
        }

        diffs.sort();

        Ok((is_code_diff, diffs))
    }

    /// adds new lines where necessary in order to increase accuracy in diffing
    pub fn beautify_body(&mut self) {
        lazy_static! {
            static ref RE_JSON_WORDS_WITHOUT_QUOTES: Regex =
                Regex::new(r#"^(\d+|null|false|true)$"#).unwrap();
            static ref RE_JSON_BRACKETS: Regex =
                Regex::new(r#"(?P<bracket>(\{"|"\}|\[("|\d)|("|\d)\]))"#).unwrap();
            static ref RE_JSON_COMMA_AFTER_DIGIT: Regex =
                Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>\d+),"#).unwrap();
            static ref RE_JSON_COMMA_AFTER_BOOL: Regex =
                Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>(false|null|true)),"#).unwrap();
        }

        self.text
            = if (self.headers.contains_key("content-type") && self.headers.get_value_case_insensitive("content-type").unwrap().contains("json"))
            || (self.text.starts_with("{") && self.text.ends_with("}")) {
            let body = self.text
                                    .replace("\\\"", "'")
                                    .replace("\",", "\",\n");
            let body = RE_JSON_BRACKETS.replace_all(&body, "${bracket}\n");
            let body = RE_JSON_COMMA_AFTER_DIGIT.replace_all(&body, "$first:$second,\n");
            let body = RE_JSON_COMMA_AFTER_BOOL.replace_all(&body, "$first:$second,\n");

            body.to_string()
        } else {
            self.text.replace('>', ">\n")
        }
    }

    /// find parameters with the different amount of reflections and add them to self.reflected_parameters
    pub fn fill_reflected_parameters(&mut self, initial_response: &Response) {
        //let base_count = self.count(&self.request.prepared_parameters[additional_param]);

        //remove non random parameters from prepared parameters because they would cause false positives in this check
        let prepated_parameters: Vec<&(String, String)> = if !self.request.as_ref().unwrap().non_random_parameters.is_empty() {
            Vec::from_iter(
                self.request.as_ref().unwrap().prepared_parameters
                    .iter()
                    .filter(|x| !self.request.as_ref().unwrap().non_random_parameters.contains_key(&x.0))
            )
        } else {
            Vec::from_iter(
                self.request.as_ref().unwrap().prepared_parameters.iter()
            )
        };

        for (k, v) in prepated_parameters.iter() {
            let new_count = self.count(v) - initial_response.count(v);

            if self.request.as_ref().unwrap().defaults.amount_of_reflections != new_count {
                self.reflected_parameters.insert(k.to_string(), new_count);
            }
        }
    }

    /// returns parameters with different amount of reflections and tells whether we need to recheck the remaining parameters
    pub fn proceed_reflected_parameters(&self) -> (Option<&str>, bool) {

        if self.reflected_parameters.is_empty() {
            return (None, false)

         // only one reflected parameter - return it
        } else if self.reflected_parameters.len() == 1 {
            return (Some(self.reflected_parameters.keys().next().unwrap()), false)
        };

        // only one reflected parameter besides additional one - return it
        // this parameter caused other parameters to reflect different amount of times
        if self.request.as_ref().unwrap().prepared_parameters.len() == 2 && self.reflected_parameters.len() == 2 {
            return (Some(self.reflected_parameters.keys().filter(|x| x != &&self.additional_parameter).next().unwrap()), false)
        }

        //save parameters by their amount of reflections
        let mut parameters_by_reflections: HashMap<usize, Vec<&str>> = HashMap::new();

        for (k, v) in self.reflected_parameters.iter() {
            if parameters_by_reflections.contains_key(v) {
                parameters_by_reflections.get_mut(v).unwrap().push(k);
            } else {
                parameters_by_reflections.insert(*v, vec![k]);
            }
        }

        //try to find a parameter with different amount of reflections between all of them
        if parameters_by_reflections.len() == 2 {
            for (_, v) in parameters_by_reflections.iter() {
                if v.len() == 1 {
                    return (Some(v[0]), true)
                }
            }
        }

        // the reflections weren't stable. It's better to recheck the parameters
        (None, true)
    }

    /// adds headers to response text
    pub fn add_headers(&mut self) {
        let mut text = String::new();
        for (k, v) in self.headers.iter().sorted() {
            text += &format!("{}: {}\n", k, v);
        }

        self.text = text + "\n" + &self.text;
    }

    /// write about found parameter to stdout and save when needed
    pub fn write_and_save(&self, config: &Config, initial_response: &Response, reason_kind: ReasonKind, parameter: &str, diff: Option<&str>) -> Result<(), Box<dyn Error>> {

        let mut message = match reason_kind {
            ReasonKind::Code => format!(
                "{}: code {} -> {}",
                &parameter,
               initial_response.code(),
                &self.code(),
            ),
            ReasonKind::Text => format!(
                "{}: page {} -> {} ({})",
                &parameter,
               initial_response.text.len(),
                &self.text.len().to_string().bright_yellow(),
                diff.unwrap()
            ),
            ReasonKind::Reflected => format!("{}: {}", "reflects".bright_blue(), parameter),
            ReasonKind::NotReflected => format!("{}: {}", "not reflected one".bright_cyan(), parameter),
        };

        if config.verbose > 0 {
            if !config.save_responses.is_empty() {
                message += &format!(" [saved to {}]", save_request(config, self, parameter)?);
            }

            writeln!(io::stdout(), "{}", message).ok();
        } else if !config.save_responses.is_empty() {
            save_request(config, self, parameter)?;
        }

        Ok(())
    }

    fn kind(&self) -> Status {
        if self.code <= 199 {
            Status::Other
        } else if self.code <= 299 {
            Status::Ok
        } else if self.code <= 399 {
            Status::Redirect
        } else if self.code <= 499 {
            Status::UserFault
        } else if self.code <= 599 {
            Status::ServerFault
        } else {
            Status::Other
        }
    }

    pub fn code(&self) -> String {
        match self.kind() {
            Status::Ok => self.code.to_string().bright_green().to_string(),
            Status::Redirect => self.code.to_string().bright_blue().to_string(),
            Status::UserFault => self.code.to_string().bright_yellow().to_string(),
            Status::ServerFault => self.code.to_string().bright_red().to_string(),
            Status::Other => self.code.to_string().magenta().to_string(),
        }
    }

    /// get possible parameters from the page itself
    pub fn get_possible_parameters(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let body = &self.text;

        let re_special_chars = Regex::new(r#"[\W]"#).unwrap();

        let re_name = Regex::new(r#"(?i)name=("|')?"#).unwrap();
        let re_inputs = Regex::new(r#"(?i)name=("|')?[\w-]+"#).unwrap();
        for cap in re_inputs.captures_iter(body) {
            found.push(re_name.replace_all(&cap[0], "").to_string());
        }

        let re_var = Regex::new(r#"(?i)(var|let|const)\s+?"#).unwrap();
        let re_full_vars = Regex::new(r#"(?i)(var|let|const)\s+?[\w-]+"#).unwrap();
        for cap in re_full_vars.captures_iter(body) {
            found.push(re_var.replace_all(&cap[0], "").to_string());
        }

        let re_words_in_quotes = Regex::new(r#"("|')[a-zA-Z0-9]{3,20}('|")"#).unwrap();
        for cap in re_words_in_quotes.captures_iter(body) {
            found.push(re_special_chars.replace_all(&cap[0], "").to_string());
        }

        let re_words_within_objects = Regex::new(r#"[\{,]\s*[[:alpha:]]\w{2,25}:"#).unwrap();
        for cap in re_words_within_objects.captures_iter(body){
            found.push(re_special_chars.replace_all(&cap[0], "").to_string());
        }

        found.sort();
        found.dedup();
        found
    }

    ///print the whole response
    pub fn print(&self) -> String {
        let http_version = match self.http_version {
            Some(val) => match val {
                http::Version::HTTP_09 => "HTTP/0.9",
                http::Version::HTTP_10 => "HTTP/1.0",
                http::Version::HTTP_11 => "HTTP/1.1",
                http::Version::HTTP_2 => "HTTP/2",
                http::Version::HTTP_3 => "HTTP/3",
                _ => "HTTP/x",
            },
            None => "HTTP/x"
        };

        format!("{} {} \n{}", http_version, self.code, self.text)
    }

    ///print the request and response
    pub fn print_all(&mut self) -> String {
        self.request.as_mut().unwrap().print() + &self.print()
    }
}