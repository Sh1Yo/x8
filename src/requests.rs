use crate::{
    structs::{Config, DefaultResponse, ResponseData, Stable},
    utils::{compare, beautify_html, beautify_json, make_body, make_query, make_hashmap, random_line},
};
use colored::*;
use reqwest::Client;
use std::{
    collections::{BTreeMap, HashMap},
    io::{self, Write},
    time::Duration,
};

//makes first requests and checks page behavior
pub fn empty_reqs(
    config: &Config,
    initial_response: &ResponseData,
    reflections_count: usize,
    count: usize,
    client: &Client,
    max: usize,
) -> (Vec<String>, Stable) {
    let mut stable = Stable {
        body: true,
        reflections: true,
    };
    let mut diffs: Vec<String> = Vec::new();

    for i in 0..count {
        let response = random_request(config, client, reflections_count, max);

        //progress bar
        if config.verbose > 0 && !config.disable_progress_bar {
            write!(
                io::stdout(),
                "{} {}/{}       \r",
                &"-> ".bright_green(),
                i,
                count
            ).ok();
            io::stdout().flush().unwrap_or(());
        }

        if response.text.len() > 25 * 1024 * 1024 && !config.force_binary {
            writeln!(io::stderr(), "[!] {} the page is too huge", &config.url).ok();
            std::process::exit(1)
        }

        if !response.reflected_params.is_empty() {
            stable.reflections = false;
        }

        let (is_code_the_same, new_diffs) = compare(config, initial_response, &response);

        if !is_code_the_same {
            writeln!(
                io::stderr(),
                "[!] {} the page is not stable (code)",
                &config.url
            ).ok();
            std::process::exit(1)
        }

        for diff in new_diffs {
            if !diffs.iter().any(|i| i == &diff) {
                diffs.push(diff);
            }
        }
    }

    let response = random_request(config, client, reflections_count, max);

    for diff in compare(config, initial_response, &response).1 {
        if !diffs.iter().any(|i| i == &diff) {
            if config.verbose > 0 {
                writeln!(
                    io::stdout(),
                    "{} the page is not stable (body)",
                    &config.url
                ).ok();
            }
            stable.body = false;
            return (diffs, stable);
        }
    }
    (diffs, stable)
}

//calls request() with random parameters
pub fn random_request(
    config: &Config,
    client: &Client,
    reflections: usize,
    max: usize,
) -> ResponseData {
    request(
        &config,
        &client,
        &make_hashmap(
            &(0..max).map(|_| random_line(config.value_size)).collect::<Vec<String>>(),
            &config.value_template,
            &config.key_template,
            config.value_size,
        ),
        reflections
    )
}

pub fn request(
    config: &Config,
    client: &Client,
    initial_query: &HashMap<String, String>,
    reflections: usize,
) -> ResponseData {
    let mut query: HashMap<String, String> = HashMap::with_capacity(initial_query.len());
    for (k, v) in initial_query.iter() {
        query.insert(k.to_string(), v.replace("%random%_", ""));
    }

    let body: String = if config.as_body && !query.is_empty() {
        make_body(&config, &query)
    } else {
        String::new()
    };

    let initial_client = client.clone();

    std::thread::sleep(config.delay);

    let url: String = if config.url.contains("%s") {
        config.url.replace("%s", &make_query(&query, config))
    } else {
        config.url.clone()
    };

    let url: &str = &url;

    let mut client = if config.as_body {
        match config.method.as_str() {
            "GET" => client.get(url).body(body),
            "POST" => client.post(url).body(body),
            "PUT" => client.put(url).body(body),
            "PATCH" => client.patch(url).body(body),
            "DELETE" => client.delete(url).body(body),
            "HEAD" => client.head(url).body(body),
            _ => {
                writeln!(io::stderr(), "Method is not supported").ok();
                std::process::exit(1);
            },
        }
    } else {
        match config.method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            "HEAD" => client.head(url),
            _ => {
                writeln!(io::stderr(), "Method is not supported").ok();
                std::process::exit(1);
            }
        }
    };

    client = if config.as_body && !config.disable_cachebuster {
        client.query(&[(random_line(config.value_size), random_line(config.value_size))])
    } else {
        client
    };

    client = if !config.as_body && !config.body.is_empty() {
        client.body(config.body.clone())
    } else {
        client
    };

    for (key, value) in config.headers.iter() {
        client = client.header(key, value.replace("{{random}}", &random_line(config.value_size)));
    }

    let mut res = match client.send() {
        Ok(val) => val,
        Err(err) => {
            writeln!(io::stderr(), "[!] {} {:?}", url, err).ok();
            if !query.is_empty() {
                //detect whether the host died
                if request(config, &initial_client, &HashMap::new(), reflections).code == 0 {
                    writeln!(io::stderr(), "[~] error at the {} observed. Wait 50 sec and repeat.", config.url).ok();
                    std::thread::sleep(Duration::from_secs(50));

                    if request(config, &initial_client, &HashMap::new(), reflections).code == 0 {
                        writeln!(io::stderr(), "[!] unable to reach {}", config.url).ok();
                        std::process::exit(1)
                    }
                }
            }
            return <ResponseData as DefaultResponse>::default();
        }
    };

    let body = match res.text() {
        Ok(val) => {
            if config.disable_response_correction {
                val
            } else if config.is_json
                || (res.headers().get("content-type").is_some()
                    && res.headers().get("content-type").unwrap().to_str().unwrap_or("").contains(&"json"))
            {
                beautify_json(&val)
            } else if res.headers().get("content-type").is_some()
                && res.headers().get("content-type").unwrap().to_str().unwrap_or("").contains(&"html")
            {
                beautify_html(&val)
            } else {
                val
            }
        }
        Err(_) => String::new(),
    };

    let mut reflected_params: Vec<String> = Vec::new();

    for (key, value) in initial_query.iter() {
        if value.contains("%random%_") && body.matches(&value.replace("%random%_", "").as_str()).count() as usize != reflections {

            reflected_params.push(key.to_string())
        }
    }

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in res.headers().iter() {
        headers.insert(
            key.as_str().to_string(),
            value.to_str().unwrap().to_string(),
        );
    }

    let mut text = String::new();
    for (key, value) in headers.iter() {
        text.push_str(&key);
        text.push_str(&": ");
        text.push_str(&value);
        text.push_str(&"\n");
    }
    text.push_str(&"\n\n");
    text.push_str(&body);

    let code = res.status().as_u16();

    ResponseData {
        text,
        code,
        reflected_params,
    }
}