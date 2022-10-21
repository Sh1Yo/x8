extern crate x8;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    error::Error, iter::FromIterator
};

use atty::Stream;

use futures::StreamExt;
use x8::{
    args::get_config,
    network::{request::{Request, RequestDefaults}, headers::Headers},
    structs::Config,
    runner::{runner::Runner, found_parameters::{ReasonKind, Parameters}},
    utils::{self, write_banner_config, read_lines, read_stdin_lines, write_banner_url},
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
            utils::error(err, None);
            1
        }
    });
}

async fn init() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let config: Config = get_config()?;

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

    //write banner
    if config.verbose > 0 && !config.test {
        write_banner_config(&config, &params);
    }

    futures::stream::iter(config.urls.iter().map(|url| {

        //each url should have each own list of parameters
        let params = params.clone();

        //each url should have it's own immutable pointer to config
        let config = &config;

        async move {
            for method in &config.methods.clone() {

                //each method should have each own list of parameters (we're changing this list through the run)
                let mut params = params.clone();

                let mut request_defaults = RequestDefaults::from_config(config, method.as_str(), url.as_str())?;

                //get cookies
                Request::new(&request_defaults, Vec::new())
                    .send()
                    .await?;

                //if --test option is used - print request/response and quit
                if config.test {
                    //TODO move to func
                    writeln!(
                        io::stdout(),
                        "{}",
                        Request::new(&request_defaults, Vec::new())
                            .send()
                            .await?
                            .print_all()
                    ).ok();
                } else {

                    run(config, &mut request_defaults, &mut params).await?;

                }
            };

            Ok::<(), Box<dyn Error>>(())
        }
    }))
    .buffer_unordered(config.threads)
    .map(|x| {
        //TODO make a custom error type or something so we can access the actual url from here..
        if x.is_err() {
            utils::error(x.as_ref().err().unwrap(), Some("url"));
        }
        x
    })
    .collect::<Vec<Result<(), Box<dyn Error>>>>()
    .await;

    Ok(())
}

async fn run(
    config: &Config, request_defaults: &mut RequestDefaults, params: &mut Vec<String>
) -> Result<(), Box<dyn Error>> {
    let runner = Runner::new(config, request_defaults).await?;

    if config.verbose > 0 {
        write_banner_url(&runner.request_defaults, &runner.initial_response, runner.request_defaults.amount_of_reflections);
    }

    let mut runner_output = runner.run(params).await?;

    if !runner_output.found_params.is_empty() {
        for depth in 1..config.recursion_depth+1 {
            params.retain(|x| !runner_output.found_params.contains_name(x));

            //custom parameters work badly with recursion enabled
            request_defaults.disable_custom_parameters = true;

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

            let mut new_found_params = Runner::new(config, request_defaults).await?
                .run(params).await?.found_params;

            // no new params where found - just quit the loop
            if !new_found_params.iter().any(|x| !runner_output.found_params.contains_name(&x.name)) {
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
        .filter(|x| !runner_output.found_params.contains_name(&x.0))
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