use crate::requests::request;
use crate::structs::{Config, ResponseData, Statistic};
use crate::diff::diff;

use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use rand::Rng;
use regex::Regex;
use reqwest::Client;
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
    static ref RE_JSON_WORDS_WITHOUT_QUOTES: Regex =
        Regex::new(r#"^(\d+|null|false|true)$"#).unwrap();
    static ref RE_JSON_BRACKETS: Regex =
        Regex::new(r#"(?P<bracket>(\{"|"\}|\[("|\d)|("|\d)\]))"#).unwrap();
    static ref RE_JSON_COMMA_AFTER_DIGIT: Regex =
        Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>\d+),"#).unwrap();
    static ref RE_JSON_COMMA_AFTER_BOOL: Regex =
        Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>(false|null|true)),"#).unwrap();

    static ref RANDOM_CHARSET: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
}

//calls check_diffs & returns code and found diffs
pub fn compare(
    initial_response: &ResponseData,
    response: &ResponseData,
) -> (bool, Vec<String>) {

    let mut code: bool = true;
    let mut diffs: Vec<String> = Vec::new();

    if initial_response.code != response.code {
        code = false
    }

    //just push every found diff to the vector of diffs
    for diff in match diff(
        &initial_response.text,
        &response.text,
    ) {
        Ok(val) => val,
        Err(err) => {
            writeln!(io::stderr(), "Unable to compare: {}", err).ok(); //TODO return error instead
            std::process::exit(1);
        }
    } {
        if !diffs.contains(&diff) {
            diffs.push(diff);
        } else {
            let mut c = 1;
            while diffs.contains(&[&diff, "(", &c.to_string(), ")"].concat()) {
                c += 1
            }
            diffs.push([&diff, " (", &c.to_string(), ")"].concat());
        }
    }

    (code, diffs)
}

