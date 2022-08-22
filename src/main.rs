extern crate x8;
use reqwest::Client;
use atty::Stream;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    time::Duration, error::Error,
};
use x8::{
    args::get_config,
    logic::check_parameters,
    requests::{empty_reqs, verify, replay},
    structs::{Config, RequestDefaults, Request, InjectionPlace, FoundParameter},
    utils::{write_banner, read_lines, read_stdin_lines, write_banner_response, try_to_increase_max, create_output},
};

#[cfg(windows)]
#[tokio::main]
async fn main() {
    colored::control::set_virtual_terminal(true).unwrap();
    std::process::exit(match run().await {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{:?}", err);
            1
        }
    });
}

#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    std::process::exit(match run().await {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{:?}", err);
            1
        }
    });
}

async fn run() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    //saves false-positive diffs
    let mut green_lines: HashMap<String, usize> = HashMap::new();

    let (config, mut request_defaults, mut default_max): (Config, RequestDefaults, isize) = get_config()?;
    //default_max can be negative in case guessed automatucally.
    let mut max = default_max.abs() as usize;

    if config.verbose > 0 && !config.test {
        write_banner(&config, &request_defaults);
    }

    if !config.save_responses.is_empty() {
        fs::create_dir_all(&config.save_responses)?;
    }

    let mut params: Vec<String> = Vec::new();

    if !config.wordlist.is_empty() {
        //read parameters from a file
        for line in read_lines(&config.wordlist)?.flatten() {
            params.push(line);
        }
    //just accept piped stdin
    } else if !atty::is(Stream::Stdin) {
        //read parameters from stdin
        params = read_stdin_lines();
    }

    let mut replay_client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(60))
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

    //get cookies
    Request::new(&request_defaults, Vec::new())
        .send()
        .await?;

    //if --test option is used - print request/response and quit
    if config.test {
        writeln!(
            io::stdout(),
            "{}",
            Request::new(&request_defaults, Vec::new())
                .send()
                .await?
                .print_all()
        ).ok();
        return Ok(())
    }

    //make first request and collect some information like code, reflections, possible parameters
    let cloned_request_defaults = request_defaults.clone();
    let mut initial_response = Request::new_random(&cloned_request_defaults, max)
                                            .send()
                                            .await?;

    //add possible parameters to the list of parameters in case the injection place is not headers
    if request_defaults.injection_place != InjectionPlace::Headers {
        for param in initial_response.get_possible_parameters() {
            if !params.contains(&param) {
                params.push(param)
            }
        }
    }

    //in case the list is too small - change the max amount of parameters
    if params.len() < max {
        max = params.len();
        default_max = params.len() as isize;
        if max == 0 {
            Err("No parameters were provided.")?
        }
    };

    initial_response.fill_reflected_parameters();

    //let reflections count = the number of reflections of the first parameter
    request_defaults.amount_of_reflections = if initial_response.reflected_parameters.len() == 0 {
        //by default reflection count is 0
        //that's why in case the initial_response.reflected_parameters is empty - every parameter was reflected 0 times
        0
    } else {
        //take the amount of reflections of the first parameter
        //the parameters were random so the amount of reflections of every parameter should be equal
        *initial_response.reflected_parameters.iter().next().unwrap().1
    };

    if config.verbose > 0 {
        write_banner_response(&initial_response, request_defaults.amount_of_reflections, &params);
    }

    request_defaults.initial_response = Some(initial_response);

    //make a few requests and collect all persistent diffs, check for stability
    let (mut diffs, stable) = empty_reqs(
        &config,
        &request_defaults,
        config.learn_requests_count,
        max,
    ).await?;

    if config.reflected_only && !stable.reflections {
        Err("Reflections are not stable")?;
    }

    //check whether it is possible to use 192 or 256 params in a single request instead of 128 default
    if default_max == -128  {
        max = try_to_increase_max(&request_defaults, &diffs, max, &stable).await?;

        if max != default_max.abs() as usize && config.verbose > 0 {
            writeln!(
                io::stdout(),
                "[#] the max amount of parameters in every request was increased to {}",
                max
            ).ok();
        }
    }

    let mut custom_parameters: HashMap<String, Vec<String>> = config.custom_parameters.clone();
    let mut remaining_params: Vec<Vec<String>> = Vec::new();
    let mut found_params: Vec<FoundParameter> = Vec::new();
    let mut first: bool = true;
    let initial_size: usize = params.len() / max;
    let mut count: usize = 0;

    loop {
        check_parameters(
            first,
            &config,
            &request_defaults,
            &mut diffs,
            &params,
            &stable,
            max,
            &mut green_lines,
            &mut remaining_params,
            &mut found_params,
        ).await?;
        first = false;
        count += 1;

        //some strange logic to detect infinity loops of requests
        if count > 100
            || (count > 50 && remaining_params.len() < 10)
            || (count > 10 && remaining_params.len() > (initial_size / 2 + 5))
            || (count > 1 && remaining_params.len() > (initial_size * 2 + 10))
        {
           Err("Infinity loop detected")?;
        }

        params = Vec::with_capacity(remaining_params.len() * max);
        max /= 2;

        if max == 0 {
            max = 1;
        }

        //if there is a parameter in remaining_params that also exists in found_params - ignore it.
        //TODO rewrite coz it looks a bit difficult
        let mut found: bool = false;
        for vector_remainig_params in remaining_params.iter() {
            for remaining_param in vector_remainig_params {
                for found_param in found_params.iter() {
                    //some strange logic in order to treat admin=1 and admin=something as the same parameters
                    let param_key = if remaining_param.matches('=').count() == 1 {
                        remaining_param.split('=').next().unwrap()
                    } else {
                        remaining_param
                    };

                    if found_param.name == param_key
                        || found_param.name.matches('=').count() == 1
                        && found_param.name.split('=').next().unwrap() == param_key {
                        found = true;
                        break;
                    }
                }
                if !found {
                    params.push(remaining_param.to_string());
                }
                found = false;
            }
        }

        //check custom parameters like admin=true
        if params.is_empty() && !config.disable_custom_parameters {
            max = default_max.abs() as usize;
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
        found_params = if let Ok(filtered_params)
            = verify(&request_defaults, &found_params, &diffs, &stable).await {
            filtered_params
        } else {
            if config.verbose > 0 {
                writeln!(io::stdout(),"[#] was unable to verify found parameters").ok();
            }
            found_params
        };
    }

    if !config.replay_proxy.is_empty() {
        if let Err(_) = replay(&config, &request_defaults, &replay_client, &found_params).await {
            if config.verbose > 0 {
                writeln!(io::stdout(),"[#] was unable to resend found parameters via different proxy").ok();
            }
        }
    }

    let output = create_output(&config, &request_defaults, found_params);

    if !config.output_file.is_empty() {
        let mut file = OpenOptions::new();

        let file = if config.append {
            file.write(true).append(true)
        } else {
            file.write(true).truncate(true)
        };

        let mut file = match file.open(&config.output_file) {
            Ok(file) => file,
            Err(_) => fs::File::create(&config.output_file)?
        };

        write!(file, "{}" , output)?;
    }
    write!(io::stdout(), "\n{}", &output).ok();

    Ok(())
}