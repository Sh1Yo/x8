use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, Write},
    path::Path,
    error::Error,
    time::Duration,
};

use indicatif::{ProgressBar, MultiProgress, ProgressStyle};
use rand::Rng;
use colored::*;
use reqwest::Client;

use crate::{structs::{Config, DataType, Stable, ParamPatterns}, RANDOM_CHARSET};
use crate::network::{request::{RequestDefaults, Request}, response::Response};
use crate::runner::found_parameters::{FoundParameter, ReasonKind};

pub fn write_banner_config(config: &Config, params: &Vec<String>) {
    let mut output = format!("urls: {}, methods: {}, wordlist len: {}", config.urls.len(), config.methods.join(" "), params.len().to_string().blue());

    if !config.proxy.is_empty() {
        output += &format!(", proxy: {}", &config.proxy.green())
    }

    if !config.replay_proxy.is_empty() {
        output += &format!(", replay proxy: {}", &config.proxy.magenta())
    }

    if config.recursion_depth != 0 {
        output += &format!(", recursion depth: {}", &config.recursion_depth.to_string().yellow())
    }

    writeln!(
        io::stdout(),
        "{}\n",
        output
    ).ok();
}

/// notify about found parameters
pub fn notify(progress_bar: &ProgressBar, config: &Config, reason_kind: ReasonKind, response: &Response, diffs: Option<&String>) {
    if config.verbose > 1 {
        match reason_kind {
            ReasonKind::Code => progress_bar.println(
            format!(
                    "{} {}",
                    response.code(),
                    response.text.len()
                )
            ),
            ReasonKind::Text => progress_bar.println(
            format!(
                    "{} {} ({})", //a few spaces to remove some chars of progress bar
                    response.code,
                    response.text.len().to_string().bright_yellow(),
                    diffs.unwrap()
                )
            ),
            _ => unreachable!()
        }
    }
}

/// prints informative messages/non critical errors
pub fn info<S: Into<String>, T: std::fmt::Display>(config: &Config, id: usize, progress_bar: &ProgressBar, word: S, msg: T) {
    if config.verbose > 0 {
        progress_bar.println(format!("{} [{}] {}", color_id(id), word.into().yellow(), msg));
    }
}

/// prints errors. Progress_bar may be null in case the error happened too early (before requests)
pub fn error<T: std::fmt::Display>(msg: T, url: Option<&str>, progress_bar: Option<&ProgressBar>) {
    let message = if url.is_none() {
        format!("{} {}", "[#]".red(), msg)
    } else {
        format!("{} [{}] {}", "[#]".red(), url.unwrap(), msg)
    };

    if progress_bar.is_none() {
        writeln!(io::stdout(), "{}", message).ok();
    } else {
        progress_bar.unwrap().println(message);
    }
}

pub fn parse_request<'a>(request: &'a str, scheme: &str, port: u16, split_by: Option<&str>) -> Result<(
    Vec<String>, //method
    Vec<String>, //url
    HashMap<&'a str, String>, //headers
    String, //body
    Option<DataType>,
), Box<dyn Error>> {
    //request by lines
    let lines = if split_by.is_none() {
        request.lines().collect::<Vec<&str>>()
    } else {
        request.split(split_by.unwrap()).collect::<Vec<&str>>()
    };
    let mut lines = lines.iter();

    let mut data_type: Option<DataType> = None;
    let mut headers: HashMap<&'a str, String> = HashMap::new();
    let mut host = String::new();

    //parse the first line
    let mut firstline = lines.next().ok_or("Unable to parse firstline")?.split(' ');
    let method = firstline.next().ok_or("Unable to parse method")?.to_string();
    let path = firstline.next().ok_or("Unable to parse path")?.to_string(); //include ' ' in path too?
    let http2 = firstline.next().ok_or("Unable to parse http version")?.contains("HTTP/2");

    //parse headers
    while let Some(line) = lines.next() {
        if line.is_empty() {
            break;
        }

        let mut k_v = line.split(':');
        let key = k_v.next().ok_or("Unable to parse header key")?;
        let value: String = [
            k_v.next().ok_or("Unable to parse header value")?.trim().to_owned(),
            k_v.map(|x| ":".to_owned() + x).collect(),
        ].concat();

        match key.to_lowercase().as_str() {
            //TODO don't forget to ignore it if the injection point decided to be in query
            "content-type" =>  if value.contains("json") {
                    data_type = Some(DataType::Json)
                } else if value.contains("urlencoded") {
                    data_type = Some(DataType::Urlencoded)
                },
            "host" => {
                host = value.clone();
                // host header in http2 breaks the h2 lib for now
                if http2 {
                    continue
                }
            },
            //breaks h2 too
            //TODO maybe add an option to keep request as it is without removing anything
            "content-length" => continue,
            _ => ()
        };

        headers.insert(key, value);
    }

    let mut body = lines.next().unwrap_or(&"").to_string();
    while let Some(part) = lines.next() {
        if !part.is_empty() {
            body.push_str("\r\n");
            body.push_str(part);
        }
    }

    Ok((
        vec![method],
        vec![format!("{}://{}:{}{}", scheme, host, port, path)],
        headers,
        body,
        data_type,
    ))
}

