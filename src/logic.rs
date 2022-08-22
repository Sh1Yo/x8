use crate::{
    structs::{Config, Stable, FuturesData, Request, RequestDefaults, FoundParameter},
    utils::{save_request, write_and_save},
};
use colored::*;
use futures::stream::StreamExt;
use std::{sync::Arc, error::Error};
use parking_lot::Mutex;

use std::{
    collections::HashMap,
    io::{self, Write},
};

/// check parameters in a loop chunk by chunk
pub async fn check_parameters<'a>(
    first: bool,
    config: &Config,
    request_defaults: &RequestDefaults<'a>,
    diffs: &mut Vec<String>,
    params: &Vec<String>,
    stable: &Stable,
    max: usize,
    green_lines: &mut HashMap<String, usize>,
    remaining_params: &mut Vec<Vec<String>>,
    found_params: &mut Vec<FoundParameter>,
) -> Result<(), Box<dyn Error>> {
    //the amount of requests needed for process all the parameters
    let all = params.len() / max;
    let mut count: usize = 0;

    //make diffs and green_lines accessable by all futures
    let shared_diffs = Arc::new(Mutex::new(diffs));
    let shared_green_lines = Arc::new(Mutex::new(green_lines));

    let futures_data = futures::stream::iter(params.chunks(max).map(|chunk| {
        count += 1;

        let mut futures_data = FuturesData{
            remaining_params: Vec::new(),
            found_params: Vec::new(),
        };

        let found_params: &Vec<FoundParameter> = found_params;
        let cloned_diffs = Arc::clone(&shared_diffs);
        let cloned_green_lines = Arc::clone(&shared_green_lines);

        async move {
            let request = Request::new(request_defaults, chunk.iter().map(|x| x.to_string()).collect::<Vec<String>>());
            let response =
                match request.clone()
                    .send()
                    .await {
                    Ok(val) => val,
                    Err(_) => match Request::new_random(request_defaults, chunk.len())
                                .send()
                                .await {
                                    //we don't return the actual response because it was a random request without original parameters
                                    //instead we return an empty response from the original request
                                    Ok(_) => request.empty_response(),
                                    Err(err) => return Err(err)
                    }
            };

            //progress bar
            if config.verbose > 0 && !config.disable_progress_bar { //TODO maybe use external library
                write!(
                    io::stdout(),
                    "{} {}/{}       \r",
                    &"-> ".bright_yellow(),
                    count,
                    all
                ).ok();

                io::stdout().flush().ok();
            }

            if stable.reflections {
                let (reflected_parameter, repeat) = response.proceed_reflected_parameters();

                if reflected_parameter.is_some() {
                    let reflected_parameter = reflected_parameter.unwrap();

                    if !found_params.iter().any(|x| x.name == reflected_parameter) {
                        futures_data.found_params.push(
                            FoundParameter::new(reflected_parameter, &vec!["reflected".to_string()], "Different amount of reflections")
                        );

                        let mut msg = "reflects";
                        // explained in response.proceed_reflected_parameters() method
                        if chunk.len() == 2 {
                            msg = "not reflected one";
                        }

                        write_and_save(
                            &config,
                            &response,
                            format!(
                                "{}: {}",
                                msg.bright_blue(),
                                reflected_parameter
                            ),
                            &reflected_parameter
                        )?;
                    }
                }

                if repeat {
                    futures_data.remaining_params.append(&mut chunk.iter().map(|x| x.to_owned()).collect());
                    return Ok(futures_data)
                }

                if config.reflected_only {
                    return Ok(futures_data)
                }
            }

            if request_defaults.initial_response.as_ref().unwrap().code != response.code {

                if config.verbose > 1 {
                    writeln!(
                        io::stdout(),
                        "{} {}      ",
                        &response.code.to_string().bright_yellow(),
                        response.body.len()
                    ).ok();
                }

                let mut green_lines = cloned_green_lines.lock();

                //to prevent loops when ip got banned or server broke
                match green_lines.get(&response.code.to_string()) {
                    Some(val) => {
                        let n_val = *val;
                        green_lines.insert(response.code.to_string(), n_val + 1);
                        if n_val > 50 {
                            drop(green_lines);

                            let check_response =
                                Request::new_random(request_defaults, chunk.len())
                                .send()
                                .await?;

                            if check_response.code != request_defaults.initial_response.as_ref().unwrap().code {
                                return Err(format!("{} The page became unstable (code)", config.url))? // Bad
                            } else {
                                let mut green_lines = cloned_green_lines.lock();
                                green_lines.insert(response.code.to_string(), 0);
                            }
                        }
                    }
                    _ => {
                        green_lines.insert(response.code.to_string(), 0);
                    }
                }

                if chunk.len() == 1 {
                    write_and_save( //maybe to response method?
                        &config,
                        &response,
                        format!(
                            "{}: code {} -> {}",
                            &chunk[0],
                            request_defaults.initial_response.as_ref().unwrap().code, //TODO maybe different color on different codes
                            &response.code.to_string().bright_yellow(),
                        ),
                        &chunk[0]
                    )?;

                    futures_data.found_params.push(
                        FoundParameter::new(
                            &chunk[0],
                            &vec![format!("{} -> {}", request_defaults.initial_response.as_ref().unwrap().code, response.code)],
                            &format!("Changes code: {} -> {}", request_defaults.initial_response.as_ref().unwrap().code, response.code)
                        )
                    );
                } else {
                    futures_data.remaining_params.append(&mut chunk.to_vec());
                }
            }

            if stable.body {
                let mut diffs = cloned_diffs.lock();
                let (_, new_diffs) = response.compare(&diffs)?;

                //check whether the new_diff has at least 1 unique diff
                //and then check whether it's permament diff or not
                if !new_diffs.is_empty()  {
                    //the next function with .await will never return if something is locked
                    //so we need to unlock diffs firstly
                    drop(diffs);

                    //trying to catch false positives
                    let tmp_resp = Request::new_random(request_defaults, chunk.len())
                                                .send()
                                                .await?;

                    //lock it again
                    diffs = cloned_diffs.lock();

                    let (_, tmp_diffs) = tmp_resp.compare(&diffs)?;

                    for diff in tmp_diffs {
                        diffs.push(diff);
                    }
                }

                let mut green_lines = cloned_green_lines.lock();

                for diff in new_diffs.iter() {
                    if !diffs.contains(&diff) {

                        if config.verbose > 1 {
                            writeln!(
                                io::stdout(),
                                "{} {} ({})",
                                response.code,
                                &response.body.len().to_string().bright_yellow(),
                                &diff
                            ).ok();
                        }

                        //catch some often false-positive diffs within the FIRST cycle
                        match green_lines.get(diff) {
                            Some(val) => {
                                let n_val = *val;
                                //if there is one diff through 10 responses - it is a false positive one
                                if first {
                                    green_lines.insert(diff.to_string(), n_val + 1);
                                }
                                if n_val > 9 {
                                    diffs.push(diff.to_string())
                                }
                            }
                            _ => {
                                green_lines.insert(diff.to_string(), 0);
                            }
                        }

                        if chunk.len() == 1
                        && !found_params.iter().any(|x| x.name == chunk[0])
                        && !futures_data.found_params.iter().any(|x| x.name == chunk[0])  {

                            if config.strict {
                                if !found_params.iter().any(|x| x.diffs == new_diffs.join("|"))
                                || !futures_data.found_params.iter().any(|x| x.diffs == new_diffs.join("|")) {

                                    if config.verbose > 0 {
                                        writeln!(io::stdout(), "Removed: {}", chunk[0].bright_black()).ok();
                                    }

                                    break;
                                }
                            }

                            write_and_save(
                                &config,
                                &response,
                                format!(
                                    "{}: page {} -> {} ({})",
                                    &chunk[0],
                                    request_defaults.initial_response.as_ref().unwrap().body.len(),
                                    &response.body.len().to_string().bright_yellow(),
                                    &diff
                                ),
                                &chunk[0]
                            )?;

                            futures_data.found_params.push(
                                FoundParameter::new(
                                    &chunk[0],
                                    &new_diffs,
                                    &format!("Changes page: {} -> {}", request_defaults.initial_response.as_ref().unwrap().body.len(), response.body.len())
                                )
                            );
                            break;
                        } else {
                            futures_data.remaining_params.append(&mut chunk.to_vec());
                            break;
                        }
                    }
                }
            }
            Ok(futures_data)
        }
    }))
    .buffer_unordered(config.concurrency)
    .collect::<Vec<Result<FuturesData, Box<dyn Error>>>>()
    .await;

    for instance in futures_data {
        let mut instance = instance?;
        found_params.append(&mut instance.found_params);
        remaining_params.push(instance.remaining_params.iter().map(|x| x.to_string()).collect());
    }

    Ok(())
}