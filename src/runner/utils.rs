use std::{
    error::Error,
};

use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Client;
use serde::Serialize;
use colored::Colorize;

use crate::{
    config::structs::Config,
    network::{
        request::{Request, RequestDefaults},
        response::Response,
        utils::InjectionPlace,
    },
    utils::random_line, VALUE_LENGTH,
};

#[derive(Debug, Default)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ReasonKind {
    Code,
    Text,
    Reflected,
    NotReflected,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoundParameter {
    pub name: String,

    /// None in case the random parameter name is used
    pub value: Option<String>,

    pub diffs: String,
    pub status: u16,
    pub size: usize,
    pub reason_kind: ReasonKind,
}

impl FoundParameter {
    pub fn new<S: Into<String>>(
        name: S,
        diffs: &[String],
        status: u16,
        size: usize,
        reason_kind: ReasonKind,
    ) -> Self {
        let name = name.into();

        let (name, value) = if name.contains('=') {
            let mut name = name.split('=');
            (
                name.next().unwrap().to_string(),
                Some(name.next().unwrap().to_string()),
            )
        } else {
            (name, None)
        };

        Self {
            name,
            value,
            diffs: diffs.join("|"),
            status,
            size,
            reason_kind,
        }
    }

    /// just returns (Key, Value) pair
    pub fn get(&self) -> (String, String) {
        (
            self.name.clone(),
            self.value.clone().unwrap_or_else(|| random_line(VALUE_LENGTH)),
        )
    }

    /// returns colored param name and param=value in case a non random value is used
    pub fn get_colored(&self) -> String {
        let param = match self.reason_kind {
            ReasonKind::Code => self.name.yellow(),
            ReasonKind::Text => self.name.bright_yellow(),
            ReasonKind::Reflected => self.name.bright_blue(),
            ReasonKind::NotReflected => self.name.bright_cyan(),
        };

        if self.value.is_some() {
            format!("{}={}", param, self.value.as_ref().unwrap())
        } else {
            param.to_string()
        }
    }
}

pub trait Parameters {
    fn contains_name(&self, key: &str) -> bool;
    fn contains_name_case_insensitive(&self, key: &str) -> bool;
    fn contains_element(&self, el: &FoundParameter) -> bool;
    fn contains_element_case_insensitive(&self, el: &FoundParameter) -> bool;
    fn process(self, injection_place: InjectionPlace) -> Self;
}

impl Parameters for Vec<FoundParameter> {
    /// checks whether the element with the same name exists within the vector
    fn contains_name(&self, key: &str) -> bool {
        self.iter().any(|x| x.name == key)
    }

    fn contains_name_case_insensitive(&self, key: &str) -> bool {
        self.iter()
            .any(|x| x.name.to_lowercase() == key.to_lowercase())
    }

    /// checks whether the combination of name, reason_kind, status exists within the vector
    fn contains_element(&self, el: &FoundParameter) -> bool {
        self.iter()
            .any(|x| x.name == el.name && x.reason_kind == el.reason_kind && x.status == el.status)
    }

    fn contains_element_case_insensitive(&self, el: &FoundParameter) -> bool {
        self.iter().any(|x| {
            x.name.to_lowercase() == el.name.to_lowercase()
                && x.reason_kind == el.reason_kind
                && x.status == el.status
        })
    }

    /// removes duplicates: [debug={random}, Debug={random}, debug=true] -> [debug={random}]
    /// not very fast but we are doing it a few times per run anyway
    fn process(mut self, injection_place: InjectionPlace) -> Self {
        fn capitalize_first(mut x: FoundParameter) -> FoundParameter {
            let mut chars = x.name.chars();
            x.name = chars
                .next()
                .map(|first_letter| first_letter.to_uppercase())
                .into_iter()
                .flatten()
                .chain(chars)
                .collect();
            x
        }

        // in case, for example, 'admin' param is found -- remove params like 'admin=true' or sth
        self = self
            .iter()
            .filter(|x| !(x.name.contains('=') && self.contains_element(x)))
            .map(|x| x.to_owned())
            .collect();

        // if there's lowercase alternative - remove that parameter
        // so Host & HOST & host are the same parameters and only host should stay
        self = self
            .iter()
            .filter(|x| {
                x.name.to_lowercase() == x.name || !self.contains_name(&x.name.to_lowercase())
            })
            .map(|x| x.to_owned())
            .collect();

        // for now reqwest capitalizes first char of every header
        self = if injection_place == InjectionPlace::Headers {
            self.iter()
                .map(|x| capitalize_first(x.to_owned()))
                .collect()
        } else {
            self
        };

        // if there's HOST and Host only one of them should stay
        let mut found_params = vec![];
        for el in self {
            if !found_params.contains_name_case_insensitive(&el.name) {
                found_params.push(el);
            }
        }

        found_params
    }
}

