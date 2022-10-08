use serde::Serialize;

use crate::{structs::{Config, InjectionPlace}, network::{request::{RequestDefaults, Request}, response::Response}};

use super::found_parameters::FoundParameter;

#[derive(Debug, Serialize)]
pub struct RunnerOutput {
    pub method: String,

    //request url without injection point
    pub url: String,

    //initial response code
    pub status: u16,

    //initial response size (body + headers)
    pub size: usize,

    pub found_params: Vec<FoundParameter>,

    pub injection_place: InjectionPlace,
}

impl RunnerOutput {

    pub fn new(request_defaults: &RequestDefaults, initial_response: &Response, found_params: Vec<FoundParameter>) -> Self {
        Self{
            method: request_defaults.method.clone(),
            //remove injection point in case the injection point within url
            url: if request_defaults.injection_place == InjectionPlace::Path {
                request_defaults.url().replace("?%s", "").replace("&%s", "")
             } else { request_defaults.url()},
            status: initial_response.code,
            size: initial_response.text.len(),
            found_params,
            injection_place: request_defaults.injection_place
        }
    }

    /// parses the runner output struct to one specified in config format
    pub fn parse(&self, config: &Config, request_defaults: &RequestDefaults) -> String {
        let mut request = Request::new(request_defaults, self.found_params.iter().map(
            //in case a parameter has a non standart value (like 'true')
            //it should be treated differently (=true) should be added
            //otherwise that parameter will have random value
            |x| if x.value.is_none() { x.name.to_owned() } else { format!("{}={}", x.name, x.value.as_ref().unwrap()) }
        ).collect());

        request.prepare(None);

        match config.output_format.as_str() {
            "url" => {
                //make line an url with injection point
                let line = if !self.found_params.is_empty() && request_defaults.injection_place == InjectionPlace::Path  {
                    if !self.url.contains("?") {
                        self.url.clone() + "?%s"
                    } else {
                        self.url.clone() + "&%s"
                    }
                } else {
                    self.url.clone()
                };

                (line+"\n").replace("%s", &request.make_query())
            },

            "json" => {
                serde_json::to_string(&self).unwrap()
            },

            "request" => {
                request.print()
            },

            _ => {
                format!(
                    "{} {} % {}\n",
                    &self.method,
                    &self.url,
                    self.found_params.iter().map(|x| x.name.as_str()).collect::<Vec<&str>>().join(", ")
                )
            },
        }
    }
}