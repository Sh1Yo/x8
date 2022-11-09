use std::{
    collections::HashMap,
    error::Error,
    io::{self, Write},
};

use colored::Colorize;

use crate::network::utils::DataType;

use super::structs::Config;

/// shorcut to convert Option<&str> to Option<String> to be able to return it from the function
pub(super) fn convert_to_string_if_some(el: Option<&str>) -> Option<String> {
    if el.is_some() {
        Some(el.unwrap().to_string())
    } else {
        None
    }
}

/// parse request from the request file
pub(super) fn parse_request<'a>(
    request: &'a str,
    scheme: &str,
    port: u16,
    split_by: Option<&str>,
) -> Result<
    (
        Vec<String>,              // method
        Vec<String>,              // url
        HashMap<&'a str, String>, // headers
        String,                   // body
        Option<DataType>,
        Option<reqwest::Version>,         // http version
    ),
    Box<dyn Error>,
> {
    // request by lines
    let lines = if split_by.is_none() {
        request.lines().collect::<Vec<&str>>()
    } else {
        request
            .split(&split_by.unwrap().replace("\\r", "\r").replace("\\n", "\n"))
            .collect::<Vec<&str>>()
    };
    let mut lines = lines.iter();

    let mut data_type: Option<DataType> = None;
    let mut headers: HashMap<&'a str, String> = HashMap::new();
    let mut host = String::new();

    // parse the first line
    let mut firstline = lines.next().ok_or("Unable to parse firstline")?.split(' ');
    let method = firstline
        .next()
        .ok_or("Unable to parse method")?
        .to_string();
    let path = firstline.next().ok_or("Unable to parse path")?.to_string(); //include ' ' in path too?
    let http2 = firstline
        .next()
        .ok_or("Unable to parse http version")?
        .contains("HTTP/2");

    // parse headers
    while let Some(line) = lines.next() {
        if line.is_empty() {
            break;
        }

        let mut k_v = line.split(':');
        let key = k_v.next().ok_or("Unable to parse header key")?;
        let value: String = [
            k_v.next()
                .ok_or("Unable to parse header value")?
                .trim()
                .to_owned(),
            k_v.map(|x| ":".to_owned() + x).collect(),
        ]
        .concat();

        match key.to_lowercase().as_str() {
            "content-type" => {
                if value.contains("json") {
                    data_type = Some(DataType::Json)
                } else if value.contains("urlencoded") {
                    data_type = Some(DataType::Urlencoded)
                }
            }
            "host" => {
                host = value.clone();
                // host header in http2 breaks the h2 lib for now
                if http2 {
                    continue;
                }
            }
            // breaks h2 too
            // TODO maybe add an option to keep request as it is without removing anything
            "content-length" => continue,
            _ => (),
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
        if http2 { Some(http::Version::HTTP_2) } else { Some(http::Version::HTTP_11) }
    ))
}

pub fn write_banner_config(config: &Config, params: &Vec<String>) {
    let mut output = format!(
        "{}:         {}\n{}:      {}\n{}: {}",
        "urls".green(),
        config.urls.join(" "),
        "methods".blue(),
        config.methods.join(" "),
        "wordlist len".cyan(),
        params.len(),
    );

    if !config.proxy.is_empty() {
        output += &format!("\n{}:        {}", "proxy".green(), &config.proxy)
    }

    if !config.replay_proxy.is_empty() {
        output += &format!("\n{}: {}", "replay proxy".magenta(), &config.replay_proxy)
    }

    if config.recursion_depth != 0 {
        output += &format!(
            "\n{}: {}",
            "recursion depth".yellow(),
            &config.recursion_depth.to_string()
        )
    }

    writeln!(io::stdout(), "{}\n", output).ok();
}