pub async fn verify<'a>(
    initial_response: &'a Response<'a>,
    request_defaults: &'a RequestDefaults,
    found_params: &Vec<FoundParameter>,
    diffs: &Vec<String>,
    stable: &Stable
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {
    //TODO maybe implement sth like similar patters? At least for reflected parameters
    //struct Pattern {kind: PatterKind, pattern: String}
    //
    //let mut similar_patters: HashMap<Pattern, Vec<String>> = HashMap::new();
    //
    //it would allow to fold parameters like '_anything1', '_anything2' (all that starts with _)
    //to just one parameter in case they have the same diffs
    //sth like a light version of --strict

    let mut filtered_params = Vec::with_capacity(found_params.len());

    for param in found_params {

        let mut response = Request::new(request_defaults, vec![param.name.clone()])
                                    .send()
                                    .await?;

        let (is_code_the_same, new_diffs) = response.compare(initial_response, &diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        response.fill_reflected_parameters(initial_response);

        if !is_code_the_same || !(!stable.body || is_the_body_the_same) || !response.reflected_parameters.is_empty() {
            filtered_params.push(param.clone());
        }
    }

    Ok(filtered_params)
}

// under development
pub async fn smart_verify(
    initial_response: &Response<'_>,
    request_defaults: &RequestDefaults,
    found_params: &Vec<FoundParameter>,
    diffs: &Vec<String>,
    stable: &Stable
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {
    let mut filtered_params = Vec::with_capacity(found_params.len());

    for param in found_params {
        let _param_patterns = ParamPatterns::get_patterns(&param.name);

        let mut response = Request::new(request_defaults, vec![param.name.clone()])
                                    .send()
                                    .await?;

        let (is_code_the_same, new_diffs) = response.compare(initial_response, &diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        response.fill_reflected_parameters(initial_response);

        if !is_code_the_same || !(!stable.body || is_the_body_the_same) || !response.reflected_parameters.is_empty() {
            filtered_params.push(param.clone());
        }
    }

    Ok(filtered_params)
}

pub async fn replay<'a>(
    config: &Config, request_defaults: &RequestDefaults, replay_client: &Client, found_params: &Vec<FoundParameter>
) -> Result<(), Box<dyn Error>> {
     //get cookies
    Request::new(request_defaults, vec![])
        .send_by(replay_client)
        .await?;

    if config.replay_once {
        Request::new(request_defaults, found_params.iter().map(|x| x.name.to_owned()).collect::<Vec<String>>())
            .send_by(replay_client)
            .await?;
    } else {
        for param in found_params {
            Request::new(request_defaults, vec![param.name.to_string()])
                .send_by(replay_client)
                .await?;
        }
    }

    Ok(())
}

/// returns last n chars of an url
pub fn fold_url(url: &str, n: usize) -> String {
    if url.len() <= n+2 {
        //we need to add some spaces to align the progress bars
        url.to_string() + &" ".repeat(2+n-url.len())
    } else {
        "..".to_owned()+&url[url.len()-n..].to_string()
    }
}

/// initialize progress bars for every url
pub fn init_progress(config: &Config) -> Vec<(String, ProgressBar)> {
    let mut url_to_progress = Vec::new();
    let m = MultiProgress::new();

    //we're creating an empty progress bar to make one empty line between progress bars and the tool's output
    let empty_line = m.add(ProgressBar::new(128));
    let sty = ProgressStyle::with_template(" ",).unwrap();
    empty_line.set_style(sty);
    empty_line.inc(1);
    url_to_progress.push((String::new(), empty_line));

    //append progress bars one after another and push them to url_to_progress
    for url in config.urls.iter() {
        let pb = m.insert_from_back(
                0,
                if config.disable_progress_bar || config.verbose < 1 {
                    ProgressBar::new(128)
                } else {
                    ProgressBar::hidden()
                }
        );

        url_to_progress.push((
            url.to_owned(),
            pb.clone()
        ));
    }

    url_to_progress
}

pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn read_stdin_lines() -> Vec<String> {
    let stdin = io::stdin();
    stdin.lock().lines().filter_map(|x| x.ok()).collect()
}


pub fn create_client(proxy: &str, follow_redirects: bool, http: &str, timeout: usize) -> Result<Client, Box<dyn Error>> {
    let mut client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(timeout as u64))
        .http1_title_case_headers()
        .cookie_store(true)
        .use_rustls_tls();

    if !proxy.is_empty() {
        client = client.proxy(reqwest::Proxy::all(proxy)?);
    }
    if !follow_redirects {
        client = client.redirect(reqwest::redirect::Policy::none());
    }

    if !http.is_empty() {
        match http {
            "1.1" =>  client = client.http1_only(),
            "2" => client = client.http2_prior_knowledge(),
            _ => writeln!(io::stdout(), "[#] Incorrect http version provided. The argument is ignored").unwrap_or(()),
        }
    }

    Ok(client.build()?)
}

/// writes request and response to a file
/// return file location
pub fn save_request(config: &Config, response: &Response, param_key: &str) -> Result<String, Box<dyn Error>> {
    let output = response.print();

    let filename = format!(
        "{}/{}-{}-{}-{}", &config.save_responses, &response.request.as_ref().unwrap().defaults.host, response.request.as_ref().unwrap().defaults.method.to_lowercase(), param_key, random_line(3)
    );

    std::fs::write(
        &filename,
        output,
    )?;

    Ok(filename)
}

pub fn random_line(size: usize) -> String {
    (0..size)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0,RANDOM_CHARSET.len());
            RANDOM_CHARSET[idx] as char
        })
        .collect()
}

pub fn color_id(id: usize) -> String {
    if id % 7 == 0 {
        id.to_string().white()
    } else if id % 6 == 0 {
        id.to_string().bright_red()
    } else if id % 5 == 0 {
        id.to_string().bright_cyan()
    } else if id % 4 == 0 {
        id.to_string().bright_blue()
    } else if id % 3 == 0 {
        id.to_string().yellow()
    } else if id % 2 == 0 {
        id.to_string().bright_green()
    } else if id % 1 == 0 {
        id.to_string().magenta()
    } else {
        unreachable!()
    }.to_string()
}