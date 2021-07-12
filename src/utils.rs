use crate::requests::request;
use crate::structs::{Config, ResponseData};
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
            writeln!(io::stderr(), "Unable to compare: {}", err).ok();
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

    let re_words_in_quotes = Regex::new(r#"("|')\w{3,20}('|")"#).unwrap();
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

pub fn generate_request(config: &Config, initial_query: &HashMap<String, String>) -> String {
    let mut query: HashMap<String, String> = HashMap::with_capacity(initial_query.len());
    for (k, v) in initial_query.iter() {
        query.insert(k.to_string(), v.replace("%random%_", ""));
    }

    let mut req: String = String::with_capacity(1024);
    req.push_str(&config.url);
    req.push('\n');
    req.push_str(&config.method);
    req.push(' ');

    if !config.as_body {
        let mut query_string = String::new();
        for (k, v) in query.iter() {
            query_string.push_str(k);
            query_string.push('=');
            query_string.push_str(v);
            query_string.push('&');
        }
        query_string.pop(); //remove the last &

        query_string = if config.encode {
            utf8_percent_encode(&query_string, &FRAGMENT).to_string()
        } else {
            query_string
        };

        req.push_str(&config.path.replace("%s", &query_string));
    }

    if config.http2 {
        req.push_str(" HTTP/2\n");
    } else {
        req.push_str(" HTTP/1.1\n");
    }

    if !config.headers.keys().any(|i| i.contains("Host")) {
        req.push_str(&("Host: ".to_owned() + &config.host));
        req.push('\n');
    }

    for (key, value) in config.headers.iter() {
        req.push_str(key);
        req.push_str(": ");
        req.push_str(&value.replace("{{random}}", &random_line(config.value_size)));
        req.push('\n');
    }

    let body: String = if config.as_body && !query.is_empty() {
        make_body(&config, &query)
    } else {
        config.body.to_owned()
    };

    if !body.is_empty() {
        req.push('\n');
        if config.encode {
            req.push_str(&utf8_percent_encode(&body, &FRAGMENT).to_string())
        } else {
            req.push_str(&body);
        }
        req.push('\n');
    }

    req
}

//prints request and response
pub async fn generate_data(config: &Config, client: &Client, query: &HashMap<String, String>) {
    let req = generate_request(config, query);

    writeln!(io::stdout(), "Request:\n{}", req).ok();

    let response = request(config, client, &query, 0).await;

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

    body = config.body.replace("%s", &body).replace("{{random}}", &random_line(config.value_size));

    if config.encode {
        utf8_percent_encode(&body, &FRAGMENT).to_string()
    } else {
        body
    }
}

pub fn make_query(params: &HashMap<String, String>, config: &Config) -> String {
    let mut query: String = String::from("");

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

pub fn parse_request(insecure: bool, request: &str, config: Config) -> Option<Config> {
    let mut lines = request.lines();
    let mut host = String::new();
    let mut content_type = String::new();
    let mut headers: HashMap<String, String> = config.headers.clone();
    let proto = if insecure { "http://" } else { "https://" };
    let mut firstline = lines.next()?.split(' ');
    let method = firstline.next()?.to_string();
    let mut path = firstline.next()?.to_string();

    let http2: bool = firstline.next()?.to_string().contains("HTTP/2");

    let mut parameter_template = match config.body_type == "json" {
        true => String::from("\"%k\":\"%v\", "),
        false => String::from("%k=%v&")
    };

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

    let mut body = lines.next().unwrap_or("").to_string();
    while let Some(part) = lines.next() {
        if !part.is_empty() {
            body.push_str(part);
        }
    }

    //check whether the body type can be json
    let body_type = if config.body_type.contains('-') && config.as_body
    && (content_type.contains("json") || (!body.is_empty() && body.starts_with('{'))) {
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

    let mut url = [proto.to_string(), host.clone(), path.clone()].concat();
    let initial_url = url.clone();
    if !config.as_body && url.contains('?') && url.contains('=') && !url.contains("%s") {
        url.push_str("&%s");
        path.push_str("&%s");
    } else if !config.as_body {
        url.push_str("?%s");
        path.push_str("?%s");
    }

    Some(Config {
        method,
        url,
        host,
        path,
        headers,
        body,
        body_type,
        parameter_template,
        http2,
        initial_url,
        ..config
    })
}

pub fn random_line(size: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(size)
        .collect::<String>()
}

pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn create_output(config: &Config, found_params: Vec<String>) -> String {
    match config.output_format.as_str() {
        "url" => {
            let mut line = match config.initial_url.contains('?') {
                true => config.initial_url.to_owned()+"&",
                false => config.initial_url.to_owned()+"?"
            };

            for param in &found_params {
                line.push_str(&param);
                if !param.contains('=') {
                    line.push('=');
                    line.push_str(&random_line(config.value_size));
                }
                line.push('&')
            }
            line.pop();
            line.push('\n');

            line
        }
        "json" => {
            let mut line = format!(
                "{{\"method\":\"{}\", \"url\":\"{}\", \"parameters\":[",
                &config.method,
                &config.initial_url
            );

            for param in &found_params {
                line.push('\"');
                line.push_str(&param.replace("\"", "\\\""));
                line.push('\"');
                line.push_str(", ");
            }

            let mut line = line[..line.len() - 2].to_string();
            line.push_str("]}\n");
            line
        },
        _ => {
            let mut line = format!("{} {} % ", &config.method, &config.initial_url);

            for param in &found_params {
                line.push_str(&param);
                line.push_str(", ")
            }

            line = line[..line.len() - 2].to_string();
            line.push('\n');
            line
        },
    }
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