use std::{
    fs::File,
    collections::HashMap,
    error::Error,
    io::{self, BufRead, Write},
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
    port: Option<u16>,
    split_by: Option<&str>,
) -> Result<
    (
        Vec<String>,              // method
        Vec<String>,              // url
        Vec<(String, String)>,    // headers
        String,                   // body
        Option<DataType>,
        Option<reqwest::Version>, // http version
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
    let mut headers: Vec<(String, String)> = Vec::new();
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

        headers.push((key.to_string(), value));
    }

    let mut body = lines.next().unwrap_or(&"").to_string();
    while let Some(part) = lines.next() {
        if !part.is_empty() {
            body.push_str("\r\n");
            body.push_str(part);
        }
    }

    // port from the --port argument has a priority against port within the host header
    let (host, port) = if port.is_some() {
       (host.split(':').next().unwrap().to_string(), port.unwrap())
    } else if port.is_none() && host.contains(':') {
        let mut host = host.split(':');
        (host.next().unwrap().to_string(), host.next().unwrap().parse()?)
    } else {
        // neither --port nor port within the host header were specified
        if scheme == "http" {
            (host.to_string(), 80u16)
        } else {
            (host.to_string(), 443u16)
        }
    };

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

pub fn read_urls_if_possible(filename: &str) -> Result<Option<Vec<String>>, io::Error> {
    let file = match File::open(filename) {
        Ok(file) => file,
        Err(_) => return Ok(None)
    };

    let mut urls = Vec::new();

    for url in io::BufReader::new(file).lines() {
        urls.push(url?);
    }

    Ok(Some(urls))
}

pub(super) fn add_default_headers(curr_headers: HashMap<&str, String>) -> Vec<(String, String)> {
    let default_headers = [
        ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 12) AppleWebKit/601.3.9 (KHTML, like Gecko) Version/9.0.2 Firefox/99.0"),
        ("Accept", "*/*"),
        ("Accept-Encoding", "gzip, deflate")
    ];

    let mut headers = Vec::new();

    for (k, v) in default_headers {
        if !curr_headers.keys().any(|i| i.contains(k)) {
            headers.push((k.to_string(), v.to_string()))
        }
    }

    curr_headers.iter().map(|(k, v)| headers.push((k.to_string(), v.to_string()))).for_each(drop);

    headers
}

pub(super) fn mimic_browser_headers(curr_headers: HashMap<&str, String>) -> Vec<(String, String)> {
    let browser_headers = [
        ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 12) AppleWebKit/601.3.9 (KHTML, like Gecko) Version/9.0.2 Firefox/99.0"),
        ("Accept", "*/*"), // TODO maybe get from file extension as browsers do
        ("Accept-Language", "en-US;q=0.7,en;q=0.3"),
        ("Accept-Encoding", "gzip, deflate"),
        ("Dnt", "1"),
        ("Upgrade-Insecure-Requests", "1"),
        ("Sec-Fetch-Dest", "document"),
        ("Sec-Fetch-Mode", "navigate"),
        ("Sec-Fetch-Site", "same-site")
    ];

    let mut headers = Vec::new();

    for (k, v) in browser_headers {
        if !curr_headers.keys().any(|i| i.contains(k)) {
            headers.push((k.to_string(), v.to_string()))
        }
    }

    curr_headers.iter().map(|(k, v)| headers.push((k.to_string(), v.to_string()))).for_each(drop);

    headers
}