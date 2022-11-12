use serde::Serialize;
use colored::Colorize;

use crate::{
    config::structs::Config,
    network::{
        request::{Request, RequestDefaults},
        response::Response,
        utils::InjectionPlace,
    },
};

use super::utils::FoundParameter;

#[derive(Debug, Serialize)]
pub struct RunnerOutput {
    /// request's method
    pub method: String,

    /// request url without injection point
    pub url: String,

    /// initial response code
    pub status: u16,

    /// initial response size (body + headers)
    pub size: usize,

    pub found_params: Vec<FoundParameter>,

    pub injection_place: InjectionPlace,


    /// prepared query with found parameters
    #[serde(skip_serializing)]
    pub query: String,

    /// prepared request with found parameters
    #[serde(skip_serializing)]
    pub request: String,
}

pub trait ParseOutputs {
    fn parse_output(&self, config: &Config) -> String;
}

impl RunnerOutput {
    pub fn new(
        request_defaults: &RequestDefaults,
        initial_response: &Response,
        found_params: Vec<FoundParameter>,
    ) -> Self {
        Self {
            method: request_defaults.method.clone(),
            //remove injection point in case the injection point within url
            url: if request_defaults.injection_place == InjectionPlace::Path {
                request_defaults.url_without_default_port().replace("?%s", "").replace("&%s", "")
            } else {
                request_defaults.url_without_default_port()
            },
            status: initial_response.code,
            size: initial_response.text.len(),
            found_params,
            injection_place: request_defaults.injection_place,
            query: String::new(),
            request: String::new(),
        }
    }

    /// fills self.request and self.query if they're needed for output
    pub fn prepare(&mut self, config: &Config, request_defaults: &RequestDefaults) {
        if config.output_format == "url" || config.output_format == "request" {
            let mut request = Request::new(
                request_defaults,
                self.found_params
                    .iter()
                    .map(
                        //in case a parameter has a non standart value (like 'true')
                        //it should be treated differently (=true) should be added
                        //otherwise that parameter will have random value
                        |x| {
                            if x.value.is_none() {
                                x.name.to_owned()
                            } else {
                                format!("{}={}", x.name, x.value.as_ref().unwrap())
                            }
                        },
                    )
                    .collect(),
            );

            request.prepare();

            if config.output_format == "url" {
                self.query = request.make_query();
            } else {
                self.request = request.print();
            }
        }
    }

    /// parses the runner output struct to one specified in config format
    pub fn parse(&self, config: &Config) -> String {
        match config.output_format.as_str() {
            "url" => {
                //make line an url with injection point
                let line = if !self.found_params.is_empty()
                    && self.injection_place == InjectionPlace::Path
                {
                    if !self.url.contains("?") {
                        self.url.clone() + "?%s"
                    } else {
                        self.url.clone() + "&%s"
                    }
                } else {
                    self.url.clone()
                };

                (line).replace("%s", &self.query)
            }

            "request" => self.request.clone(),

            _ => {
                format!(
                    "{} {} % {}",
                    &self.method.blue(),
                    &self.url,
                    self.found_params
                        .iter()
                        .map(|x| x.get_colored())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            }
        }
    }
}

impl ParseOutputs for Vec<RunnerOutput> {
    fn parse_output(&self, config: &Config) -> String {
        // print an array of json objects instead of just new line separeted new objects
        if config.output_format.as_str() == "json" {
            serde_json::to_string(&self).unwrap()
        // otherwise calls .parse on every RunnerOutput
        } else {
            self.iter()
                .map(|x| x.parse(config))
                .collect::<Vec<String>>()
                .join("")
        }
    }
}
