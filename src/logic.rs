use crate::{
    requests::{random_request, request},
    structs::{Config, ResponseData, DefaultResponse, Stable, FuturesData, Statistic},
    utils::{compare, make_hashmap, random_line, save_request},
};
use colored::*;
use futures::stream::StreamExt;
use std::sync::Arc;
use parking_lot::Mutex;
use reqwest::Client;

use std::{
    collections::HashMap,
    io::{self, Write},
};

//check parameters in a loop chunk by chunk
pub async fn check_parameters(
    first: bool,
    config: &Config,
    stats: &mut Statistic,
    initial_response: &ResponseData,
    diffs: &mut Vec<String>,
    params: &[String],
    stable: &Stable,
    reflections_count: usize,
    client: &Client,
    max: usize,
    green_lines: &mut HashMap<String, usize>,
    remaining_params: &mut Vec<Vec<String>>,
    found_params: &mut HashMap<String, String>,
) {
    let all = params.len() / max;
    let mut count: usize = 0;
    let shared_diffs = Arc::new(Mutex::new(diffs));
    let shared_green_lines = Arc::new(Mutex::new(green_lines));

    let futures_data = futures::stream::iter(params.chunks(max).map(|chunk| {
        count += 1;
        let mut futures_data = FuturesData{
            remaining_params: Vec::new(),
            found_params: HashMap::new(),
            stats: Statistic{amount_of_requests: 0}
        };

        let found_params: &HashMap<String, String> = &found_params;
        let cloned_diffs = Arc::clone(&shared_diffs);
        let cloned_green_lines = Arc::clone(&shared_green_lines);

        async move {

            let query = &make_hashmap(&chunk, config.value_size);
            let response =
                request(config, &mut futures_data.stats, client, query, reflections_count)
                    .await
                    .unwrap_or(ResponseData::default());

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

            //try to find parameters with different number of reflections
            if stable.reflections && response.reflected_params.len() < 10 && response.reflected_params.len() != chunk.len() {
                for param in response.reflected_params.keys() {
                    if !found_params.contains_key(param) {
                        futures_data.found_params.insert(param.to_string(), String::from("Different amount of reflections"));

                        if config.verbose > 0 {
                            let mut output_message = format!(
                                "{}: {}",
                                &"reflects".bright_blue(),
                                param
                            );

                            if !config.save_responses.is_empty() {
                                output_message += &format!(" [saved to {}]", save_request(config, &query, &response, param));
                            }

                            writeln!(io::stdout(), "{}", output_message).ok();
                        } else if !config.save_responses.is_empty() {
                            save_request(config, &query, &response, param);
                        }
                    }
                }
            //if the amount of reflected parameters == the amount of send parameters - that means that sth went wrong
            //so we are trying to find a parameter that caused that
            } else if stable.reflections && !response.reflected_params.is_empty() {
                let mut not_reflected_one: &str = &"";

                //saves the number of occurencies for each number of reflections
                //key: the number of reflections
                let mut amount_of_reflections: HashMap<usize, usize> = HashMap::new();

                for (_, v) in &response.reflected_params {
                    if amount_of_reflections.contains_key(&v) {
                        amount_of_reflections.insert(*v, amount_of_reflections[v] + 1);
                    } else {
                        amount_of_reflections.insert(*v, 1);
                    }
                }

                //tries to find the unique parameter - the parameter with the unique number of reflections
                //example:
                // <input name="sth1&sth2&sth3" value="sth1&sth2&sth3" type="sth4"> -> sth4 is the unique reflection
                // <div data="sth1">sth1&sth2&sth3</div> -> sth1 is the unique reflection

                let unique_ones = amount_of_reflections.iter().filter(|x| x.1 == &1).collect::<HashMap<&usize, &usize>>();
                if unique_ones.len() == 1 {
                    not_reflected_one = response
                        .reflected_params
                        .iter()
                        .find(|(_, reflections)| reflections == unique_ones.iter().next().unwrap().0)
                        .unwrap()
                        .0;

                    if config.verbose > 0 {
                        let mut output_message = format!(
                            "{}: {}",
                            &"not reflected one".bright_cyan(),
                            &not_reflected_one
                        );

                        if !config.save_responses.is_empty() {
                            output_message += &format!(" [saved to {}]", save_request(config, &query, &response, not_reflected_one));
                        }

                        writeln!(io::stdout(), "{}", output_message).ok();
                    } else if !config.save_responses.is_empty() {
                        save_request(config, &query, &response, not_reflected_one);
                    }
                }

                //if we found that parameter that caused others to reflect differently:
                if !not_reflected_one.is_empty() {
                    futures_data.found_params.insert(not_reflected_one.to_owned(), String::from("Causes other parameters to reflect different times"));
                //in case we didn't find the unique parameter - check parameters till we find it or there is only one left
                } else {
                    futures_data.remaining_params.append(&mut chunk.to_vec());
                }
                return futures_data
            }

            if config.reflected_only {
                return futures_data
            }

            if initial_response.code == response.code {
                if stable.body {
                    let (_, new_diffs) = compare(
                        initial_response,
                        &response,
                    );
                    let mut diffs = cloned_diffs.lock();

                    //check whether the new_diff has at least 1 unique diff
                    //and then check whether it was stored or not
                    if !new_diffs.iter().all(|i| diffs.contains(i))  {
                        //the next function with .await will never return if something is locked
                        //so we need to unlock diffs firstly
                        drop(diffs);

                        let tmp_resp =
                            random_request(&config, &mut futures_data.stats, &client, reflections_count, max)
                            .await
                            .unwrap_or(ResponseData::default());

                        //lock it again
                        diffs = cloned_diffs.lock();

                        let (_, tmp_diffs) = compare(
                            initial_response,
                            &tmp_resp,
                        );

                        for diff in tmp_diffs {
                            if !diffs.iter().any(|i| i == &diff) {
                                diffs.push(diff);
                            }
                        }
                    }

                    let mut green_lines = cloned_green_lines.lock();

                    for diff in new_diffs {
                        if !diffs.contains(&diff) {

                            if config.verbose > 1 {
                                writeln!(
                                    io::stdout(),
                                    "{} {} ({})",
                                    response.code,
                                    &response.text.len().to_string().bright_yellow(),
                                    &diff
                                ).ok();
                            }

                            //catch some often false-positive diffs within the FIRST cycle
                            match green_lines.get(&diff) {
                                Some(val) => {
                                    let n_val = *val;
                                    //if there is one diff through 10 responses - it is a false positive one
                                    if first || config.verbose == 0 {
                                        green_lines.insert(diff.to_string(), n_val + 1);
                                    } else if n_val > 9 {
                                        diffs.push(diff.to_string())
                                    }
                                }
                                _ => {
                                    green_lines.insert(diff.to_string(), 0);
                                }
                            }

                            if chunk.len() == 1 && !found_params.contains_key(&chunk[0]) && !futures_data.found_params.contains_key(&chunk[0]) {

                                if config.verbose > 0 {
                                    let mut output_message = format!(
                                        "{}: page {} -> {} ({})",
                                        &chunk[0],
                                        initial_response.text.len(),
                                        &response.text.len().to_string().bright_yellow(),
                                        &diff
                                    );

                                    if !config.save_responses.is_empty() {
                                        output_message += &format!(" [saved to {}]", save_request(config, query, &response, &chunk[0]));
                                    }

                                    writeln!(io::stdout(), "{}", output_message).ok();
                                } else if !config.save_responses.is_empty() {
                                    save_request(config, query, &response, &chunk[0]);
                                }

                                futures_data.found_params.insert(chunk[0].to_owned(), format!("Changes page: {} -> {}", initial_response.text.len(), response.text.len()));
                                break;
                            } else {
                                futures_data.remaining_params.append(&mut chunk.to_vec());
                                break;
                            }
                        }
                    }
                }
            } else if chunk.len() == 1 && !found_params.contains_key(&chunk[0]) && !futures_data.found_params.contains_key(&chunk[0]) {

                if config.verbose > 0 {
                    let mut output_message = format!(
                        "{}: code {} -> {}",
                        &chunk[0],
                        initial_response.code,
                        &response.code.to_string().bright_yellow()
                    );

                    if !config.save_responses.is_empty() {
                        output_message += &format!(" [saved to {}]", save_request(config, query, &response, &chunk[0]));
                    }

                    writeln!(io::stdout(), "{}", output_message).ok();
                } else if !config.save_responses.is_empty() {
                    save_request(config, query, &response, &chunk[0]);
                }

                futures_data.found_params.insert(chunk[0].to_owned(), format!("Changes response code: {} -> {}", initial_response.code, response.code));
            } else {
               if config.verbose > 1 {
                    writeln!(
                        io::stdout(),
                        "{} {}      ",
                        &response.code.to_string().bright_yellow(),
                        response.text.len()
                    ).ok();
                }

                futures_data.remaining_params.append(&mut chunk.to_vec());

                let mut green_lines = cloned_green_lines.lock();

                //to prevent loops when ip got banned or server broke
                match green_lines.get(&response.code.to_string()) {
                    Some(val) => {
                        let n_val = *val;
                        green_lines.insert(response.code.to_string(), n_val + 1);
                        if n_val > 50 {
                            drop(green_lines);

                            let mut random_params: Vec<String> = Vec::new();

                            for _ in 0..max {
                                random_params.push(random_line(config.value_size));
                            }

                            let query = make_hashmap(
                                &random_params[..],
                                config.value_size,
                            );

                            let check_response =
                                request(config, &mut futures_data.stats, client, &query, 0)
                                    .await
                                    .unwrap_or(ResponseData::default());

                            if check_response.code != initial_response.code {
                                writeln!(
                                    io::stderr(),
                                    "[!] {} the page became unstable (code)",
                                    &config.url
                                ).ok();
                                std::process::exit(1) //TODO return error instead
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
            }
            futures_data
        }
    }))
    .buffer_unordered(config.concurrency)
    .collect::<Vec<FuturesData>>()
    .await;

    for instance in futures_data {
        for (k, v) in instance.found_params {
            found_params.insert(k, v);
        }
        remaining_params.push(instance.remaining_params);
        stats.merge(instance.stats);
    }
}