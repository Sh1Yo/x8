use crate::requests::request;
use crate::structs::{Config, ResponseData};

use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use rand::Rng;
use regex::Regex;
use reqwest::Client;
use std::{
    collections::HashMap,
    fs::{self, File},
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
    config: &Config,
    initial_response: &ResponseData,
    response: &ResponseData,
) -> (bool, Vec<String>) {
    let name1 = random_line(3);
    let name2 = random_line(3);

    let mut code: bool = true;
    let mut diffs: Vec<String> = Vec::new();

    if initial_response.code != response.code {
        code = false
    }

    for diff in check_diffs(
        config,
        &initial_response.text,
        &response.text,
        &name1,
        &name2,
    ) {
        diffs.push(diff)
    }

    (code, diffs)
}

//get possible parameters from the page source code
pub fn heuristic(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();

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

    let re_quotes = Regex::new(r#"("|')"#).unwrap();
    let re_words_in_quotes = Regex::new(r#"("|')\w{3,20}('|")"#).unwrap();
    for cap in re_words_in_quotes.captures_iter(body) {
        found.push(re_quotes.replace_all(&cap[0], "").to_string());
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

    req.push_str(" HTTP/1.1\n");

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
pub fn generate_data(config: &Config, client: &Client, query: &HashMap<String, String>) {
    let req = generate_request(config, query);

    writeln!(io::stdout(), "Request:\n{}", req).ok();

    let response = request(config, client, &query, 0);

    writeln!(
        io::stdout(),
        "Response:\nCode: {}\n\n{}",
        response.code,
        response.text
    ).ok();
}

//Add %s if it is absent in the body
pub fn adjust_body(body: &str, t: &str) -> String {
    let mut body = body.to_string();

    if t.contains("json") {
        body.pop();
        body.push_str(", %s}");
        body
    } else {
        body.push_str("&%s");
        body
    }
}

pub fn make_body(config: &Config, query: &HashMap<String, String>) -> String {
    let mut body: String = String::new();

    if config.body_type.contains("urlencode") {
        for (k, v) in query {
            body.push_str(k);
            body.push('=');
            body.push_str(v);
            body.push('&');
        }
        body.pop();

        body = if config.encode {
            utf8_percent_encode(&body, &FRAGMENT).to_string()
        } else {
            body
        };

        if config.body.is_empty() {
            body
        } else {
            config.body.replace("%s", &body).replace("{{random}}", &random_line(config.value_size))
        }
    } else if config.body_type.contains("json") {
        if config.body.is_empty() {
            body.push('{');
        }

        for (k, v) in query {
            body.push('"');
            body.push_str(k);
            body.push_str("\":");
            if !RE_JSON_WORDS_WITHOUT_QUOTES.is_match(v) {
                body.push('"');
                body.push_str(v);
                body.push('"');
            } else {
                body.push_str(v);
            }
            body.push_str(", ");
        }
        body.pop();
        body.pop();

        if config.body.is_empty() {
            body.push('}');
        }

        body = if config.encode {
            utf8_percent_encode(&body, &FRAGMENT).to_string()
        } else {
            body
        };

        if config.body.is_empty() {
            body
        } else {
            config.body.replace("%s", &body).replace("{{random}}", &random_line(config.value_size))
        }
    } else {
        writeln!(io::stderr(), "Unsupported body type").ok();
        std::process::exit(1);
    }
}

pub fn make_query(params: &HashMap<String, String>, config: &Config) -> String {
    let mut query: String = String::from("");

    for (k, v) in params {
        query = query + k + "=" + v + "&";
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
    value_template: &str,
    key_template: &str,
    value_size: usize,
) -> HashMap<String, String> {
    let mut hashmap: HashMap<String, String> = HashMap::new();

    let key_template: &str = if key_template.is_empty() {
        "%s"
    } else {
        key_template
    };

    let value_template: &str = if value_template.is_empty() {
        "%s"
    } else {
        value_template
    };

    for word in words.iter() {
        let (param, value) = if word.matches('=').count() == 1 {
            let mut splitted = word.split('=');
            (splitted.next().unwrap(), splitted.next().unwrap().to_string())
        } else {
            (word.as_str(), "%random%_".to_owned()+&random_line(value_size))
        };

        hashmap.insert(
            key_template.replace("%s", param),
            value_template.replace("%s", &value),
        );
    }

    hashmap
}

//use external diff tool to compare responses
pub fn check_diffs(
    config: &Config,
    resp1: &str,
    resp2: &str,
    postfix1: &str,
    postfix2: &str,
) -> Vec<String> {
    use std::process::Command;

    let mut name1 = config.tmp_directory.clone();
    let mut name2 = config.tmp_directory.clone();

    name1.push_str(postfix1);
    name2.push_str(postfix2);

    let mut diffs: Vec<String> = Vec::new();

    match fs::write(&name1, resp1) {
        Ok(_) => (),
        Err(err) => {
            writeln!(io::stderr(), "[!] unable to create temp file: {}", err).ok();
            std::process::exit(1);
        }
    };
    match fs::write(&name2, resp2) {
        Ok(_) => (),
        Err(err) => {
            writeln!(io::stderr(), "[!] unable to create temp file: {}", err).ok();
            std::process::exit(1);
        }
    };

    let output = match Command::new(&config.diff_location)
        .arg(&name1)
        .arg(&name2)
        .output()
    {
        Ok(val) => val,
        Err(err) => {
            writeln!(io::stderr(), "[!] diff: {}", err).ok();
            std::process::exit(1);
        }
    };

    match fs::remove_file(name1) {
        Ok(_) => (),
        Err(err) => writeln!(io::stderr(), "[!] unable to remove file: {}", err).unwrap_or(()),
    };
    match fs::remove_file(name2) {
        Ok(_) => (),
        Err(err) => writeln!(io::stderr(), "[!] unable to remove file: {}", err).unwrap_or(()),
    };

    for line in match std::str::from_utf8(&output.stdout) {
        Ok(val) => val,
        Err(err) => {
            writeln!(io::stderr(), "[!] {}", err).ok();
            return Vec::new();
        }
    }.split('\n') {
        let line = line.to_string();

        if !line.is_empty() {
            match line.chars().next().unwrap() {
                '<' => (),
                '>' => (),
                '\\' => (),
                _ => {
                    if line.len() == 3 {
                        match &line[..3] {
                            "---" => (),
                            _ => diffs.push(line),
                        }
                    } else {
                        diffs.push(line)
                    }
                }
            }
        }

        if !diffs.is_empty() && diffs[0].contains("Binary files") && !config.force_binary {
            writeln!(io::stderr(), "[!] binary data detected").ok();
            std::process::exit(1)
        }
    }
    diffs
}

pub fn parse_request(insecure: bool, request: &str, config: Config) -> Option<Config> {
    let mut lines = request.lines();
    let mut host = String::new();
    let mut headers: HashMap<String, String> = config.headers.clone();
    let proto = if insecure { "http://" } else { "https://" };
    let mut firstline = lines.next()?.split(' ');
    let method = firstline.next()?.to_string();
    let path = firstline.next()?.to_string();

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

        if key.to_lowercase() == "host" {
            host = value.clone()
        };

        headers.insert(key.to_string(), value);
    }

    let body = lines.next().unwrap_or("");
    let body_type = if config.body_type.contains('-') && !body.is_empty() && body.starts_with('{') {
        String::from("json-")
    } else {
        config.body_type
    };
    let body = if !body.is_empty() && !body.contains("%s") {
        adjust_body(body, &body_type)
    } else {
        body.to_string()
    };

    let mut url = [proto.to_string(), host.clone(), path.clone()].concat();
    if !config.as_body && url.contains('?') && url.contains('=') && !url.contains("%s") {
        url.push_str("&%s");
    } else if !config.as_body {
        url.push_str("?%s");
    }

    Some(Config {
        method,
        url,
        host,
        path,
        headers,
        body,
        body_type,
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