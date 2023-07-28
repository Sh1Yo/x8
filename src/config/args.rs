use crate::{
    config::{
        structs::Config,
        utils::{convert_to_string_if_some, parse_request},
    },
    network::utils::{DataType, Headers},
};
use clap::{crate_version, App, AppSettings, Arg};
use std::{collections::HashMap, error::Error, fs, io::{self, Write}};
use tokio::time::Duration;
use url::Url;

use super::utils::{read_urls_if_possible, mimic_browser_headers, add_default_headers};

pub fn get_config() -> Result<Config, Box<dyn Error>> {
    let app = App::new("x8")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(crate_version!())
        .author("sh1yo <sh1yo@tuta.io>")
        .about("Hidden parameters discovery suite")
        .arg(Arg::with_name("url")
            .short("u")
            .long("url")
            .help("You can add a custom injection point with %s.\nMultiple urls and filenames are supported:\n-u filename.txt\n-u https://url1 http://url2")
            .takes_value(true)
            .min_values(1)
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
        .arg(Arg::with_name("port")
            .long("port")
            .short("-p")
            .help("Port to use with request file")
            .takes_value(true)
            .requires("request")
            .conflicts_with("url")
        )
        .arg(Arg::with_name("split-by")
            .long("split-by")
            .help("Split the request into lines by the provided sequence. By default splits by \\r, \\n and \\r\\n")
            .takes_value(true)
            .requires("request")
            .conflicts_with("url")
        )
        .arg(
            Arg::with_name("wordlist")
                .short("w")
                .long("wordlist")
                .help("The file with parameters (leave empty to read from stdin)")
                .default_value("")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("parameter-template")
                .short("P")
                .long("param-template")
                .help("%k - key, %v - value. Example: --param-template 'user[%k]=%v'\nDefault: urlencoded - <%k=%v>, json - <\"%k\":%v>, headers - <%k=%v>")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("joiner")
            .short("j")
            .long("joiner")
            .help("How to join parameter templates. Example: --joiner '&'\nDefault: urlencoded - '&', json - ', ', header values - '; '")
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
            Arg::with_name("data-type")
                .short("t")
                .long("data-type")
                .help("Available: urlencode, json\nCan be detected automatically if --body is specified (default is \"urlencode\")")
                .value_name("data-type")
        )
        .arg(
            Arg::with_name("proxy")
                .short("x")
                .long("proxy")
                .value_name("proxy")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("burp-proxy")
                .short("B")
                .help("Equal to -x http://localhost:8080")
                .conflicts_with("proxy")
        )
        .arg(
            Arg::with_name("delay")
                .short("d")
                .long("delay")
                .value_name("Delay between requests in milliseconds")
                .default_value("0")
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
                .help("standart, json, url, request")
                .default_value("standart")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("append")
                .long("append")
                .help("Append to the output file instead of overwriting it.")
        )
        .arg(
            Arg::with_name("remove-empty")
                .long("remove-empty")
                .requires("output")
                .help("Skip writing to file outputs of url:method pairs without found parameters")
        )
        .arg(
            Arg::with_name("method")
                .short("X")
                .long("method")
                .value_name("methods")
                .help("Multiple values are supported: -X GET POST")
                .takes_value(true)
                .min_values(1)
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("headers")
                .short("H")
                .help("Example: -H 'one:one' 'two:two'")
                .takes_value(true)
                .min_values(1)
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("invert")
                .long("invert")
                .help("By default, parameters are sent within the body only in case POST,PUT,PATCH,DELETE methods are used.
It's possible to overwrite this behavior by specifying the option")
                .conflicts_with("headers-discovery")
        )
        .arg(
            Arg::with_name("headers-discovery")
                .long("headers")
                .help("Switch to header discovery mode.\nNOTE Content-Length and Host headers are automatically removed from the list")
                .conflicts_with("invert")
                .conflicts_with("param-template")
        )
        .arg(
            Arg::with_name("force")
                .long("force")
                .help("Force searching for parameters on pages > 25MB. Remove an error in case there's 1 worker with --one-worker-per-host option.")
        )
        .arg(
            Arg::with_name("disable-custom-parameters")
                .long("disable-custom-parameters")
                .help("Do not automatically check parameters like admin=true")
        )
        .arg(
            Arg::with_name("disable-colors")
                .long("disable-colors")
        )
        .arg(
            Arg::with_name("force-enable-colors")
                .long("force-enable-colors")
        )
        .arg(
            Arg::with_name("disable-trustdns")
                .long("disable-trustdns")
                .help("Can solve some dns related problems")
        )
        .arg(
            Arg::with_name("disable-progress-bar")
                .long("disable-progress-bar")
        )
        .arg(
            Arg::with_name("progress-bar-len")
                .long("progress-bar-len")
                .default_value("26")
        )
        .arg(
            Arg::with_name("replay-once")
                .long("replay-once")
                .help("If a replay proxy is specified, send all found parameters within one request.")
                .requires("replay-proxy")
        )
        .arg(
            Arg::with_name("replay-proxy")
                .takes_value(true)
                .long("replay-proxy")
                .help("Request target with every found parameter via the replay proxy at the end.")
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
                .help("Values for custom parameters (default is \"1 0 false off null true yes no\")")
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
                .help("Encodes query or body before making a request, i.e & -> %26, = -> %3D\nList of chars to encode: \", `, , <, >, &, #, ;, /, =, %")
        )
        .arg(
            Arg::with_name("strict")
                .long("strict")
                .help("Only report parameters that have changed the different parts of a page")
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
                .help("Verbose level 0/1/2")
                .default_value("1")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("save-responses")
                .long("save-responses")
                .help("Save request and response to a directory when a parameter is found")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("learn-requests-count")
                .long("learn-requests")
                .help("Set the custom number of learn requests.")
                .default_value("9")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("recursion-depth")
                .long("recursion-depth")
                .help("Check the same list of parameters with the found parameters until there are no new parameters to be found.
Conflicts with --verify for now.")
                .takes_value(true)
                .conflicts_with("verify")
        )
        .arg(
            Arg::with_name("max")
                .short("m")
                .long("max")
                .help("Change the maximum number of parameters per request.\n(default is <= 256 for query, 64 for headers and 512 for body)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("timeout")
                .long("timeout")
                .help("HTTP request timeout in seconds.")
                .default_value("15")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("concurrency")
                .short("c")
                .help("The number of concurrent requests per url")
                .default_value("1")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("workers")
                .short("W")
                .long("workers")
                .help("The number of concurrent url checks.\nUse -W0 to run everything in parallel")
                .default_value("1")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("verify")
                .long("verify")
                .help("Verify found parameters.")
        )
        .arg(
            Arg::with_name("reflected-only")
                .long("reflected-only")
                .help("Disable page comparison and search for reflected parameters only.")
        )
        .arg(
            Arg::with_name("one-worker-per-host")
                .long("one-worker-per-host")
                .help("Multiple urls with the same host will be checked one after another,\nwhile urls with different hosts - are in parallel.\nDoesn't increase the number of workers")
        )
        .arg(
            Arg::with_name("mimic-browser")
                .long("mimic-browser")
                .help("Add default headers that browsers usually set.")
                .conflicts_with("request")
        )
        .arg(
            Arg::with_name("http")
                .long("http")
                .help("HTTP version. Supported versions: --http 1.1, --http 2")
                .takes_value(true)
        ).arg(
            Arg::with_name("check-binary")
                .long("check-binary")
                .help("Check the body of responses with binary content types")
        );

    let args = app.clone().get_matches();

    if args.value_of("url").is_none() && args.value_of("request").is_none() {
        Err("A target was not provided")?;
    }

    // parse numbers
    let delay = Duration::from_millis(args.value_of("delay").unwrap().parse()?);

    let learn_requests_count = args.value_of("learn-requests-count").unwrap().parse()?;
    let concurrency = args.value_of("concurrency").unwrap().parse()?;
    let workers = args.value_of("workers").unwrap().parse()?;
    let verbose = args.value_of("verbose").unwrap().parse()?;
    let timeout = args.value_of("timeout").unwrap().parse()?;
    let recursion_depth = args.value_of("recursion-depth").unwrap_or("0").parse()?;
    let progress_bar_len = args.value_of("progress-bar-len").unwrap().parse()?;

    let max: Option<usize> = if args.is_present("max") {
        Some(args.value_of("max").unwrap().parse()?)
    } else {
        None
    };

    if workers == 1 && args.is_present("one-worker-per-host") && !args.is_present("force") {
        Err("The --one-worker-per-host option doesn't increase the amount of workers. \
So there's no point in --one-worker-per-host with 1 worker. \
Increase the amount of workers to remove the error or use --force.")?;
    }

    // try to read request file
    let request = match args.value_of("request") {
        Some(val) => fs::read_to_string(val)?,
        None => String::new(),
    };

    let data_type  = match args.value_of("data-type") { 
        Some(val) => {
            if val == "json" {
                Some(DataType::Json)
            } else if val == "urlencoded" {
                Some(DataType::Urlencoded)
            } else {
                Err("Incorrect --data-type specified")?
            }
        }
        None => None
    };

    // parse the default request information
    // either via the request file or via provided parameters
    let (methods, urls, headers, body, data_type, http_version) = if !request.is_empty() {
        // if the request file is specified - get protocol (https/http) from args, specify scheme and port, and parse request file
        let proto = args
            .value_of("proto")
            .unwrap_or("https")
            .to_string();

        let scheme = proto.replace("://", "");

        let port: Option<u16> = if args.value_of("port").is_some() {
            Some(args.value_of("port").unwrap().parse()?)
        } else {
            None
        };

        parse_request(&request, &scheme, port, data_type, args.value_of("split-by"))?
    } else {
        // parse everything from user-supplied command line arguments
        let methods = if args.is_present("method") {
            args.values_of("method")
                .unwrap()
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
        } else {
            vec!["GET".to_string()]
        };

        let mut headers: HashMap<&str, String> = HashMap::new();

        if let Some(val) = args.values_of("headers") {
            for header in val {
                let mut k_v = header.split(':');
                let key = match k_v.next() {
                    Some(val) => val,
                    None => Err("Unable to parse headers")?,
                };
                let value = [
                    match k_v.next() {
                        Some(val) => val.trim().to_owned(),
                        None => Err("Unable to parse headers")?,
                    },
                    k_v.map(|x| ":".to_owned() + x).collect(),
                ]
                .concat();

                headers.insert(key, value);
            }
        };

        // set default headers if weren't specified by a user.
        let headers = if args.is_present("mimic-browser") {
            mimic_browser_headers(headers)
        } else {
            add_default_headers(headers)
        };

        // TODO replace with ".parse()" or sth like it
        let data_type = match data_type {
            Some(val) => {
                Some(val)
            }
            None => if headers.get_value_case_insensitive("content-type") == Some("application/json".to_string()) {
                Some(DataType::ProbablyJson)
            } else {
                None
            },
        };

        let http_version = if args.value_of("http").is_some() {
            match  args.value_of("http").unwrap() {
                "1.1" => Some(http::Version::HTTP_11),
                "2" => Some(http::Version::HTTP_2),
                _ => {
                    writeln!(
                        io::stdout(),
                        "[#] Incorrect http version provided. The argument is ignored"
                    ).ok();
                    None
                }
            }
        } else {
            None
        };

        let urls = args
            .values_of("url")
            .unwrap();

        let urls = if urls.len() == 1 && !urls.clone().any(|x| x.contains("://")) {
            // it can be a file
            match read_urls_if_possible(urls.clone().next().unwrap())? {
                Some(urls) => urls,
                None => Err("The provided --url value is neither url nor a filename.")?
            }
        } else {
            urls.map(|x| x.to_string()).collect()
        };

        let urls = urls.iter().map(|x| Url::parse(x))
            .collect::<Vec<Result<Url, url::ParseError>>>();

        // in case there's at least a single wrong url -- return with an error
        if urls.iter().any(|x| x.is_err()) {
            for err_url in urls.iter().filter(|x| x.is_err()) {
                err_url.to_owned()?;
            }
            unreachable!();
        } else {
            (
                methods,
                urls.iter()
                    .map(|x| x.as_ref().unwrap().to_string())
                    .collect::<Vec<String>>(),
                headers,
                args.value_of("body").unwrap_or("").to_string(),
                data_type,
                http_version
            )
        }
    };

    // generate custom param values like admin=true
    let custom_keys: Vec<String> = match args.values_of("custom-parameters") {
        Some(val) => val.map(|x| x.to_string()).collect(),
        None => [
            "admin",
            "bot",
            "captcha",
            "debug",
            "disable",
            "encryption",
            "env",
            "show",
            "sso",
            "test",
            "waf",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect(),
    };

    let custom_values: Vec<String> = match args.values_of("custom-values") {
        Some(val) => val.map(|x| x.to_string()).collect(),
        None => ["1", "0", "false", "off", "null", "true", "yes", "no"]
            .iter()
            .map(|x| x.to_string())
            .collect(),
    };

    let mut custom_parameters: HashMap<String, Vec<String>> =
        HashMap::with_capacity(custom_keys.len());
    for key in custom_keys.iter() {
        let mut values: Vec<String> = Vec::with_capacity(custom_values.len());
        for value in custom_values.iter() {
            values.push(value.to_string());
        }
        custom_parameters.insert(key.to_string(), values);
    }

    // disable colors
    if args.is_present("disable-colors") {
        colored::control::set_override(false);
    }

    // force enable colors to preseve colors while redirecting output
    if args.is_present("force-enable-colors") {
        colored::control::set_override(true);
    }

    // decrease verbose by 1 in case > 1 url is being checked in parallel
    // this behavior is explained in docs
    let verbose = if verbose > 0 && !(workers == 1 || urls.len() == 1) {
        verbose - 1
    } else {
        verbose
    };

    let proxy = if args.is_present("burp-proxy") {
        "http://localhost:8080".to_string()
    } else {
        args.value_of("proxy").unwrap_or("").to_string()
    };

    // TODO maybe replace empty with None
    Ok(Config {
        urls,
        methods,
        wordlist: args.value_of("wordlist").unwrap_or("").to_string(),
        custom_parameters,
        proxy,
        replay_proxy: args.value_of("replay-proxy").unwrap_or("").to_string(),
        replay_once: args.is_present("replay-once"),
        output_file: args.value_of("output").unwrap_or("").to_string(),
        save_responses: args.value_of("save-responses").unwrap_or("").to_string(),
        output_format: args.value_of("output-format").unwrap_or("").to_string(),
        append: args.is_present("append"),
        remove_empty: args.is_present("remove-empty"),
        force: args.is_present("force"),
        strict: args.is_present("strict"),
        disable_progress_bar: args.is_present("disable-progress-bar"),
        progress_bar_len,
        follow_redirects: args.is_present("follow-redirects"),
        test: args.is_present("test"),
        verbose,
        learn_requests_count,
        concurrency,
        workers,
        timeout,
        recursion_depth,
        verify: args.is_present("verify"),
        reflected_only: args.is_present("reflected-only"),
        http_version,
        template: convert_to_string_if_some(args.value_of("parameter-template")),
        joiner: convert_to_string_if_some(args.value_of("joiner")),
        encode: args.is_present("encode"),
        disable_custom_parameters: args.is_present("disable-custom-parameters"),
        one_worker_per_host: args.is_present("one-worker-per-host"),
        invert: args.is_present("invert"),
        headers_discovery: args.is_present("headers-discovery"),
        body,
        delay,
        custom_headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        data_type,
        max,
        disable_colors: args.is_present("disable-colors"),
        disable_trustdns: args.is_present("disable-trustdns"),
        check_binary: args.is_present("check-binary"),
    })
}
