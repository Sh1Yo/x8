use crate::structs::{Config, Response, DataType, InjectionPlace, RequestDefaults, Request, Stable};

use lazy_static::lazy_static;
use percent_encoding::{AsciiSet, CONTROLS};
use rand::Rng;
use colored::*;
use std::error::Error;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, Write},
    path::Path,
};

lazy_static! {
    static ref FRAGMENT: AsciiSet = CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'&')
        .add(b'#')
        .add(b';')
        .add(b'/')
        .add(b'=')
        .add(b'%');

    static ref RANDOM_CHARSET: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
}

pub fn write_banner(config: &Config, request_defaults: &RequestDefaults) {
    writeln!(
        io::stdout(),
        " _________  __ ___     _____\n|{} {}",
        &request_defaults.method.blue(),
        &config.url.green(),
    ).ok();

    if !config.proxy.is_empty() {
        writeln!(
            io::stdout(),
            "|{} {}",
            "Proxy".magenta(),
            &config.proxy.green(),
        ).ok();
    }
}

pub fn write_banner_response(initial_response: &Response, reflections_count: usize, params: &Vec<String>) {
    writeln!(
        io::stdout(),
        "|{} {}\n|{} {}\n|{} {}\n|{} {}\n",
        &"Code".magenta(),
        &initial_response.code.to_string().green(),
        &"Response Len".magenta(),
        &initial_response.body.len().to_string().green(),
        &"Reflections".magenta(),
        &reflections_count.to_string().green(),
        &"Words".magenta(),
        &params.len().to_string().green(),
    ).ok();
}

/// checks whether increasing the amount of parameters changes the page
/// returns the max possible amount of parameters that possible to send without changing the page
pub async fn try_to_increase_max<'a>(
    request_defaults: &RequestDefaults<'a>, diffs: &Vec<String>, mut max: usize, stable: &Stable
) -> Result<usize, Box<dyn Error>> {
    let response = Request::new_random(&request_defaults, max + 64)
                                .send()
                                .await?;

    let (is_code_the_same, new_diffs) = response.compare(&diffs)?;
    let mut is_the_body_the_same = true;

    if !new_diffs.is_empty() {
        is_the_body_the_same = false;
    }

    //in case the page isn't different from previous one - try to increase max amount of parameters by 128
    if is_code_the_same && (!stable.body || is_the_body_the_same) {
        let response =  Request::new_random(&request_defaults, max + 128)
                .send()
                .await?;

        let (is_code_the_same, new_diffs) = response.compare(&diffs)?;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        if is_code_the_same && (!stable.body || is_the_body_the_same) {
            max += 128
        } else {
            max += 64
        }

    }

    Ok(max)
}

pub fn parse_request<'a>(request: &'a str, as_body: bool) -> Result<(
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
    let mut injection_place: InjectionPlace = InjectionPlace::Path;
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

pub fn create_output(config: &Config, request_defaults: &RequestDefaults, found_params: HashMap<String, String>) -> String {

    //for internal methods like .url()
    let mut req = Request::new(request_defaults, found_params.keys().map(|x| x.to_owned()).collect());

    match config.output_format.as_str() {
        "url" => {
            let url = req.url();

            //make line an url with injection point
            let line = if !found_params.is_empty() && !url.contains("%s")  {
                if !url.contains("?") {
                    url + "%s"
                } else {
                    url + "?%s"
                }
            } else {
                url
            };

            (line+"\n").replace("%s", &req.make_query())
        }
        //TODO maybe use external lib :think:
        //don't want to use serde for such a simple task
        "json" => {
            format!(
                r#"{{"method": "{}", "url": "{}", "parameters": [{}]}}"#,
                &request_defaults.method,
                &config.url,
                found_params
                    .iter()
                    .map(|(name, reason)| format!(r#""name": "{}", "reason": "{}""#, name.replace("\"", "\\\""), reason))
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        },

        "request" => {
            req.print()+"\n"
        },

        _ => {
            format!(
                "{} {} % {}\n",
                &request_defaults.method,
                &config.url,
                found_params.keys().map(|x| x.as_str()).collect::<Vec<&str>>().join(", ")
            )
        },
    }
}

//writes request and response to a file
//return file location
pub fn save_request(config: &Config, response: &Response, param_key: &str) -> Result<String, Box<dyn Error>> {
    let output = response.print();

    let filename = format!(
        "{}/{}-{}-{}-{}", &config.save_responses, response.request.defaults.host, response.request.method.to_lowercase(), param_key, random_line(3)
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