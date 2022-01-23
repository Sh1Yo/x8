use crate::{structs::Config, utils::{parse_request, adjust_body}};
use clap::{crate_version, App, AppSettings, Arg};
use std::{collections::HashMap, fs, time::Duration, io::{self, Write}};
use url::Url;

pub fn get_config() -> (Config, usize) {

    let app = App::new("x8")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(crate_version!())
        .author("sh1yo <sh1yo@tuta.io>")
        .about("Hidden parameters discovery suite")
        .arg(Arg::with_name("url")
            .short("u")
            .long("url")
            .help("You can add a custom injection point with %s.")
            .takes_value(true)
            .conflicts_with("request")
        )
        .arg(Arg::with_name("request")
            .short("r")
            .long("request")
            .help("The file with the raw http request")
            .takes_value(true)
            .conflicts_with("url")
        )
        .arg(Arg::with_name("proto")
            .long("proto")
            .help("Protocol to use with request file (default is \"https\")")
            .takes_value(true)
            .requires("request")
            .conflicts_with("url")
        )
        .arg(
            Arg::with_name("wordlist")
                .short("w")
                .long("wordlist")
                .help("The file with parameters")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("parameter_template")
                .short("P")
                .long("param-template")
                .help("%k - key, %v - value. Example: --param-template 'user[%k]=%v&'")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("body")
                .short("b")
                .long("body")
                .help("Example: --body '{\"x\":{%s}}'\nAvailable variables: {{random}}")
                .value_name("body")
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("body-type")
                .short("t")
                .long("body-type")
                .help("Available: urlencode, json. (default is \"urlencode\")\nCan be detected automatically if --body is specified")
                .value_name("body type")
        )
        .arg(
            Arg::with_name("proxy")
                .short("x")
                .long("proxy")
                .value_name("proxy")
        )
        .arg(
            Arg::with_name("delay")
                .short("d")
                .long("delay")
                .value_name("Delay between requests in milliseconds")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("file")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("output-format")
                .short("O")
                .long("output-format")
                .help("standart, json, url, request (default is \"standart\")")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("append")
                .long("append")
                .help("Append to the output file instead of overwriting it.")
        )
        .arg(
            Arg::with_name("method")
                .short("X")
                .long("method")
                .value_name("method")
                .help("Available: GET, POST, PUT, PATCH, DELETE, HEAD. (default is \"GET\")")
                .takes_value(true)
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("headers")
                .short("H")
                .help("Example: -H 'one:one' 'two:two'")
                .takes_value(true)
                .min_values(1)
        )
        .arg(
            Arg::with_name("as-body")
                .long("as-body")
                .help("Send parameters via body.\nBuilt in body types that can be detected automatically: json, urlencode")
        )
        .arg(
            Arg::with_name("headers-discovery")
                .long("headers")
                .help("Switch to header discovery mode.\nForbidden chars would be automatically removed from headers names")
                .conflicts_with("as-body")
                .conflicts_with("param-template")
        )
        .arg(
            Arg::with_name("force")
                .long("force")
                .help("Ignore 'binary data detected', 'the page is too huge', 'param_template lacks variables' error messages")
        )
        .arg(
            Arg::with_name("disable-response-correction")
                .long("disable-response-correction")
                .short("C")
                .help("Do not beautify responses before processing. Reduces accuracy.")
        )
        .arg(
            Arg::with_name("disable-custom-parameters")
                .long("disable-custom-parameters")
                .help("Do not check automatically parameters like admin=true")
        )
        .arg(
            Arg::with_name("disable-colors")
                .long("disable-colors")
        )
        .arg(
            Arg::with_name("disable-progress-bar")
                .long("disable-progress-bar")
        )
        .arg(
            Arg::with_name("keep-newlines")
                .long("keep-newlines")
                .help("--body 'a\\r\\nb' -> --body 'a{{new_line}}b'.\nWorks with body and parameter templates only.")
            )
        .arg(
            Arg::with_name("replay-once")
                .long("replay-once")
                .help("If replay proxy is specified, send all found parameters within one request.")
                .requires("replay-proxy")
        )
        .arg(
            Arg::with_name("replay-proxy")
                .takes_value(true)
                .long("replay-proxy")
                .help("Request target with every found parameter via replay proxy at the end.")
        )
        .arg(
            Arg::with_name("custom-parameters")
                .long("custom-parameters")
                .help("Check these parameters with non-random values like true/false yes/no\n(default is \"admin bot captcha debug disable encryption env show sso test waf\")")
                .takes_value(true)
                .min_values(1)
                .conflicts_with("disable-custom-parameters")
        )
        .arg(
            Arg::with_name("custom-values")
                .long("custom-values")
                .help("Check custom parameters with these values (default is \"1 0 false off null true yes no\")")
                .takes_value(true)
                .min_values(1)
                .conflicts_with("disable-custom-parameters")
        )
        .arg(
            Arg::with_name("follow-redirects")
                .long("follow-redirects")
                .short("L")
                .help("Follow redirections")
        )
        .arg(
            Arg::with_name("encode")
                .long("encode")
                .help("Encodes query or body before a request, i.e & -> %26, = -> %3D\nList of chars to encode: \", `, , <, >, &, #, ;, /, =, %")
        )
        .arg(
            Arg::with_name("is-json")
                .long("is-json")
                .help("If the output is valid json and the content type does not contain 'json' keyword - specify this argument for a more accurate search")
        )
        .arg(
            Arg::with_name("test")
                .long("test")
                .help("Prints request and response")
        )
        .arg(
            Arg::with_name("verbose")
                .long("verbose")
                .short("v")
                .help("Verbose level 0/1/2 (default is 1)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("save-responses")
                .long("save-responses")
                .help("Save matched responses to a directory")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("disable-cachebuster")
                .long("disable-cachebuster")
        )
        .arg(
            Arg::with_name("value_size")
                .long("value-size")
                .help("Custom value size. Affects {{random}} variables as well (default is 7)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("learn_requests_count")
                .long("learn-requests")
                .help("Set the custom number of learning requests. (default is 9)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("max")
                .short("m")
                .long("max")
                .help("Change the maximum number of parameters.\n(default is 128/192/256 for query, 64/128/196 for headers and 512 for body)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("concurrency")
                .short("c")
                .help("The number of concurrent requests (default is 1)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("http2")
                .long("http2")
                .help("Prefer http/2 over http/1.1")
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("verify")
                .long("verify")
                .help("Verify found parameters one more time.")
        )
        .arg(
            Arg::with_name("reflected_only")
                .long("reflected-only")
                .help("Disable page comparison and search for reflected parameters only.")
        );

    let args = app.clone().get_matches();

    let delay = match args.value_of("delay") {
        Some(val) => match val.parse() {
            Ok(val) => Duration::from_millis(val),
            Err(_) => {
                writeln!(io::stderr(), "Unable to parse 'delay' value").ok();
                std::process::exit(1);
            }
        },
        None => Duration::from_millis(0),
    };

    let max: usize = match args.value_of("max") {
        Some(val) => match val.parse() {
            Ok(val) => val,
            Err(_) => {
                writeln!(io::stderr(), "Unable to parse 'max' value").ok();
                std::process::exit(1);
            }
        },
        None => {
            if args.is_present("as-body") {
                512
            } else if !args.is_present("headers-discovery") {
                128
            } else {
                64
            }
        }
    };

    let value_size: usize = match args.value_of("value_size") {
        Some(val) => match val.parse() {
            Ok(val) => val,
            Err(_) => {
                writeln!(io::stderr(), "Unable to parse 'value_size' value").ok();
                std::process::exit(1);
            }
        },
        None => {
            7
        }
    };

    let learn_requests_count: usize = match args.value_of("learn_requests_count") {
        Some(val) => match val.parse() {
            Ok(val) => val,
            Err(_) => {
                writeln!(io::stderr(), "Unable to parse 'learn_requests_count' value").ok();
                std::process::exit(1);
            }
        },
        None => {
            9
        }
    };

    let concurrency: usize = match args.value_of("concurrency") {
        Some(val) => match val.parse() {
            Ok(val) => val,
            Err(_) => {
                writeln!(io::stderr(), "Unable to parse 'concurrency' value").ok();
                std::process::exit(1);
            }
        },
        None => {
            1
        }
    };

    let mut headers: HashMap<String, String> = HashMap::new();
    let mut within_headers: bool = false;
    if let Some(val) = args.values_of("headers") {
        for header in val {
            let mut k_v = header.split(':');
            let key = match k_v.next() {
                Some(val) => val,
                None => {
                    writeln!(io::stderr(), "Unable to parse headers").ok();
                    std::process::exit(1);
                }
            };
            let value: String = [
                match k_v.next() {
                    Some(val) => val.trim().to_owned(),
                    None => {
                        writeln!(io::stderr(), "Unable to parse headers").ok();
                        std::process::exit(1);
                    }
                },
                k_v.map(|x| ":".to_owned() + x).collect(),
            ].concat();

            if value.contains("%s") {
                within_headers = true;
            }

            headers.insert(key.to_string(), value);
        }
    };

    let verbose: u8 = match args.value_of("verbose") {
        Some(val) => val.parse().expect("incorrect verbose"),
        None => 1,
    };

    let url = match Url::parse(args.value_of("url").unwrap_or("https://example.com")) {
        Ok(val) => val,
        Err(err) => {
            writeln!(io::stderr(), "Unable to parse target url: {}", err).ok();
            std::process::exit(1);
        },
    };

    let host = url.host_str().unwrap();
    let mut path = url[url::Position::BeforePath..].to_string();

    let body = match args.is_present("keep-newlines") {
        true => args.value_of("body").unwrap_or("")/*.replace("\\\\", "\\")*/.replace("\\n", "\n").replace("\\r", "\r"),
        false => args.value_of("body").unwrap_or("").to_string()
    };

    //check whether it is possible to automatically fix body type
    //- at the end means "specified automatically"
    let body_type = if args.value_of("body-type").is_none() && args.value_of("parameter_template").unwrap_or("").is_empty()
        && (
            (
                !body.is_empty() && body.starts_with('{')
            )
            || (
                headers.contains_key("Content-Type") && headers["Content-Type"].contains("json")
            )
        ) {
        String::from("json-")
    } else {
        args.value_of("body-type").unwrap_or("urlencode-").to_string()
    };

    let body = if !body.contains("%s") && args.is_present("as-body") {
        adjust_body(&body, &body_type)
    } else {
        body
    };

    //set default headers if weren't specified by a user.
    if !headers.keys().any(|i| i.contains("User-Agent")) {
        headers.insert(String::from("User-Agent"), String::from("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/83.0.4103.97 Safari/537.36"));
    }

    if !args.is_present("disable-cachebuster") {
        if !headers.keys().any(|i| i.contains("Accept")) {
            headers.insert(String::from("Accept"), String::from("*/*, text/{{random}}"));
        }
        if !headers.keys().any(|i| i.contains("Accept-Language")) {
            headers.insert(
                String::from("Accept-Language"),
                String::from("en-US, {{random}};q=0.9, *;q=0.5"),
            );
        }
        if !headers.keys().any(|i| i.contains("Accept-Charset")) {
            headers.insert(
                String::from("Accept-Charset"),
                String::from("utf-8, iso-8859-1;q=0.5, {{random}};q=0.2, *;q=0.1"),
            );
        }
    }

    if !headers.keys().any(|i| i.contains("Content-Type")) && (args.is_present("as-body") || args.value_of("body").is_some()) {
        if body_type.contains("json") {
            headers.insert(
                String::from("Content-Type"),
                String::from("application/json"),
            );
        } else {
            headers.insert(
                String::from("Content-Type"),
                String::from("application/x-www-form-urlencoded"),
            );
        }
    }

    let mut url = args
        .value_of("url")
        .unwrap_or("https://something.something")
        .to_string();

    if !args.is_present("as-body") && !within_headers && !args.is_present("headers-discovery") && url.contains('?') && url.contains('=') && !url.contains("%s") {
        if args.is_present("encode") {
            url.push_str("%26%s");
            path.push_str("%26%s");
        } else {
            url.push_str("&%s");
            path.push_str("&%s");
        }
    } else if !args.is_present("as-body") && !within_headers &&!args.is_present("headers-discovery") && !url.contains("%s") {
        if args.is_present("encode") {
            url.push_str("%3f%s");
            path.push_str("%3f%s");
        } else {
            url.push_str("?%s");
            path.push_str("?%s");
        }
    }

    let parameter_template = match args.is_present("keep-newlines") {
        true => args.value_of("parameter_template").unwrap_or("").replace("\\n", "\n").replace("\\r", "\r"),
        false => args.value_of("parameter_template").unwrap_or("").to_string()
    };
    let mut parameter_template = parameter_template.as_str();

    if !parameter_template.is_empty()
        && (!parameter_template.contains("%k") || !parameter_template.contains("%v"))
        && !args.is_present("force") {
            writeln!(io::stderr(), "param_template lacks important variables like %k or %v").ok();
            std::process::exit(1);
    }

    if parameter_template.is_empty() {
        if body_type.contains("json") && args.is_present("as-body") {
            parameter_template = "\"%k\":\"%v\", ";
        } else if within_headers {
            parameter_template = "%k=%v; ";
        } else {
            parameter_template = "%k=%v&";
        }
    }

    let custom_keys: Vec<String> = match args.values_of("custom-parameters") {
        Some(val) => {
            val.map(|x| x.to_string()).collect()
        }
        None =>["admin", "bot", "captcha", "debug", "disable", "encryption", "env", "show", "sso", "test", "waf"]
            .iter()
            .map(|x| x.to_string())
            .collect()
    };

    let custom_values: Vec<String> = match args.values_of("custom-values") {
        Some(val) => {
            val.map(|x| x.to_string()).collect()
        }
        None => ["1", "0", "false", "off", "null", "true", "yes", "no"]
            .iter()
            .map(|x| x.to_string())
            .collect()
    };

    let mut custom_parameters: HashMap<String, Vec<String>> = HashMap::with_capacity(custom_keys.len());
    for key in custom_keys.iter() {
        let mut values: Vec<String> = Vec::with_capacity(custom_values.len());
        for value in custom_values.iter() {
            values.push(value.to_string());
        }
        custom_parameters.insert(key.to_string(), values);
    }


    let request = match args.value_of("request") {
        Some(val) => match fs::read_to_string(val) {
            Ok(val) => val,
            Err(err) => {
                writeln!(io::stderr(), "Unable to open request file: {}", err).ok();
                std::process::exit(1);
            }
        },
        None => String::new(),
    };

    if args.is_present("disable-colors") {
        colored::control::set_override(false);
    }

    let mut config = Config {
        method: args.value_of("method").unwrap_or("GET").to_string(),
        initial_url: args.value_of("url").unwrap_or("").to_string(),
        url,
        host: host.to_string(),
        path,
        wordlist: args.value_of("wordlist").unwrap_or("").to_string(),
        parameter_template: parameter_template.to_string(),
        custom_parameters,
        headers,
        body,
        body_type,
        proxy: args.value_of("proxy").unwrap_or("").to_string(),
        replay_proxy: args.value_of("replay-proxy").unwrap_or("").to_string(),
        replay_once: args.is_present("replay-once"),
        output_file: args.value_of("output").unwrap_or("").to_string(),
        save_responses: args.value_of("save-responses").unwrap_or("").to_string(),
        output_format: args.value_of("output-format").unwrap_or("").to_string(),
        append: args.is_present("append"),
        as_body: args.is_present("as-body"),
        headers_discovery: args.is_present("headers-discovery"),
        within_headers,
        force: args.is_present("force"),
        disable_response_correction: args.is_present("disable-response-correction"),
        disable_custom_parameters: args.is_present("disable-custom-parameters"),
        disable_progress_bar: args.is_present("disable-progress-bar"),
        follow_redirects: args.is_present("follow-redirects"),
        encode: args.is_present("encode"),
        is_json: args.is_present("is-json"),
        test: args.is_present("test"),
        verbose,
        disable_cachebuster: args.is_present("disable-cachebuster"),
        delay,
        value_size,
        learn_requests_count,
        max,
        concurrency,
        http2: args.is_present("http2"),
        verify: args.is_present("verify"),
        reflected_only: args.is_present("reflected_only")
    };

    config = if !request.is_empty() {
        match parse_request(config, args.value_of("proto").unwrap_or("https"), &request, !args.value_of("parameter_template").unwrap_or("").is_empty()) {
            Some(val) => val,
            None => {
                writeln!(io::stderr(), "Unable to parse request file.").ok();
                std::process::exit(1);
            }
        }
    } else {
        config
    };

    (config, max)
}