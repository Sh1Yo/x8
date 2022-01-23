use crate::{
    requests::{random_request, request},
    structs::{Config, ResponseData, Stable, FuturesData},
    utils::{compare, make_hashmap, random_line, generate_request},
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
pub async fn cycles(
    first: bool,
    config: &Config,
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
            found_params: HashMap::new()
        };

        let found_params: &HashMap<String, String> = &found_params;
        let cloned_diffs = Arc::clone(&shared_diffs);
        let cloned_green_lines = Arc::clone(&shared_green_lines);

        async move {

            let query = &make_hashmap(&chunk, config.value_size);
            let response = request(config, client, query, reflections_count).await;

            //progress bar
            if config.verbose > 0 && !config.disable_progress_bar {
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
            if stable.reflections && first && response.reflected_params.len() < 10 {
                for param in response.reflected_params.iter() {
                    if !found_params.contains_key(param) {
                        futures_data.found_params.insert(param.to_string(), String::from("Different amount of reflections"));
                        if config.verbose > 0 {
                            writeln!(
                                io::stdout(),
                                "{}: {}",
                                &"reflects".bright_blue(),
                                param
                            ).ok();
                        }
                    }
                }
            } else if stable.reflections && !response.reflected_params.is_empty() {
                //check whether there is one not reflected parameter between reflected ones
                let mut not_reflected_one: &str = &"";

                if chunk.len() - response.reflected_params.len() == 1 {
                    for el in chunk.iter() {
                        if !response.reflected_params.contains(el) {
                            not_reflected_one = el;
                            if config.verbose > 0 {
                                writeln!(
                                    io::stdout(),
                                    "{}: {}",
                                    &"not reflected one".bright_cyan(),
                                    &not_reflected_one
                                )
                                .ok();
                            }
                        }
                    }
                }

                if !not_reflected_one.is_empty() && chunk.len() >= 2 {
                    futures_data.found_params.insert(not_reflected_one.to_owned(), String::from("Causes other parameters to reflect different times"));
                }

                if response.reflected_params.len() == 1 {
                    futures_data.found_params.insert(chunk[0].to_owned(), String::from("Causes other parameters to reflect different times"));
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
                            random_request(&config, &client, reflections_count, max).await;

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
                            if !config.save_responses.is_empty() {
                                let mut output = generate_request(config, query);
                                output += &("\n\n--- response ---\n\n".to_owned() + &response.text);

                                match std::fs::write(
                                    &(config.save_responses.clone() + "/" + &random_line(10)),
                                    output,
                                ) {
                                    Ok(_) => (),
                                    Err(err) => {
                                        writeln!(
                                            io::stdout(),
                                            "Unable to write to {}/random_values due to {}",
                                            config.save_responses,
                                            err
                                        ).ok();
                                    }
                                }
                            }

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
                                    writeln!(
                                        io::stdout(),
                                        "{}: page {} -> {} ({})",
                                        chunk[0],
                                        initial_response.text.len(),
                                        &response.text.len().to_string().bright_yellow(),
                                        &diff
                                    ).ok();
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
                    writeln!(
                        io::stdout(),
                        "{}: code {} -> {}",
                        chunk[0],
                        initial_response.code,
                        &response.code.to_string().bright_yellow()
                    ).ok();
                }
                futures_data.found_params.insert(chunk[0].to_owned(), format!("Changes response code: {} -> {}", initial_response.code, response.code));
            } else {
                if !config.save_responses.is_empty() {
                    let filename = random_line(10);
                    let mut output = generate_request(config, query);
                    output += &("\n\n--- response ---\n\n".to_owned() + &response.text);

                    match std::fs::write(&(config.save_responses.clone() + "/" + &filename), output) {
                        Ok(_) => (),
                        Err(err) => {
                            writeln!(
                                io::stdout(),
                                "Unable to write to {}/random_values due to {}",
                                config.save_responses,
                                err
                            ).ok();
                        }
                    }

                    if config.verbose > 1 {
                        writeln!(
                            io::stdout(),
                            "{} {} and was saved as {}",
                            &response.code.to_string().bright_yellow(),
                            response.text.len(),
                            &filename
                        ).ok();
                    }
                } else if config.verbose > 1 {
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

                            let check_response = request(config, client, &query, 0).await;

                            if check_response.code != initial_response.code {
                                writeln!(
                                    io::stderr(),
                                    "[!] {} the page became unstable (code)",
                                    &config.url
                                ).ok();
                                std::process::exit(1)
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
        remaining_params.push(instance.remaining_params)
    }
}