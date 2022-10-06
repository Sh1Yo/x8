extern crate x8;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    error::Error, iter::FromIterator,
};

use atty::Stream;

use reqwest::Client;
use x8::{
    args::get_config,
    logic::check_parameters,
    network::request::{Request, RequestDefaults},
    structs::{Config, FoundParameter, ReasonKind},
    utils::{self, replay, empty_reqs, verify, write_banner, read_lines, read_stdin_lines, write_banner_response, create_output, create_client, random_line}, runner::runner::Runner, //runner::Runner,
};

#[cfg(windows)]
#[tokio::main]
async fn main() {
    colored::control::set_virtual_terminal(true).unwrap();
    std::process::exit(match init().await {
        Ok(_) => 0,
        Err(err) => {
            utils::error(err);
            1
        }
    });
}

#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    std::process::exit(match init().await {
        Ok(_) => 0,
        Err(err) => {
            utils::error(err);
            1
        }
    });
}

async fn init() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let (config, mut request_defaults, default_max): (Config, RequestDefaults, isize) = get_config()?;

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

    let replay_client = create_client(&config.replay_proxy, config.follow_redirects, &config.http, config.timeout)?;

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

    run(&config, &mut request_defaults, &replay_client, &mut params, default_max).await?;

    /*let output = create_output(&config, &request_defaults, all_found_params);

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
    write!(io::stdout(), "\n{}", &output).ok();*/

    Ok(())
}

async fn run(
    config: &Config, request_defaults: &mut RequestDefaults, replay_client: &Client, params: &mut Vec<String>, default_max: isize
) -> Result<(), Box<dyn Error>> {
    let runner = Runner::new(config, request_defaults, replay_client, default_max).await?;

    let found_params = runner.run(params).await?;

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

async fn _run(
    config: &Config,
    mut request_defaults: RequestDefaults,
    replay_client: &Client,
    mut params: Vec<String>,
    mut default_max: isize,
    first_run: bool
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {

    //let mut runner = Runner::new(config, request_defaults, replay_client, params, default_max);

    //saves false-positive diffs
    let mut green_lines: HashMap<String, usize> = HashMap::new();

    //default_max can be negative in case guessed automatically.
    let mut max = default_max.abs() as usize;


    //make first request and collect some information like code, reflections, possible parameters
    let cloned_request_defaults = request_defaults.clone();
    let mut initial_response = Request::new_random(&cloned_request_defaults, max)
                                            .send()
                                            .await?;

    //add possible parameters to the list of parameters in case the injection place is not headers
    /*if runner.request_defaults.injection_place != InjectionPlace::Headers {
        for param in initial_response.get_possible_parameters() {
            if !runner.params.contains(&param) {
                runner.params.push(param)
            }
        }
    }*/

    //in case the list is too small - change the max amount of parameters
    if params.len() < max {
        max = params.len();
        default_max = params.len() as isize;
        if max == 0 {
            Err("No parameters were provided.")?
        }
    };

    //TODO
    //initial_response.fill_reflected_parameters();

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

    if config.verbose > 0 && first_run {
        write_banner_response(&initial_response, request_defaults.amount_of_reflections, &params);
    }

    //make a few requests and collect all persistent diffs, check for stability
    let (mut diffs, stable) = empty_reqs(
        &config,
        &initial_response,
        &request_defaults,
        config.learn_requests_count,
        max,
    ).await?;

    if config.reflected_only && !stable.reflections {
        Err("Reflections are not stable")?;
    }

    //check whether it is possible to use 192 or 256 params in a single request instead of 128 default

    //parameters like admin=true
    let mut custom_parameters: HashMap<String, Vec<String>> = config.custom_parameters.clone();

    //remaining sets of parameters where new parameters can be found
    let mut remaining_params: Vec<Vec<String>> = Vec::new();

    //all found parameters
    let mut found_params: Vec<FoundParameter> = Vec::new();

    //whether it's the first run. Changes some logic in check_parameters function
    let mut first: bool = true;

    //the initial size of parameters' sets (the amount of requests to be send in the one loop iteration)
    let initial_size: usize = params.len() / max;

    //how many times subsets of the same parameters were checked
    //helps to detect infinity loops that can happen in rare cases
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
            &initial_response,
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

        //TODO move them somwhere else because they break recursion things
        //check custom parameters like admin=true
        if params.is_empty() && !config.disable_custom_parameters && first_run {
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
            = verify(&initial_response, &request_defaults, &found_params, &diffs, &stable).await {
            filtered_params
        } else {
            utils::info(&config, "~", "was unable to verify found parameters");
            found_params
        };
    }

    if !config.replay_proxy.is_empty() {
        if let Err(_) = replay(&config, &request_defaults, &replay_client, &found_params).await {
            utils::info(&config, "~", "was unable to resend found parameters via different proxy");
        }
    }

    Ok(found_params)
}