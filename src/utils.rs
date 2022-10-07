use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, Write},
    path::Path,
    error::Error,
    time::Duration,
};

use rand::Rng;
use colored::*;
use reqwest::Client;

use crate::structs::{Config, DataType, InjectionPlace, Stable, FoundParameter, ReasonKind};
use crate::network::{request::{RequestDefaults, Request}, response::Response};

static RANDOM_CHARSET: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const MAX_PAGE_SIZE: usize = 25 * 1024 * 1024; //25MB usually

pub fn write_banner_config(config: &Config, request_defaults: &RequestDefaults, params: &Vec<String>) {
    let mut output = format!("wordlist len: {}", params.len().to_string().blue());

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

pub fn write_banner_url(request_defaults: &RequestDefaults, initial_response: &Response, amount_of_reflections: usize) {
    writeln!(
        io::stdout(),
        "{} {} ({}) [{}] {{{}}}",
        &request_defaults.method.blue(),
        &request_defaults.url().green(),
        &initial_response.code(),
        &initial_response.text.len().to_string().green(),
        &amount_of_reflections.to_string().magenta()
    ).ok();
}

/// notify about found parameters
pub fn notify(config: &Config, reason_kind: ReasonKind, response: &Response, diffs: Option<&String>) {
    if config.verbose > 1 {
        match reason_kind {
            ReasonKind::Code => writeln!(
                io::stdout(),
                "{} {}     ", //a few spaces to remove some chars of progress bar
                response.code(),
                response.text.len()
            ).unwrap_or(()),
            ReasonKind::Text => writeln!(
                io::stdout(),
                "{} {} ({})", //a few spaces to remove some chars of progress bar
                response.code,
                response.text.len().to_string().bright_yellow(),
                diffs.unwrap()
            ).unwrap_or(()),
            _ => unreachable!()
        }
    }
}

pub fn info<S: Into<String>, T: std::fmt::Display>(config: &Config, word: S, msg: T) {
    if config.verbose > 0 {
        writeln!(io::stdout(), "[{}] {}", word.into().yellow(), msg).ok();
    }
}

pub fn error<T: std::fmt::Display>(msg: T) {
    writeln!(io::stderr(), "{} {}", "[#]".red(), msg).ok();
}

pub fn progress_bar(config: &Config, count: usize, all: usize) {
    if config.verbose > 0 && !config.disable_progress_bar {
        write!(
            io::stdout(),
            "{} {}/{}         \r",
            &"-> ".bright_yellow(),
            count,
            all
        ).ok();

        io::stdout().flush().ok();
    }
}

pub fn parse_request<'a>(request: &'a str, invert: bool) -> Result<(
    String, //method
    String, //host
    String, //path
    HashMap<&'a str, String>, //headers
    String, //body
    Option<DataType>,
    InjectionPlace,
), Box<dyn Error>> {
    //request by lines
    //TODO maybe add option whether split lines only by '\r\n' instead of splitting by '\n' as well.
    let mut lines = request.lines();

    let mut data_type: Option<DataType> = None;
    let mut headers: HashMap<&'a str, String> = HashMap::new();
    let mut host = String::new();

    //parse the first line
    let mut firstline = lines.next().ok_or("Unable to parse firstline")?.split(' ');
    let method = firstline.next().ok_or("Unable to parse method")?.to_string();
    let path = firstline.next().ok_or("Unable to parse path")?.to_string(); //include ' ' in path too?
    let http2 = firstline.next().ok_or("Unable to parse http version")?.contains("HTTP/2");

    //this behavior is explained within the --invert option help's line
    let as_body =  ((method == "POST" || method == "PUT") && !invert)
    || (method != "POST" && method != "PUT" && invert);

    let mut injection_place: InjectionPlace = if as_body {
        InjectionPlace::Body
    } else {
        InjectionPlace::Path
    };

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

        if value.contains("%s") {
            injection_place = InjectionPlace::HeaderValue;
        }

        match key.to_lowercase().as_str() {
            "content-type" => if as_body {
                if value.contains("json") {
                    data_type = Some(DataType::Json)
                } else if value.contains("urlencoded") {
                    data_type = Some(DataType::Urlencoded)
                }
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

    let mut body = lines.next().unwrap_or("").to_string();
    while let Some(part) = lines.next() {
        if !part.is_empty() {
            body.push_str("\r\n");
            body.push_str(part);
        }
    }

    Ok((
        method,
        host,
        path,
        headers,
        body,
        data_type,
        injection_place
    ))
}

///makes first requests and checks page behavior
pub async fn empty_reqs(
    config: &Config,
    initial_response: &Response<'_>,
    request_defaults: &RequestDefaults,
    count: usize,
    max: usize,
) -> Result<(Vec<String>, Stable), Box<dyn Error>> {
    let mut stable = Stable {
        body: true,
        reflections: true,
    };
    let mut diffs: Vec<String> = Vec::new();

    for i in 0..count {
        let response =
            Request::new_random(request_defaults, max)
                .send()
                .await?;

        progress_bar(config, i, count);

        //do not check pages >25MB because usually its just a binary file or sth
        if response.text.len() > MAX_PAGE_SIZE && !config.force {
            Err("The page is too huge")?;
        }

        //TODO i think it works wrong
        if !response.reflected_parameters.is_empty() {
            stable.reflections = false;
        }

        let (is_code_diff, mut new_diffs) = response.compare(initial_response, &diffs)?;

        if is_code_diff {
            Err("The page is not stable (code)")?
        }

        diffs.append(&mut new_diffs);
    }

    //check the last time
    let response =
        Request::new_random(request_defaults, max)
            .send()
            .await?;

    //in case the page is still different from other random ones - the body isn't stable
    if !response.compare(initial_response, &diffs)?.1.is_empty() {
        info(config, "~", "The page is not stable (body)");
        stable.body = false;
    }

    Ok((diffs, stable))
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