use crate::{
    structs::{Config, Stable, FuturesData, Request, RequestDefaults, FoundParameter, ReasonKind, Response}, utils::{progress_bar, self},
};
use futures::stream::StreamExt;
use std::{sync::Arc, error::Error};
use parking_lot::Mutex;

use std::{
    collections::HashMap,
};

/// check parameters in a loop chunk by chunk
/// probably way better to make it recursive
/// but looks like it's not that easy to make recursive async funcs in rust that call other async funcs..
pub async fn check_parameters(
    first: bool,
    config: &Config,
    request_defaults: &RequestDefaults,
    diffs: &mut Vec<String>,
    params: &Vec<String>,
    stable: &Stable,
    max: usize,
    green_lines: &mut HashMap<String, usize>,
    remaining_params: &mut Vec<Vec<String>>,
    found_params: &mut Vec<FoundParameter>,
    initial_response: &Response,
) -> Result<(), Box<dyn Error>> {
    //the amount of requests needed for process all the parameters
    let all = params.len() / max;
    let mut count: usize = 0;

    //make diffs and green_lines accessable by all futures
    let shared_diffs = Arc::new(Mutex::new(diffs));
    let shared_green_lines = Arc::new(Mutex::new(green_lines));
    let shared_found_params = Arc::new(Mutex::new(found_params));

    let futures_data = futures::stream::iter(params.chunks(max).map(|chunk| {
        count += 1;

        let mut futures_data = FuturesData{
            remaining_params: Vec::new(),
            found_params: Vec::new(),
        };

        //let found_params: &Vec<FoundParameter> = found_params;
        let cloned_diffs = Arc::clone(&shared_diffs);
        let cloned_green_lines = Arc::clone(&shared_green_lines);
        let cloned_found_params = Arc::clone(&shared_found_params);

        async move {
            let request = Request::new(request_defaults, chunk.iter().map(|x| x.to_string()).collect::<Vec<String>>());
            let mut response =
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

            progress_bar(config, count, all);

            if stable.reflections {
                response.fill_reflected_parameters(initial_response);

                let (reflected_parameter, repeat) = response.proceed_reflected_parameters();

                if reflected_parameter.is_some() {
                    let reflected_parameter = reflected_parameter.unwrap();

                    let mut found_params = cloned_found_params.lock();
                    if !found_params.iter().any(|x| x.name == reflected_parameter) {

                        let mut kind = ReasonKind::Reflected;
                        // explained in response.proceed_reflected_parameters() method
                        // chunk.len() == 1 and not 2 because the random parameter appends later
                        if chunk.len() == 1 {
                            kind = ReasonKind::NotReflected;
                        }

                        found_params.push(
                            FoundParameter::new(
                                reflected_parameter, &vec!["reflected".to_string()], "Different amount of reflections", kind.clone()
                            )
                        );

                        drop(found_params);

                        response.write_and_save(config, initial_response, kind, &reflected_parameter, None)?;
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

            if initial_response.code != response.code {

                utils::notify(config, ReasonKind::Code, &response, None);

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

                            if check_response.code != initial_response.code {
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
                    response.write_and_save(config, initial_response, ReasonKind::Code, &chunk[0], None)?;

                    let mut found_params = cloned_found_params.lock();
                    found_params.push(
                        FoundParameter::new(
                            &chunk[0],
                            &vec![format!("{} -> {}", initial_response.code, response.code)],
                            &format!("Changes code: {} -> {}", initial_response.code, response.code),
                            ReasonKind::Code
                        )
                    );
                } else {
                    futures_data.remaining_params.append(&mut chunk.to_vec());
                }

            } else if stable.body {
                let mut diffs = cloned_diffs.lock();
                let (_, new_diffs) = response.compare(initial_response, &diffs)?;

                //check whether the new_diff has at least 1 unique diff
                //and then check whether it's permament diff or not
                if !new_diffs.is_empty()  {

                    if config.strict {
                        let found_params = cloned_found_params.lock();
                        if found_params.iter().any(|x| x.diffs == new_diffs.join("|")) {
                            return Ok(futures_data);
                        }
                    }

                    //the next function with .await will never return if something is locked
                    //so we need to unlock diffs firstly
                    drop(diffs);

                    //trying to catch false positives
                    let tmp_resp = Request::new_random(request_defaults, chunk.len())
                                                .send()
                                                .await?;

                    //lock it again
                    diffs = cloned_diffs.lock();

                    let (_, tmp_diffs) = tmp_resp.compare(initial_response, &diffs)?;

                    for diff in tmp_diffs {
                        diffs.push(diff);
                    }
                }

                let mut green_lines = cloned_green_lines.lock();

                for diff in new_diffs.iter() {
                    if !diffs.contains(&diff) {

                        utils::notify(config, ReasonKind::Text, &response, Some(&diff));

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

                        let mut found_params = cloned_found_params.lock();
                        if chunk.len() == 1
                        && !found_params.iter().any(|x| x.name == chunk[0]) {

                            //we need to repeat this because futures are fast and a few same parameters can already be here
                            if config.strict {
                                if found_params.iter().any(|x| x.diffs == new_diffs.join("|")) {
                                    return Ok(futures_data);
                                }
                            }

                            response.write_and_save(config, initial_response, ReasonKind::Text, &chunk[0], Some(&diff))?;

                            found_params.push(
                                FoundParameter::new(
                                    &chunk[0],
                                    &new_diffs,
                                    &format!("Changes page: {} -> {}", initial_response.text.len(), response.text.len()),
                                    ReasonKind::Text
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
        let instance = instance?;
        //found_params.append(&mut instance.found_params);
        remaining_params.push(instance.remaining_params.iter().map(|x| x.to_string()).collect());
    }

    Ok(())
}