pub(super) async fn replay<'a>(
    config: &Config,
    request_defaults: &RequestDefaults,
    replay_client: &Client,
    found_params: &Vec<FoundParameter>,
) -> Result<(), Box<dyn Error>> {
    // get cookies
    Request::new(request_defaults, vec![])
        .send_by(replay_client)
        .await?;

    if config.replay_once {
        Request::new(
            request_defaults,
            found_params
                .iter()
                .map(|x| x.get())
                .map(|(x, y)| format!("{}={}", x, y))
                .collect::<Vec<String>>(),
        )
        .send_by(replay_client)
        .await?;
    } else {
        for param in found_params {
            let param = param.get();
            Request::new(request_defaults, vec![format!("{}={}", param.0, param.1)])
                .send_by(replay_client)
                .await?;
        }
    }

    Ok(())
}

pub(super) async fn verify<'a>(
    initial_response: &'a Response<'a>,
    request_defaults: &'a RequestDefaults,
    found_params: &Vec<FoundParameter>,
    diffs: &Vec<String>,
    stable: &Stable,
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {
    let mut filtered_params = Vec::with_capacity(found_params.len());

    for param in found_params {
        let param_value = param.get();
        let mut response = Request::new(request_defaults, vec![format!("{}={}", param_value.0, param_value.1)])
            .send()
            .await?;

        let (is_code_diff, new_diffs) = response.compare(initial_response, diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        response.fill_reflected_parameters(initial_response);

        if is_code_diff || !response.reflected_parameters.is_empty() || stable.body && !is_the_body_the_same {
            filtered_params.push(param.clone());
        }
    }

    Ok(filtered_params)
}

pub enum ParamPatterns {
    /// _anything
    SpecialPrefix(char),

    /// anything1243124
    /// from example: (string = anything, usize=7)
    HasNumbersPostfix(String, usize),

    /// any!thing
    ContainsSpecial(char),

    /// password_anything
    BeforeUnderscore(String),

    /// anything_password
    AfterUnderscore(String),

    /// password-anything
    BeforeDash(String),

    /// anything-password
    AfterDash(String),
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

    /// in case 2 patterns match like sth1-sth2 == check all the patterns.
    /// In case nothing confirms -- leave sth1-sth2
    /// In case all confirms ¯\_(ツ)_/¯
    pub fn get_patterns(param: &str) -> Vec<Self> {
        lazy_static! {
            static ref RE_NUMBER_PREFIX: Regex = Regex::new(r"^([^\d]+)(\d+)$").unwrap();
        };

        let mut patterns = Vec::new();
        let param_chars: Vec<char> = param.chars().collect();

        if param_chars[0].is_ascii_punctuation() {
            patterns.push(ParamPatterns::SpecialPrefix(param_chars[0]))
        }

        let special_chars: Vec<&char> = param_chars
            .iter()
            .filter(|x| x.is_ascii_punctuation() && x != &&'-' && x != &&'_')
            .collect();
        if special_chars.len() == 1 {
            patterns.push(ParamPatterns::ContainsSpecial(*special_chars[0]));
        }

        if param_chars.contains(&'-') {
            // we're treating as if there's only one '-' for now.
            // maybe needs to be changed in future
            let mut splitted = param.split('-');

            patterns.push(ParamPatterns::BeforeDash(
                splitted.next().unwrap().to_string(),
            ));
            patterns.push(ParamPatterns::AfterDash(
                splitted.next().unwrap().to_string(),
            ));
        }

        if param_chars.contains(&'_') {
            // we're treating as if there's only one '_' for now.
            // maybe needs to be changed in future
            let mut splitted = param.split('_');

            patterns.push(ParamPatterns::BeforeUnderscore(
                splitted.next().unwrap().to_string(),
            ));
            patterns.push(ParamPatterns::AfterUnderscore(
                splitted.next().unwrap().to_string(),
            ));
        }

        if let Some(caps) = RE_NUMBER_PREFIX.captures(param) {
            let (word, digits) = (caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str());
            patterns.push(ParamPatterns::HasNumbersPostfix(
                word.to_string(),
                digits.len(),
            ))
        }

        patterns
    }
}

/// under development
pub(super) async fn _smart_verify(
    initial_response: &Response<'_>,
    request_defaults: &RequestDefaults,
    found_params: &Vec<FoundParameter>,
    diffs: &Vec<String>,
    stable: &Stable,
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {
    let mut filtered_params = Vec::with_capacity(found_params.len());

    for param in found_params {
        let _param_patterns = ParamPatterns::get_patterns(&param.name);

        let mut response = Request::new(request_defaults, vec![param.name.clone()])
            .send()
            .await?;

        let (is_code_the_same, new_diffs) = response.compare(initial_response, &diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        response.fill_reflected_parameters(initial_response);

        if !is_code_the_same || !response.reflected_parameters.is_empty() || stable.body && !is_the_body_the_same {
            filtered_params.push(param.clone());
        }
    }

    Ok(filtered_params)
}

/// returns last n chars of an url
pub(super) fn fold_url(url: &str, n: usize) -> String {
    if url.len() <= n + 2 {
        //we need to add some spaces to align the progress bars
        url.to_string() + &" ".repeat(2 + n - url.len())
    } else {
        "..".to_owned() + &url[url.len() - n..]
    }
}
