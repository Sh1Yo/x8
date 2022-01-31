use crate::{
    structs::{Config, ResponseData, Stable, Statistic},
    utils::{compare, beautify_html, beautify_json, make_body, make_query, make_header_value, make_hashmap, fix_headers, random_line},
};
use colored::*;
use reqwest::Client;
use std::{
    error::Error,
    time::Duration,
    collections::{BTreeMap, HashMap},
    io::{self, Write},
};

//makes first requests and checks page behavior
pub async fn empty_reqs(
    config: &Config,
    stats: &mut Statistic,
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
        let response = random_request(config, stats, client, reflections_count, max).await;

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

        if response.text.len() > 25 * 1024 * 1024 && !config.force {
            writeln!(io::stderr(), "[!] {} the page is too huge", &config.url).ok();
            std::process::exit(1)
        }

        if !response.reflected_params.is_empty() {
            stable.reflections = false;
        }

        let (is_code_the_same, new_diffs) = compare(initial_response, &response);

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

    let response = random_request(config, stats, client, reflections_count, max).await;

    for diff in compare(initial_response, &response).1 {
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
pub async fn random_request(
    config: &Config,
    stats: &mut Statistic,
    client: &Client,
    reflections: usize,
    max: usize,
) -> ResponseData {
    request(
        &config,
        stats,
        &client,
        &make_hashmap(
            &(0..max).map(|_| random_line(config.value_size*2)).collect::<Vec<String>>(),
            config.value_size,
        ),
        reflections
    ).await
}

fn create_request(
    config: &Config,
    query: String,
    hashmap_query: &HashMap<String, String>,
    client: &Client
) -> reqwest::RequestBuilder {
    let url: String = if config.url.contains("%s") {
        config.url.replace("%s", &query)
    } else {
        config.url.clone()
    };

    let mut client = if config.as_body {
        match config.method.as_str() {
            "GET" => client.get(url).body(query.clone()),
            "POST" => client.post(url).body(query.clone()),
            "PUT" => client.put(url).body(query.clone()),
            "PATCH" => client.patch(url).body(query.clone()),
            "DELETE" => client.delete(url).body(query.clone()),
            "HEAD" => client.head(url).body(query.clone()),
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
        if value.contains("%s") && config.within_headers {
            client = client.header(key, value.replace("%s", &query).replace("{{random}}", &random_line(config.value_size)));
        } else {
            client = client.header(key, value.replace("{{random}}", &random_line(config.value_size)));
        };
    }

    if config.headers_discovery && !config.within_headers {
        for (key, value) in hashmap_query.iter() {

            client = match fix_headers(key) {
                Some(val) => client.header(&val, value.replace("{{random}}", &random_line(config.value_size))),
                None => client.header(key, value.replace("{{random}}", &random_line(config.value_size)))
            };
        }
    }

    client
}

pub async fn request(
    config: &Config,
    stats: &mut Statistic,
    client: &Client,
    initial_query: &HashMap<String, String>,
    reflections: usize,
) -> ResponseData {
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

    std::thread::sleep(config.delay);

    let url: &str = &config.url;

    stats.amount_of_requests += 1;
    let res = match create_request(config, query, &hashmap_query, client).send().await {
        Ok(val) => val,
        Err(_) => {
            //Try to make a random request instead
            let mut random_query: HashMap<String, String> = HashMap::with_capacity(hashmap_query.len());
            for (k, v) in make_hashmap(
                &(0..hashmap_query.len()).map(|_| random_line(config.value_size)).collect::<Vec<String>>(),
                config.value_size,
            ) {
                random_query.insert(k.to_string(), v.replace("%random%_", ""));
            }
            let random_query: String = if !random_query.is_empty() {
                if config.as_body {
                    make_body(&config, &random_query)
                } else if config.headers_discovery {
                    make_header_value(&config, &random_query)
                } else {
                    make_query(&config, &random_query)
                }
            } else {
                String::new()
            };

            stats.amount_of_requests += 1;
            match create_request(config, random_query.clone(), &hashmap_query, client).send().await {
                Ok(_) => return ResponseData {
                                    text: String::new(),
                                    code: 0,
                                    reflected_params: Vec::new(),
                                },
                Err(err) => {
                    writeln!(io::stderr(), "[!] {} {:?}", url, err).ok();
                    match err.source() {
                        Some(val) => if val.to_string() == "invalid HTTP version parsed" && !config.http2 {
                             writeln!(io::stdout(), "[!] {}", "Try to use --http2 option".bright_red()).ok();
                             std::process::exit(1);
                        },
                        None => ()
                    };
                    writeln!(io::stderr(), "[~] error at the {} observed. Wait 50 sec and repeat.", config.url).ok();
                    std::thread::sleep(Duration::from_secs(50));
                    match create_request(config, random_query, &hashmap_query, client).send().await {
                        Ok(_) => return ResponseData {
                            text: String::new(),
                            code: 0,
                            reflected_params: Vec::new(),
                        },
                        Err(_) => {
                            writeln!(io::stderr(), "[!] unable to reach {}", config.url).ok();
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    };

    let code = res.status().as_u16();
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in res.headers().iter() {
        headers.insert(
            key.as_str().to_string(),
            value.to_str().unwrap_or("").to_string(),
        );
    }

    let body = match res.text().await {
        Ok(val) => {
            if config.disable_response_correction {
                val
            } else if config.is_json
                || (headers.get("content-type").is_some()
                    && headers.get("content-type").unwrap().as_str().contains(&"json"))
            {
                beautify_json(&val)
            } else if headers.get("content-type").is_some()
                && headers.get("content-type").unwrap().as_str().contains(&"html")
            {
                beautify_html(&val)
            } else {
                val
            }
        }
        Err(_) => String::new(),
    };

    let mut text = String::new();
    for (key, value) in headers.iter() {
        text.push_str(&key);
        text.push_str(&": ");
        text.push_str(&value);
        text.push_str(&"\n");
    }
    text.push_str(&"\n\n");
    text.push_str(&body);

    let mut reflected_params: Vec<String> = Vec::new();

    for (key, value) in initial_query.iter() {
        if value.contains("%random%_") && text.to_ascii_lowercase().matches(&value.replace("%random%_", "").as_str()).count() as usize != reflections {
            reflected_params.push(key.to_string());
        }
    }

    ResponseData {
        text,
        code,
        reflected_params,
    }
}