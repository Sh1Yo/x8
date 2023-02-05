use std::{error::Error, io::{self, Write}};

use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::{
    config::structs::Config,
    network::{
        request::{Request, RequestDefaults},
        response::Response,
        utils::{create_client, InjectionPlace},
    },
    utils::{self, color_id, random_line, progress_style_learn_requests, is_id_important},
    DEFAULT_PROGRESS_URL_MAX_LEN, MAX_PAGE_SIZE,
};

use super::{
    output::RunnerOutput,
    utils::{fold_url, replay, verify, FoundParameter, Parameters, Stable},
};

pub struct Runner<'a> {
    /// unique id of the runner to distinguish output between different urls
    pub id: usize,

    pub config: &'a Config,

    /// request data to create the request object
    pub request_defaults: RequestDefaults,

    /// parameters found by scraping words from the page
    pub possible_params: Vec<String>,

    /// the max amount of parameters to send per request
    pub max: usize,

    /// whether body or/and reflections are stable
    pub stable: Stable,

    /// initial response to compare with
    pub initial_response: Response<'a>,

    /// page's diffs for the current url|method pair
    pub diffs: Vec<String>,

    /// progress bar object to print progress bar & found parameters
    pub progress_bar: &'a ProgressBar,
}

impl<'a> Runner<'a> {
    /// creates a runner, makes an initial response
    pub async fn new(
        config: &'a Config,
        request_defaults: &'a mut RequestDefaults,
        progress_bar: &'a ProgressBar,
        id: usize,
    ) -> Result<Runner<'a>, Box<dyn Error>> {
        // make first request and collect some information like code, reflections, possible parameters
        // we are making another request defaults because the original one will be changed right after
        let mut temp_request_defaults = request_defaults.clone();

        // we need a random_parameter with a long value in order to increase accuracy while determining the default amount of reflections
        let mut random_parameter = vec![(random_line(10), random_line(10))];

        temp_request_defaults
            .parameters
            .append(&mut random_parameter);

        let initial_response = Request::new(&temp_request_defaults, vec![]).send().await?;

        // add possible parameters to the list of parameters in case the injection place is not headers
        let possible_params = if request_defaults.injection_place != InjectionPlace::Headers {
            initial_response.get_possible_parameters()
        } else {
            Vec::new()
        };

        // find how many times was the random parameter reflected
        request_defaults.amount_of_reflections =
            initial_response.count(&temp_request_defaults.parameters.first().unwrap().1);

        // some "magic" to be able to return initial_response
        // otherwise throws lifetime errors
        // turns out you can't simple do 'initial_response.request = None'.
        let initial_response = Response {
            time: initial_response.time,
            code: initial_response.code,
            headers: initial_response.headers,
            text: initial_response.text,
            reflected_parameters: initial_response.reflected_parameters,
            request: None,
            http_version: initial_response.http_version,
        };

