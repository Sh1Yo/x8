extern crate x8;
use colored::*;
use reqwest::Client;
use atty::Stream;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    time::Duration,
};
use x8::{
    args::get_config,
    logic::check_parameters,
    requests::{empty_reqs, random_request, request},
    structs::{Config, Statistic, ResponseData, DefaultResponse},
    utils::{compare, generate_data, heuristic, make_hashmap, random_line, read_lines, read_stdin_lines, create_output},
};

#[cfg(windows)]
#[tokio::main]
async fn main() {
    colored::control::set_virtual_terminal(true).unwrap();
    run().await;
}

#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    run().await;
}

async fn run() {
    //colored::control::set_override(true);

    let mut stats = Statistic{amount_of_requests: 0};

    //saves false-positive diffs
    let mut green_lines: HashMap<String, usize> = HashMap::new();

    let (config, mut max): (Config, usize) = get_config();
    if config.verbose > 0 && !config.test {
        writeln!(
            io::stdout(),
            " _________  __ ___     _____\n|{} {}",
            &config.method.blue(),
            &config.url.green(),
        ).ok();
    }

    if !config.proxy.is_empty() && config.verbose > 0 && !config.test {
        writeln!(
            io::stdout(),
            "|{} {}",
            "Proxy".magenta(),
            &config.proxy.green(),
        ).ok();
    }

    if !config.save_responses.is_empty() {
        match fs::create_dir_all(&config.save_responses) {
            Ok(_) => (),
            Err(err) => {
                writeln!(
                    io::stderr(),
                    "Unable to create a directory '{}' due to {}",
                    &config.save_responses,
                    err
                ).unwrap_or(());
                std::process::exit(1);
            }
        };
    }

    let mut params: Vec<String> = Vec::new();

    if !config.wordlist.is_empty() {
        //read parameters from a file
        if let Ok(lines) = read_lines(&config.wordlist) {
            for line in lines.flatten() {
                params.push(line);
            }
        }
    //just accept piped stdin
    } else if !atty::is(Stream::Stdin) {
        //read parameters from stdin
        params = read_stdin_lines();
    }

    //build clients
    let mut client = Client::builder()
        //.resolve("localhost", "127.0.0.1".parse().unwrap())
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(config.timeout as u64))
        .connect_timeout(Duration::from_secs(config.timeout as u64))
        .http1_title_case_headers()
        .cookie_store(true)
        .use_rustls_tls();

    if !config.proxy.is_empty() {
        client = client.proxy(reqwest::Proxy::all(&config.proxy).unwrap());
    }
    if !config.follow_redirects {
        client = client.redirect(reqwest::redirect::Policy::none());
    }

    let client = client.build().unwrap();

    let mut replay_client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(config.timeout as u64))
        .connect_timeout(Duration::from_secs(config.timeout as u64))
        .http1_title_case_headers()
        .cookie_store(true)
        .use_rustls_tls();

    if !config.replay_proxy.is_empty() {
        replay_client = replay_client.proxy(reqwest::Proxy::all(&config.replay_proxy).unwrap());
    }
    if !config.follow_redirects {
        replay_client = replay_client.redirect(reqwest::redirect::Policy::none());
    }

    let replay_client = replay_client.build().unwrap();

    //generate random query for the first request
    let query = make_hashmap(
        &(0..max).map(|_| random_line(config.value_size*2)).collect::<Vec<String>>(),
        config.value_size,
    );

    //get cookies
    request(&config, &mut stats, &client, &HashMap::new(), 0).await;

    // if opened in the test mode - generate request/response and quit
    if config.test {
        match generate_data(&config, &mut stats, &client, &query).await {
            Some(()) => (),
            None => {
                writeln!(io::stderr(), "Unable to connect to the server").ok();
            }
        };
        return
    }

    // make first request and collect some information like code, reflections, possible parameters
    let mut initial_response =
        match request(&config, &mut stats, &client, &query, 0)
            .await {
                Some(val) => val,
                None => {
                    writeln!(io::stderr(), "Unable to connect to the server").ok();
                    return
                }
    };

    if !config.headers_discovery {
        for param in heuristic(&initial_response.text) {
            if !params.contains(&param) {
                params.push(param)
            }
        }
    }

    if params.len() < max {
        max = params.len();
        if max == 0 {
            params.push(String::from("something"));
            max = 1;
        }
    }

    initial_response.reflected_params = HashMap::new();

    //let reflections count = the number of reflections of the first parameter
    let reflections_count = initial_response
        .text
        .to_ascii_lowercase()
        .matches(&query.values().next().unwrap().replace("%random%_", "").as_str())
        .count() as usize;

    if config.verbose > 0 {
        writeln!(
            io::stdout(),
            "|{} {}\n|{} {}\n|{} {}\n|{} {}\n",
            &"Code".magenta(),
            &initial_response.code.to_string().green(),
            &"Response Len".magenta(),
            &initial_response.text.len().to_string().green(),
            &"Reflections".magenta(),
            &reflections_count.to_string().green(),
            &"Words".magenta(),
            &params.len().to_string().green(),
        ).ok();
    }

    //make a few requests and collect all persistent diffs, check for stability
    let (mut diffs, stable) = empty_reqs(
        &config,
        &mut stats,
        &initial_response,
        reflections_count,
        config.learn_requests_count,
        &client,
        max,
    ).await;

    if config.reflected_only && !stable.reflections {
        writeln!(io::stderr(), "{} Reflections are not stable", config.url).ok();
        return
    }

    //check whether it is possible to use 192(128) or 256(196) params in a single request instead of 128 default
    if max == 128 || max == 64 {
        let response =
            match random_request(&config, &mut stats, &client, reflections_count, max + 64)
                .await {
                    Some(val) => val,
                    None => {
                        writeln!(io::stderr(), "The server is not stable").ok();
                        return
                    }
        };

        let (is_code_the_same, new_diffs) = compare(&initial_response, &response);
        let mut is_the_body_the_same = true;

        for diff in new_diffs.iter() {
            if !diffs.iter().any(|i| i == diff) {
                is_the_body_the_same = false;
            }
        }

        if is_code_the_same && (!stable.body || is_the_body_the_same) {
            let response =
                match random_request(&config, &mut stats, &client, reflections_count, max + 128)
                    .await {
                        Some(val) => val,
                        None => {
                            writeln!(io::stderr(), "The server is not stable").ok();
                            return
                        }
            };

            let (is_code_the_same, new_diffs) = compare(&initial_response, &response);

            for diff in new_diffs {
                if !diffs.iter().any(|i| i == &diff) {
                    is_the_body_the_same = false;
                }
            }

            if is_code_the_same && (!stable.body || is_the_body_the_same) {
                max += 128
            } else {
                max += 64
            }
            if config.verbose > 0 {
                writeln!(
                    io::stdout(),
                    "[#] the max number of parameters in every request was increased to {}",
                    max
                ).ok();
            }
        }
    }

    let mut custom_parameters: HashMap<String, Vec<String>> = config.custom_parameters.clone();
    let mut remaining_params: Vec<Vec<String>> = Vec::new();
    let mut found_params: HashMap<String, String> = HashMap::new();
    let mut first: bool = true;
    let initial_size: usize = params.len() / max;
    let mut count: usize = 0;

    loop {
        check_parameters(
            first,
            &config,
            &mut stats,
            &initial_response,
            &mut diffs,
            &params,
            &stable,
            reflections_count,
            &client,
            max,
            &mut green_lines,
            &mut remaining_params,
            &mut found_params,
        ).await;
        first = false;
        count += 1;

        if count > 100
            || (count > 50 && remaining_params.len() < 10)
            || (count > 10 && remaining_params.len() > (initial_size / 2 + 5))
            || (count > 1 && remaining_params.len() > (initial_size * 2 + 10))
        {
            writeln!(io::stderr(), "{} Infinity loop detected", config.url).ok();
            return
        }

        params = Vec::with_capacity(remaining_params.len() * max);
        max /= 2;

        if max == 0 {
            max = 1;
        }

        //if there is a parameter in remaining_params that also exists in found_params - ignore it.
        let mut found: bool = false;
        for vector_params in &remaining_params {
            for param in vector_params {
                for found_param in found_params.keys() {
                    //some strange logic in order to treat admin=1 and admin=something as the same parameters
                    let param_key = if param.matches('=').count() == 1 {
                        param.split('=').next().unwrap()
                    } else {
                        param
                    };

                    if found_param == param_key
                        || found_param.matches('=').count() == 1
                        && found_param.split('=').next().unwrap() == param_key {
                        found = true;
                        break;
                    }
                }
                if !found {
                    params.push(param.to_string());
                }
                found = false;
            }
        }

        if params.is_empty() && !config.disable_custom_parameters {
            max = config.max;
            for (k, v) in custom_parameters.iter_mut() {
                if !v.is_empty() {
                    params.push([k.as_str(), "=", v.pop().unwrap().as_str()].concat());
                }
            }
            if max > params.len() {
                max = params.len()
            }
        }

        if params.is_empty() {
            break;
        }

        remaining_params = Vec::new()
    }

    if config.verify {
        let mut filtered_params = HashMap::with_capacity(found_params.len());
        for (param, reason) in found_params {

            let response = request(
                &config,
                &mut stats,
                 &client,
                &make_hashmap(
                    &[param.clone()], config.value_size
                ),
                reflections_count
            ).await.unwrap_or(ResponseData::default());

            let (is_code_the_same, new_diffs) = compare(&initial_response, &response);
            let mut is_the_body_the_same = true;

            for diff in new_diffs.iter() {
                if !diffs.iter().any(|i| i==diff) {
                    is_the_body_the_same = false;
                }
            }

            if !response.reflected_params.is_empty() || !is_the_body_the_same || !is_code_the_same {
                filtered_params.insert(param, reason);
            }
        }
        found_params = filtered_params;
    }

    if !config.replay_proxy.is_empty() {
        let temp_config = Config{
            disable_cachebuster: true,
            ..config.clone()
        };

        request(&temp_config, &mut stats, &replay_client, &HashMap::new(), 0).await;

        if config.replay_once {
            request(
                &temp_config,
                &mut stats,
                &replay_client,
                &make_hashmap(
                    &found_params.keys().map(|x| x.to_owned()).collect::<Vec<String>>(),
                    config.value_size
                ),
                0
            ).await;
        } else {
            for param  in found_params.keys() {
                request(
                    &temp_config,
                    &mut stats,
                    &replay_client,
                    &make_hashmap(
                        &[param.to_owned()],
                        config.value_size
                    ),
                    0
                ).await;
            }
        }
    }

    if config.verbose > 0 {
        writeln!(io::stdout(),"\n{}: {}", &"Amount of requests".magenta(), stats.amount_of_requests).ok();
    }

    let output = create_output(&config, &stats, found_params);

    if !config.output_file.is_empty() {
        let mut file = OpenOptions::new();

        let file = if config.append {
            file.write(true).append(true)
        } else {
            file.write(true).truncate(true)
        };

        let mut file = match file.open(&config.output_file) {
            Ok(file) => file,
            Err(_) => match fs::File::create(&config.output_file) {
                Ok(file) => file,
                Err(err) => {
                    writeln!(io::stderr(), "[!] Unable to create file - {}", err).ok();
                    write!(io::stdout(), "\n{}", &output).ok();
                    return
                }
            }
        };

        match write!(file, "{}" , output) {
            Ok(_) => (),
            Err(err) => {
                writeln!(io::stderr(), "[!] {}", err).ok();
            }
        };
    }
    write!(io::stdout(), "\n{}", &output).ok();
}