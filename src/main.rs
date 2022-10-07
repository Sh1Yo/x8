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
    network::request::{Request, RequestDefaults},
    structs::{Config, FoundParameter, ReasonKind, Parameters, Headers},
    utils::{self, replay, empty_reqs, verify, write_banner_config, read_lines, read_stdin_lines, write_banner_url, create_client, random_line}, runner::runner::Runner, //runner::Runner,
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

    let (mut config, mut request_defaults, default_max): (Config, RequestDefaults, isize) = get_config()?;

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

    //write banner
    if config.verbose > 0 && !config.test {
        write_banner_config(&config, &request_defaults, &params);
    }

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

    run(&mut config, &mut request_defaults, &replay_client, &mut params, default_max).await
}

async fn run(
    config: &mut Config, request_defaults: &mut RequestDefaults, replay_client: &Client, params: &mut Vec<String>, default_max: isize
) -> Result<(), Box<dyn Error>> {
    let runner = Runner::new(config, request_defaults, replay_client, default_max).await?;

    if config.verbose > 0 {
        write_banner_url(&runner.request_defaults, &runner.initial_response, runner.request_defaults.amount_of_reflections);
    }

    let mut runner_output = runner.run(params).await?;

    if !runner_output.found_params.is_empty() {
        for depth in 1..config.recursion_depth+1 {
            params.retain(|x| !runner_output.found_params.contains_key(x));

            //custom parameters work badly with recursion enabled
            config.disable_custom_parameters = true;

            //so we are keeping parameters that don't change pages' code
            //or change it to 200
            //we cant simply overwrite request_defaults.parameters because there's user-supplied parameters as well.
            request_defaults.parameters.append(&mut Vec::from_iter(
                runner_output.found_params.iter().filter(
                    |x|
                    !request_defaults.parameters.contains_key(&x.name)
                    &&
                    (x.reason_kind != ReasonKind::Code || x.status == 200)).map(|x| (x.get())
                )
            ));

            utils::info(config, "recursion", format!(
                "({}) repeating with {}", depth, request_defaults.parameters.iter().map(|x| x.0.as_str()).collect::<Vec<&str>>().join(", ")
            ));

            let mut new_found_params = Runner::new(config, request_defaults, replay_client, default_max).await?
                .run(params).await?.found_params;

            // no new params where found - just quit the loop
            if !new_found_params.iter().any(|x| !runner_output.found_params.contains_key(&x.name)) {
                break
            }

            runner_output.found_params.append(&mut new_found_params);
        }
    }

    //we probably changed request_defaults.parameters within the loop above
    //so we are removing all of the added parameters in there
    //leaving only user-supplied ones
    //(to not cause double parameters in some output types)
    request_defaults.parameters = request_defaults.parameters
        .iter()
        .filter(|x| !runner_output.found_params.contains_key(&x.0))
        .map(|x| x.to_owned())
        .collect();

    let output = runner_output.parse(config, request_defaults);//create_output(&config, &request_defaults, found_params);

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