        Ok(Runner {
            config,
            request_defaults: request_defaults.clone(),
            possible_params,
            max: 0, //to be filled later, in stability-checker()
            stable: Default::default(),
            initial_response,
            diffs: Vec::new(),
            progress_bar,
            id,
        })
    }

    /// actually runs the runner
    pub async fn run(mut self, params: &mut Vec<String>) -> Result<RunnerOutput, Box<dyn Error>> {
        self.write_banner_url();

        // makes a few request to check page's behavior
        self.stability_checker().await?;

        if self.config.max.is_none() {
            utils::info(
                self.config,
                self.id,
                self.progress_bar,
                "info",
                format!("Amount of parameters per request - {}", self.max),
            );
        }

        // add only unique possible params to the vec of all params (the tool works properly only with unique parameters)
        // less efficient than making it within the sorted vec but I want to preserve the order
        for param in self.possible_params.iter() {
            if !params.contains(param) {
                params.push(param.to_owned());
            }
        }

        // try to find existing parameters from the list
        let (diffs, mut found_params) = if !params.is_empty() {
            self.check_parameters(params).await?
        } else {
            utils::info(
                self.config,
                self.id,
                self.progress_bar,
                "info",
                "No parameters were provided",
            );
            (Vec::new(), Vec::new())
        };

        self.check_non_random_parameters(&mut found_params).await?;

        // remove duplicates
        let mut found_params = found_params.process(self.request_defaults.injection_place);

        // verify found parameters
        if self.config.verify {
            found_params = if let Ok(filtered_params) = verify(
                &self.initial_response,
                &self.request_defaults,
                &found_params,
                &diffs,
                &self.stable,
            )
            .await
            {
                filtered_params
            } else {
                utils::info(
                    self.config,
                    self.id,
                    self.progress_bar,
                    "~",
                    "was unable to verify found parameters",
                );
                found_params
            };
        }

        // replay request with found parameters via another proxy
        if !self.config.replay_proxy.is_empty() {
            if replay(
                self.config,
                &self.request_defaults,
                &create_client(self.config)?,
                &found_params,
            ).await
            .is_err() {
                utils::info(
                    self.config,
                    self.id,
                    self.progress_bar,
                    "~",
                    "was unable to resend found parameters via different proxy",
                );
            }
        }

        Ok(RunnerOutput::new(
            &self.request_defaults,
            &self.initial_response,
            found_params,
        ))
    }

    /// check parameters with non random values
    async fn check_non_random_parameters(
        &self,
        found_params: &mut Vec<FoundParameter>,
    ) -> Result<(), Box<dyn Error>> {
        if !self.request_defaults.disable_custom_parameters {
            let mut custom_parameters = self.config.custom_parameters.clone();
            let mut params = Vec::new();

            // in a loop check common parameters like debug, admin, .. with common values true, 1, false..
            // until there's no values left
            loop {
                for (k, v) in custom_parameters.iter_mut() {
                    //do not request parameters that already have been found
                    if found_params
                        .iter()
                        .map(|x| x.name.split('=').next().unwrap())
                        .any(|x| x == k)
                    {
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
        // guess or get from the user the amount of parameters to send per request
        let default_max = match self.config.max {
            Some(var) => var as isize,
            None => match self.request_defaults.injection_place {
                InjectionPlace::Body => -512,
                InjectionPlace::Path => self.try_to_guess_the_right_max_for_query().await?,
                InjectionPlace::Headers => -64,
                InjectionPlace::HeaderValue => -64,
            },
        };

        self.max = default_max.unsigned_abs();

        // make a few requests and collect all persistent diffs, check for stability
        self.empty_reqs().await?;

        if self.config.reflected_only && !self.stable.reflections {
            Err("Reflections are not stable")?;
        }

        // check whether it is possible to use 192 or 256 params in a single request instead of 128 default
        if default_max == -128 {
            self.try_to_increase_max().await?;
        }

        Ok(())
    }

    /// makes first requests and checks page behavior
    /// fills self.diffs and self.stable
    pub async fn empty_reqs(&mut self) -> Result<(), Box<dyn Error>> {
        let mut stable = Stable {
            body: true,
            reflections: true,
        };
        let mut diffs: Vec<String> = Vec::new();

        // set up progress bar
        self.prepare_progress_bar(progress_style_learn_requests(self.config), self.config.learn_requests_count);

        for _ in 0..self.config.learn_requests_count {
            // to increase stability
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

            let response = Request::new_random(&self.request_defaults, self.max)
                .send()
                .await?;

            self.progress_bar.inc(1);

            // do not check pages >25MB because usually its just a binary file or sth
            if response.text.len() > MAX_PAGE_SIZE && !self.config.force {
                Err("The page's size > 25MB. Use --force flag to disable this error")?;
            }

            if !response.reflected_parameters.is_empty() {
                stable.reflections = false;
            }

            let (is_code_diff, mut new_diffs) = response.compare(&self.initial_response, &diffs)?;

            if is_code_diff {
                Err("The page is not stable (code)")?
            }

            diffs.append(&mut new_diffs);
        }

        // check the last time
        let response = Request::new_random(&self.request_defaults, self.max)
            .send()
            .await?;

        // in case the page is still different from other random ones - the body isn't stable
        if !response
            .compare(&self.initial_response, &diffs)?
            .1
            .is_empty()
        {
            utils::info(
                self.config,
                self.id,
                self.progress_bar,
                "~",
                "The page is not stable (body)",
            );
            stable.body = false;
        }

        (self.diffs, self.stable) = (diffs, stable);

        Ok(())
    }

    /// checks whether the increasing of the amount of parameters changes the page
    /// changes self.max in case the page is stable with more parameters per request
    pub async fn try_to_increase_max(&mut self) -> Result<(), Box<dyn Error>> {

        let delta = self.max / 2;

        let response = Request::new_random(&self.request_defaults, self.max + delta)
            .send()
            .await?;

        let (is_code_different, new_diffs) =
            response.compare(&self.initial_response, &self.diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        // in case the page isn't different from previous one - try to increase max amount of parameters by 128
        if !is_code_different && (!self.stable.body || is_the_body_the_same) {
            let response = Request::new_random(&self.request_defaults, self.max + delta*2)
                .send()
                .await?;

            let (is_code_different, new_diffs) =
                response.compare(&self.initial_response, &self.diffs)?;

            if !new_diffs.is_empty() {
                is_the_body_the_same = false;
            }

            if !is_code_different && (!self.stable.body || is_the_body_the_same) {
                self.max += delta*2
            } else {
                self.max += delta
            }
        }

        Ok(())
    }

    /// tries to detect the right amount of parameters that can be send per request in query
    /// TODO maybe detect based on reflection as well
    pub async fn try_to_guess_the_right_max_for_query(&mut self) -> Result<isize, Box<dyn Error>> {

        let mut max = 128;

        let mut response = match Request::new_random(&self.request_defaults, max)
            .send()
            .await {
                Ok(val) => val,
                // some servers may cut connection in case url is too long
                // that's why we assume that this request returned response with status code = 0.
                Err(_) => {
                    Request::empty_response(Request::new_random(&self.request_defaults, 0))
                }
        };

        loop {
            // the choosen max is okay
            if self.initial_response.code == response.code {
                break
            }

            if Request::new_random(&self.request_defaults, 0).send().await?.code != self.initial_response.code {
                Err("The page became unstable (code)")?
            };

            max /= 2;

            if max < 4 {
                Err("Unable to guess the max amount of parameters per request. Try to use --max command line argument.")?
            }

            response = match Request::new_random(&self.request_defaults, max)
                .send()
                .await {
                    Ok(val) => val,
                    Err(_) => {
                        Request::empty_response(Request::new_random(&self.request_defaults, 0))
                    }
            };
        }

        Ok(max as isize *-1)
    }

    pub fn prepare_progress_bar(&self, sty: ProgressStyle, length: usize) {
        self.progress_bar.reset();
        self.progress_bar.set_prefix(self.make_progress_prefix());
        self.progress_bar.set_style(sty);
        self.progress_bar.set_length(length as u64);
    }

    fn make_progress_prefix(&self) -> String {
        // to align all the progress bars
        let id = if is_id_important(self.config) {
            let mut id = self.id.to_string() + ":";
            id += &" ".repeat(1 + self.config.urls.len().to_string().len() - id.to_string().len());
            format!("{} ", id.replace(&self.id.to_string(), &color_id(self.id)))
        } else {
            String::new()
        };

        let mut method = self.request_defaults.method.clone();
        method +=
            &" ".repeat(self.config.methods.iter().map(|x| x.len()).max().unwrap() - method.len());

        format!(
            "{}{} {}",
            id,
            method.blue(),
            fold_url(
                &self.request_defaults.url_without_default_port(),
                DEFAULT_PROGRESS_URL_MAX_LEN
            )
            .green()
        )
    }

    pub fn write_banner_url(&self) {

        let id = if is_id_important(self.config) {
            format!("[{}] ", color_id(self.id))
        } else {
            String::new()
        };

        let msg = format!(
            "{}{} {} ({}) [{}] {{{}}}",
            id,
            self.request_defaults.method.blue(),
            self.request_defaults.url_without_default_port().green(),
            self.initial_response.code(),
            self.initial_response.text.len().to_string().green(),
            self.request_defaults
                .amount_of_reflections
                .to_string()
                .magenta()
        );

        if self.config.disable_progress_bar {
            writeln!(io::stdout(), "{}", msg).ok();
        } else {
            self.progress_bar.println(msg);
        }
    }
}
