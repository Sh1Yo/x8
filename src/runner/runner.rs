use std::error::Error;

use reqwest::Client;

use crate::{structs::{Config, InjectionPlace, Stable}, utils::{empty_reqs, random_line, verify, self, replay}, network::{request::{RequestDefaults, Request}, response::Response}};

use super::{output::RunnerOutput, found_parameters::{FoundParameter, Parameters}};

pub struct Runner<'a> {
    pub config: &'a Config,
    pub request_defaults: RequestDefaults,
    replay_client: &'a Client,
    pub possible_params: Vec<String>,

    pub max: usize,
    pub stable: Stable,
    pub initial_response: Response<'a>,

    //shared_info: SharedInfo<'a>,
    pub diffs: Vec<String>,
}

impl<'a> Runner<'a> {

    /// creates a runner, makes an initial response
    pub async fn new(
        config: &'a Config,
        request_defaults: &'a mut RequestDefaults,
        replay_client: &'a Client,
    ) -> Result<Runner<'a>, Box<dyn Error>> {
         //make first request and collect some information like code, reflections, possible parameters
         //we are making another request defaults because the original one will be changed right after

         let mut temp_request_defaults = request_defaults.clone();

         //we need a random_parameter with a long value in order to increase accuracy while determining the default amount of reflections
         let mut random_parameter = vec![(random_line(10), random_line(10))];

         temp_request_defaults.parameters.append(&mut random_parameter);

         let initial_response = Request::new(&temp_request_defaults, vec![])
                                                 .send()
                                                 .await?;

         //add possible parameters to the list of parameters in case the injection place is not headers
         let possible_params = if request_defaults.injection_place != InjectionPlace::Headers {
            initial_response.get_possible_parameters()
         } else {
            Vec::new()
         };

         //find how many times reflected our random parameter
         request_defaults.amount_of_reflections = initial_response.count(&temp_request_defaults.parameters.iter().next().unwrap().1);

         //some "magic" to be able to return initial_response
         //turns out you can't simple do 'initial_response.request = None'.
         //otherwise throws lifetime errors
         let initial_response = Response{
             time: initial_response.time,
             code: initial_response.code,
             headers: initial_response.headers,
             text: initial_response.text,
             reflected_parameters: initial_response.reflected_parameters,
             additional_parameter: initial_response.additional_parameter,
             request: None,
             http_version: initial_response.http_version
         };

         Ok(
             Runner{
                 config,
                 request_defaults: request_defaults.clone(),
                 replay_client,
                 possible_params,
                 max: 0, //to be filled later, in stability-checker()
                 stable: Default::default(),
                 initial_response: initial_response,
                 diffs: Vec::new(),
             }
         )
    }

    /// acually runs the runner
    pub async fn run(mut self, params: &mut Vec<String>) -> Result<RunnerOutput, Box<dyn Error>> {

        self.stability_checker().await?;

        //add only unique possible params to the vec of all params (the tool works properly only with unique parameters)
        //less efficient than making it within the sorted vec
        //but I want to preserve order
        for param in self.possible_params.iter() {
            if !params.contains(&param) {
                params.push(param.to_owned());
            }
        }

        let (diffs, mut found_params) = self.check_parameters(params).await?;

        self.check_non_random_parameters(&mut found_params).await?;

        //remove duplicates
        let mut found_params = found_params.process(self.request_defaults.injection_place);

        //verify found parameters
        if self.config.verify {
            found_params = if let Ok(filtered_params)
                = verify(&self.initial_response, &self.request_defaults, &found_params, &diffs, &self.stable).await {
                filtered_params
            } else {
                utils::info(&self.config, "~", "was unable to verify found parameters");
                found_params
            };
        }

        if !self.config.replay_proxy.is_empty() {
            if let Err(_) = replay(&self.config, &self.request_defaults, &self.replay_client, &found_params).await {
                utils::info(&self.config, "~", "was unable to resend found parameters via different proxy");
            }
        }

        Ok(RunnerOutput::new(&self.request_defaults, &self.initial_response, found_params))
    }

    //check parameters like admin=true
    async fn check_non_random_parameters(&self, found_params: &mut Vec<FoundParameter>) -> Result<(), Box<dyn Error>> {
        if !self.request_defaults.disable_custom_parameters {
            let mut custom_parameters = self.config.custom_parameters.clone();
            let mut params = Vec::new();

            // in a loop check common parameters like debug, admin, .. with common values true, 1, false..
            // until there's no values left
            loop {
                for (k, v) in custom_parameters.iter_mut() {

                    //do not request parameters that already have been found
                    if found_params.iter().map(|x| x.name.split("=").next().unwrap()).any(|x| x == k) {
                        continue;
                    }

                    if !v.is_empty() {
                        params.push([k.as_str(), "=", v.pop().unwrap().as_str()].concat());
                    }
                }

                if params.is_empty() {
                    break;
                }

                found_params.append(&mut self.check_parameters(&params).await?.1);
                params.clear();
            }
        }

        Ok(())
    }

    /// makes several requests in order to learn how the page behaves
    /// tries to increase the max amount of parameters per request in case the default value not changed
    async fn stability_checker(&mut self) -> Result<(), Box<dyn Error>> {

        //guess or get from the user the amount of parameters to send per request
        let default_max = match self.config.max {
            Some(var) => var as isize,
            None => {
                match self.config.injection_place {
                    InjectionPlace::Body => -512,
                    InjectionPlace::Path => -128,
                    InjectionPlace::Headers => -64,
                    InjectionPlace::HeaderValue => -64,
                }
            }
        };

        self.max = default_max.abs() as usize;

        //make a few requests and collect all persistent diffs, check for stability
        (self.diffs, self.stable) = empty_reqs(
            self.config,
            &self.initial_response,
            &self.request_defaults,
            self.config.learn_requests_count,
            self.max,
        ).await?;

        if self.config.reflected_only && !self.stable.reflections {
            Err("Reflections are not stable")?;
        }

        //check whether it is possible to use 192 or 256 params in a single request instead of 128 default
        if default_max == -128  {
            self.try_to_increase_max().await?;
        }

        Ok(())
    }

    /// checks whether the increasing of the amount of parameters changes the page
    /// changes self.max in case the page is stable with more parameters per request
    pub async fn try_to_increase_max(&mut self) -> Result<(), Box<dyn Error>> {
        let response = Request::new_random(&self.request_defaults, self.max + 64)
                                    .send()
                                    .await?;

        let (is_code_different, new_diffs) = response.compare(&self.initial_response, &self.diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        //in case the page isn't different from previous one - try to increase max amount of parameters by 128
        if !is_code_different && (!self.stable.body || is_the_body_the_same) {

            let response =  Request::new_random(&self.request_defaults, self.max + 128)
                    .send()
                    .await?;

            let (is_code_different, new_diffs) = response.compare(&self.initial_response, &self.diffs)?;

            if !new_diffs.is_empty() {
                is_the_body_the_same = false;
            }

            if !is_code_different && (!self.stable.body || is_the_body_the_same) {
                self.max += 128
            } else {
                self.max += 64
            }

        }

        Ok(())
    }
}