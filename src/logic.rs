use crate::{
    requests::{random_request, request},
    structs::{Config, ResponseData, Stable},
    utils::{check_diffs, make_hashmap, random_line, generate_request},
};
use colored::*;
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
    found_params: &mut Vec<String>,
) {
    let name1 = random_line(config.value_size);
    let name2 = random_line(config.value_size);
    let all = params.len() / max;

    for (count, chunk) in params.chunks(max).enumerate() {
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
                if !found_params.contains(param) {
                    found_params.push(param.to_string());
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
                        if config.verbose > 1 {
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
                found_params.push(not_reflected_one.to_owned());
            }

            if response.reflected_params.len() == 1 {
                found_params.push(chunk[0].to_owned());
            } else {
                remaining_params.push(chunk.to_vec());
            }
            continue;
        }

        if initial_response.code == response.code {
            if stable.body {
                let new_diffs = check_diffs(
                    config,
                    &initial_response.text,
                    &response.text,
                    &name1,
                    &name2,
                );

                for diff in new_diffs {
                    if !diffs.iter().any(|i| i == &diff) {
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

                        match green_lines.get(&diff) {
                            Some(val) => {
                                let n_val = *val;
                                if first || config.verbose == 0 {
                                    green_lines.insert(diff.to_string(), n_val + 1);
                                } else {
                                    //check whether the diff was stored or not
                                    let tmp_resp =
                                        random_request(&config, &client, reflections_count, max).await;

                                    let tmp_diffs = check_diffs(
                                        config,
                                        &initial_response.text,
                                        &tmp_resp.text,
                                        &name1,
                                        &name2,
                                    );

                                    for diff in tmp_diffs {
                                        if !diffs.iter().any(|i| i == &diff) {
                                            diffs.push(diff);
                                        }
                                    }
                                }

                                if n_val > 9 {
                                    diffs.push(diff.to_string())
                                }
                            }
                            _ => {
                                green_lines.insert(diff.to_string(), 0);
                            }
                        }

                        if chunk.len() == 1 && !found_params.contains(&chunk[0]) {
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
                            found_params.push(chunk[0].to_owned());
                            break;
                        } else {
                            remaining_params.push(chunk.to_vec());
                            break;
                        }
                    }
                }
            }
        } else if chunk.len() == 1 && !found_params.contains(&chunk[0]) {
            if config.verbose > 0 {
                writeln!(
                    io::stdout(),
                    "{}: code {} -> {}",
                    chunk[0],
                    initial_response.code,
                    &response.code.to_string().bright_yellow()
                ).ok();
            }
            found_params.push(chunk[0].to_owned());
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

            match green_lines.get(&response.code.to_string()) {
                Some(val) => {
                    let n_val = *val;
                    green_lines.insert(response.code.to_string(), n_val + 1);
                    if n_val > 50 {
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
                        }
                    }
                }
                _ => {
                    green_lines.insert(response.code.to_string(), 0);
                }
            }

            remaining_params.push(chunk.to_vec());
        }
    }
}