//get possible parameters from the page code
pub fn heuristic(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();

    let re_special_chars = Regex::new(r#"[\W]"#).unwrap();

    let re_name = Regex::new(r#"(?i)name=("|')?"#).unwrap();
    let re_inputs = Regex::new(r#"(?i)name=("|')?[\w-]+"#).unwrap();
    for cap in re_inputs.captures_iter(body) {
        found.push(re_name.replace_all(&cap[0], "").to_string());
    }

    let re_var = Regex::new(r#"(?i)(var|let|const)\s+?"#).unwrap();
    let re_full_vars = Regex::new(r#"(?i)(var|let|const)\s+?[\w-]+"#).unwrap();
    for cap in re_full_vars.captures_iter(body) {
        found.push(re_var.replace_all(&cap[0], "").to_string());
    }

    let re_words_in_quotes = Regex::new(r#"("|')[a-zA-Z0-9]{3,20}('|")"#).unwrap();
    for cap in re_words_in_quotes.captures_iter(body) {
        found.push(re_special_chars.replace_all(&cap[0], "").to_string());
    }

    let re_words_within_objects = Regex::new(r#"[\{,]\s*[[:alpha:]]\w{2,25}:"#).unwrap();
    for cap in re_words_within_objects.captures_iter(body){
        found.push(re_special_chars.replace_all(&cap[0], "").to_string());
    }

    found.sort();
    found.dedup();
    found
}

//remove forbidden characters from header name, otherwise reqwest throws errors
pub fn fix_headers<'a>(header: &'a str) -> Option<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[^!-'*+\-\.0-9a-zA-Z^-`|~]").unwrap();
    }

    if RE.is_match(header) {
        Some(RE.replace_all(header, "").to_string())

    // hyper throws an error in case the Content-Length header contains random value with http2
    } else if header.to_ascii_lowercase() == "content-length" {
        Some(String::from("disabled"))
    } else {
        None
    }
}

pub fn generate_request(config: &Config, initial_query: &HashMap<String, String>) -> String {
    let mut hashmap_query: HashMap<String, String> = HashMap::with_capacity(initial_query.len());
    for (k, v) in initial_query.iter() {
        hashmap_query.insert(k.to_string(), v.replace("%random%_", ""));
    }

    let query: String = if !hashmap_query.is_empty() {
        if config.as_body {
            make_body(&config, &hashmap_query)
        } else if config.within_headers {
            make_header_value(&config, &hashmap_query)
        } else if config.headers_discovery {
            String::new()
        } else {
            make_query(&config, &hashmap_query)
        }
    } else {
        String::new()
    };

    let mut req: String = String::with_capacity(4096);
    req.push_str(&config.url);
    req.push('\n');
    req.push_str(&config.method);
    req.push(' ');
    req.push_str(&config.path.replace("%s", &query));


    req.push_str(" HTTP/1.1\n");

    if !config.headers.keys().any(|i| i.contains("Host")) {
        req.push_str(&("Host: ".to_owned() + &config.host));
        req.push('\n');
    }

    for (key, value) in config.headers.iter() {
        req.push_str(key);
        req.push_str(": ");
        if value.contains("%s") && config.headers_discovery && config.within_headers {
            req.push_str(&value.replace("%s", &query).replace("{{random}}", &random_line(config.value_size)));
        } else {
            req.push_str(&value.replace("{{random}}", &random_line(config.value_size)));
        }
        req.push('\n');
    }

    if config.headers_discovery && !config.within_headers {
        for (key, value) in hashmap_query.iter() {
            req.push_str(key);
            req.push_str(": ");
            req.push_str(&value.replace("{{random}}", &random_line(config.value_size)));
            req.push('\n');
        }
    }

    if config.as_body && !query.is_empty() {
        req.push('\n');
        req.push_str(&query);
        req.push('\n');
    }

    req
}

//prints request and response
pub async fn generate_data(config: &Config, stats: &mut Statistic, client: &Client, query: &HashMap<String, String>) -> Option<()> {
    let req = generate_request(config, query);

    writeln!(io::stdout(), "Request:\n{}", req).ok();

    let response =
        request(config, stats, client, &query, 0)
            .await?;

    writeln!(
        io::stdout(),
        "Response:\nCode: {}\n\n{}",
        response.code,
        response.text
    ).ok();

    writeln!(
        io::stdout(),
        "Possible parameters: {}",
        heuristic(&response.text).join(", ")
    ).ok();

    Some(())
}

//Add %s if it is absent in the body
pub fn adjust_body(body: &str, t: &str) -> String {
    let mut body = body.to_string();

    //if type is json and body has parameters -> add an injection to the end
    if t.contains("json") && !body.is_empty() && body.contains('"') {
        body.pop();
        body.push_str(", %s}");
        body
    } else if t.contains("json") {
        String::from("{%s}")
    //suppose that body type is urlencode
    } else if !body.is_empty()  {
        body.push_str("&%s");
        body
    } else {
        body.push_str("%s");
        body
    }
}

pub fn make_header_value(config: &Config, query: &HashMap<String, String>) -> String {
    make_query(config, query)
}

pub fn make_body(config: &Config, query: &HashMap<String, String>) -> String {
    let mut body: String = String::new();

    for (k, v) in query {
        if config.body_type.contains("json") && RE_JSON_WORDS_WITHOUT_QUOTES.is_match(v) {
            body.push_str(&config.parameter_template.replace("%k", k).replace("\"%v\"", v));
        } else {
            body.push_str(&config.parameter_template.replace("%k", k).replace("%v", v));
        }
    }

    if config.body_type.contains("json") {
        body.truncate(body.len().saturating_sub(2)) //remove the last ', '
    }

    body = match config.encode {
        true =>config.body.replace("%s", &utf8_percent_encode(&body, &FRAGMENT).to_string()).replace("{{random}}", &random_line(config.value_size)),
        false => config.body.replace("%s", &body).replace("{{random}}", &random_line(config.value_size))
    };

    body
}

pub fn make_query(config: &Config, params: &HashMap<String, String>) -> String {
    let mut query: String = String::new();

    for (k, v) in params {
        query = query + &config.parameter_template.replace("%k", k).replace("%v", v);
    }
    query.pop();

    if config.encode {
        utf8_percent_encode(&query, &FRAGMENT).to_string()
    } else {
        query
    }
}

//"param" -> param:random_value
//"param=value" -> param:value
pub fn make_hashmap(
    words: &[String],
    value_size: usize,
) -> HashMap<String, String> {
    let mut hashmap: HashMap<String, String> = HashMap::new();

    for word in words.iter() {
        let (param, value) = if word.matches('=').count() == 1 {
            let mut splitted = word.split('=');
            (splitted.next().unwrap(), splitted.next().unwrap().to_string())
        } else {
            (word.as_str(), "%random%_".to_owned()+&random_line(value_size))
        };

        hashmap.insert(param.to_string(), value);
    }

    hashmap
}

pub fn parse_request(config: Config, proto: &str, request: &str, custom_parameter_template: bool) -> Option<Config> {
    let mut lines = request.lines();
    let mut host = String::new();
    let mut content_type = String::new();
    let mut headers: HashMap<String, String> = config.headers.clone();
    let mut within_headers: bool = config.within_headers;
    let mut firstline = lines.next()?.split(' ');
    let method = firstline.next()?.to_string();
    let mut path = firstline.next()?.to_string();

    let http2: bool = firstline.next()?.to_string().contains("HTTP/2");

    //read headers
    while let Some(line) = lines.next() {
        if line.is_empty() {
            break;
        }

        let mut k_v = line.split(':');
        let key = k_v.next()?;
        let value: String = [
            k_v.next()?.trim().to_owned(),
            k_v.map(|x| ":".to_owned() + x).collect(),
        ].concat();

        if value.contains("%s") {
            within_headers = true;
        }

        match key.to_lowercase().as_str() {
            "content-type" => content_type = value.clone(),
            "host" => {
                host = value.clone();
                if http2 {
                    continue
                }
            },
            "content-length" => continue,
            _ => ()
        };

        headers.insert(key.to_string(), value);
    }

    let mut parameter_template = if !custom_parameter_template {
        if config.within_headers {
            String::from("%k=%v; ")
        } else {
            match config.body_type == "json" {
                true => String::from("\"%k\":\"%v\", "),
                false => String::from("%k=%v&")
            }
        }
    } else {
        config.parameter_template.clone()
    };

    let mut body = lines.next().unwrap_or("").to_string();
    while let Some(part) = lines.next() {
        if !part.is_empty() {
            body.push_str("\r\n");
            body.push_str(part);
        }
    }

    //check whether the body type can be json
     //TODO check whether can be combined with the same check in args.rs
    let body_type = if config.body_type.contains('-') && config.as_body && !custom_parameter_template
    && (
        content_type.contains("json") || (!body.is_empty() && body.starts_with('{') )
    ) {
        parameter_template = String::from("\"%k\":\"%v\", ");
        String::from("json-")
    } else {
        config.body_type
    };

    //if --as-body is specified and body is empty or lacks injection points - add an injection point
    let body = if config.as_body && ((!body.is_empty() && !body.contains("%s")) || body.is_empty()) {
        adjust_body(&body, &body_type)
    } else {
        body
    };

    let mut url = [proto,"://", &host, &path].concat();
    let initial_url = url.clone();

    if !config.as_body && url.contains('?') && !within_headers && !config.headers_discovery && url.contains('=') && !url.contains("%s") {
        url.push_str("&%s");
        path.push_str("&%s");
    } else if !config.as_body && !within_headers && !config.headers_discovery {
        url.push_str("?%s");
        path.push_str("?%s");
    }

    Some(Config {
        method,
        url,
        host,
        path,
        headers,
        within_headers,
        body,
        body_type,
        parameter_template,
        initial_url,
        ..config
    })
}

pub fn random_line(size: usize) -> String {
    (0..size)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0,RANDOM_CHARSET.len());
            RANDOM_CHARSET[idx] as char
        })
        .collect()
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

pub fn create_output(config: &Config, stats: &Statistic, found_params: HashMap<String, String>) -> String {
    match config.output_format.as_str() {
        "url" => {
            let mut line = if !found_params.is_empty() {
                match config.initial_url.contains('?') {
                    true => config.initial_url.to_owned()+"&",
                    false => config.initial_url.to_owned()+"?"
                }
            } else {
                config.initial_url.clone()
            };

            if !found_params.is_empty() {

                for (param, _) in &found_params {
                    line.push_str(&param);
                    if !param.contains('=') {
                        line.push('=');
                        line.push_str(&random_line(config.value_size));
                    }
                    line.push('&')
                }

                line.pop();
            }

            line.push('\n');

            line
        }
        "json" => {
            let mut line = format!(
                "{{\"method\":\"{}\", \"url\":\"{}\", \"parameters\":[",
                &config.method,
                &config.initial_url
            );

            if !found_params.is_empty() {

                for (param, reason) in &found_params {
                    line.push_str(&format!("{{\"name\":\"{}\", \"reason\":\"{}\"}}, ", param, reason));
                }

                line = line[..line.len() - 2].to_string();
            }

            line.push_str(&format!("], \"amount_of_requests\":{}}}\n", stats.amount_of_requests));

            line
        },
        "request" => {
            generate_request(config, &make_hashmap(&found_params.keys().map(|x| x.to_owned()).collect::<Vec<String>>(), config.value_size))
                .lines()
                .skip(1)
                .collect::<Vec<&str>>()
                .join("\n") + "\n"
        },
        _ => {
            let mut line = format!("{} {} % ", &config.method, &config.initial_url);

            if !found_params.is_empty() {

                for (param, _) in &found_params {
                    line.push_str(&param);
                    line.push_str(", ")
                }

                line = line[..line.len() - 2].to_string();
            }

            line.push('\n');
            line
        },
    }
}

//writes request and response to a file
//return file location
pub fn save_request(config: &Config, query: &HashMap<String, String>, response: &ResponseData, param_key: &str) -> String {
    let output = format!(
        "{}\n\n--- response ---\nCode: {}\n{}",
        generate_request(config, query),
        &response.code,
        &response.text
    );

    let filename = format!(
        "{}/{}-{}-{}-{}", &config.save_responses, config.host, config.method.to_lowercase(), param_key, random_line(3)
    );

    match std::fs::write(
        &filename,
        output,
    ) {
        Ok(_) => (),
        Err(err) => {
            writeln!(
                io::stderr(),
                "Unable to save request - {}",
                err
            ).ok();
        }
    }

    return filename
}

//beautify json before comparing responses
pub fn beautify_json(json: &str) -> String {
    let json = json.replace("\\\"", "'");
    let json = json.replace("\",", "\",\n");
    let json = RE_JSON_BRACKETS.replace_all(&json, "${bracket}\n");
    let json = RE_JSON_COMMA_AFTER_DIGIT.replace_all(&json, "$first:$second,\n");
    let json = RE_JSON_COMMA_AFTER_BOOL.replace_all(&json, "$first:$second,\n");
    json.to_string()
}

//same with html
pub fn beautify_html(html: &str) -> String {
    html.replace(">", ">\n")
}