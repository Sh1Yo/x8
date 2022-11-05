use crate::{config::{structs::Config, utils::{parse_request, convert_to_string_if_some}}, network::utils::DataType};
use clap::{crate_version, App, AppSettings, Arg};
use std::{collections::HashMap, fs, error::Error};
use tokio::time::Duration;
use snailquote::unescape;
use url::Url;

pub fn get_config() -> Result<Config, Box<dyn Error>> {

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
        .arg(Arg::with_name("split-by")
            .long("split-by")
            .help("Split request into lines by provided sequence. By default splits by \\r, \\n and \\r\\n")
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
            .help("How to join parameter templates. Example: --joiner '&'\nDefault: urlencoded - '&', json - ', ', headers - '; '")
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
            Arg::with_name("method")
                .short("X")
                .long("method")
                .value_name("method")
                .default_value("GET")
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
                .help("By default parameters are sent within the body only in case PUT or POST methods are used.
It's possible to overwrite this behaviour by specifying the option")
        )
        .arg(
            Arg::with_name("headers-discovery")
                .long("headers")
                .help("Switch to header discovery mode.\nForbidden chars would be automatically removed from headers names")
                .conflicts_with("indert")
                .conflicts_with("param-template")
        )
        .arg(
            Arg::with_name("force")
                .long("force")
                .help("Ignore 'binary data detected', 'the page is too huge', 'param_template lacks variables' error messages")
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
            Arg::with_name("strict")
                .long("strict")
                .help("Only report parameters that've changed the different parts of a page")
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
                .help("Verbose level 0/1")
                .default_value("1")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("save-responses")
                .long("save-responses")
                .help("Save request and response to a directory in case the parameter is found")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("disable-cachebuster")
                .long("disable-cachebuster")
        )
        .arg(
            Arg::with_name("learn_requests_count")
                .long("learn-requests")
                .help("Set the custom number of learning requests.")
                .default_value("9")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("recursion_depth")
                .long("recursion-depth")
                .help("Check the same list of parameters with the found parameters until there are no new parameters to be found.
Conflicts with --verify for now. Will be changed in the future.")
                .default_value("0")
                .takes_value(true)
                .conflicts_with("verify")
        )
        .arg(
            Arg::with_name("max")
                .short("m")
                .long("max")
                .help("Change the maximum number of parameters.\n(default is 128/192/256 for query, 64/128/196 for headers and 512 for body)")
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
                .help("The number of concurrent url checks")
                .default_value("1")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("verify")
                .long("verify")
                .help("Verify found parameters one more time.")
        )
        .arg(
            Arg::with_name("reflected-only")
                .long("reflected-only")
                .help("Disable page comparison and search for reflected parameters only.")
        )
        .arg(
            Arg::with_name("one-worker-per-host")
                .long("one-worker-per-host")
                .help("Multiple urls with the same host will be checked one after another, while urls with different hosts - are in parallel.")
        )
        .arg(
            Arg::with_name("http")
                .long("http")
                .help("HTTP version. Supported versions: --http 1.1, --http 2")
                .takes_value(true)
        );

    let args = app.clone().get_matches();

    if args.value_of("url").is_none() && args.value_of("request").is_none() {
        Err("A target was not provided")?;
    }

    // parse numbers
    let delay = Duration::from_millis(args.value_of("delay").unwrap().parse()?);

    let learn_requests_count = args.value_of("learn_requests_count").unwrap().parse()?;
    let concurrency = args.value_of("concurrency").unwrap().parse()?;
    let workers = args.value_of("workers").unwrap().parse()?;
    let verbose = args.value_of("verbose").unwrap().parse()?;
    let timeout = args.value_of("timeout").unwrap().parse()?;
    let recursion_depth = args.value_of("recursion_depth").unwrap().parse()?;

    let max: Option<usize> = if args.is_present("max") {
        Some(args.value_of("max").unwrap().parse()?)
    } else {
        None
    };

    // try to read request file
    let request = match args.value_of("request") {
        Some(val) => fs::read_to_string(val)?,
        None => String::new(),
    };

    // parse the default request information
    // either via the request file or via provided parameters
    let (
        methods,
        urls,
        headers,
        body,
        data_type,
    ) = if !request.is_empty() {
        // if the request file is specified - get protocol (https/http) from args, specify scheme and port, and parse request file
        let proto = args.value_of("proto").ok_or("--proto wasn't provided")?.to_string();

        let scheme = proto.replace("://", "");

        let port: u16 = if args.value_of("port").is_some() {
            args.value_of("port").unwrap().parse()?
        } else {
            if scheme == "https" {
                443
            } else {
                80
            }
        };

        parse_request(
            &request,
            &scheme,
            port,
            args.value_of("split-by")
        )?
    } else {
        // parse everything from user-supplied command line arguments
        let methods = args.values_of("method").unwrap().map(|x| x.to_string()).collect::<Vec<String>>();

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
                ].concat();

                headers.insert(key, value);
            }
        };

        // set default headers if weren't specified by a user.
        if !headers.keys().any(|i| i.contains("User-Agent")) {
            headers.insert("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/83.0.4103.97 Safari/537.36".to_string());
        }

        // TODO return cachebuster in query as well
        if !args.is_present("disable-cachebuster") {
            if !headers.keys().any(|i| i.contains("Accept")) {
                headers.insert("Accept", "*/*, text/{{random}}".to_string());
            }
            if !headers.keys().any(|i| i.contains("Accept-Language")) {
                headers.insert("Accept-Language","en-US, {{random}};q=0.9, *;q=0.5".to_string());
            }
            if !headers.keys().any(|i| i.contains("Accept-Charset")) {
                headers.insert("Accept-Charset","utf-8, iso-8859-1;q=0.5, {{random}};q=0.2, *;q=0.1".to_string());
            }
        }

        // TODO replace with ".parse()" or sth like it
        let data_type = match args.value_of("data-type") {
            Some(val) => if val == "json" {
                Some(DataType::Json)
            } else if val == "urlencoded" {
                Some(DataType::Urlencoded)
            } else {
                Err("Incorrect --data-type specified")?
            },
            None => None
        };

        let urls =
            args.values_of("url")
                .unwrap()
                .map(|x| Url::parse(x)).collect::<Vec<Result<Url, url::ParseError>>>();

        // in case there's at least a single wrong url -- return with an error
        if urls.iter().any(|x| x.is_err()) {
            for err_url in urls.iter().filter(|x| x.is_err()) {
                err_url.to_owned()?;
            };
            unreachable!();
        } else {
            (
                methods,
                urls.iter().map(|x| x.as_ref().unwrap().to_string()).collect::<Vec<String>>(),
                headers,
                unescape(&args.value_of("body").unwrap_or("").to_string())?,
                data_type,
            )
        }
    };

    // generate custom param values like admin=true
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

    // disable colors
    if args.is_present("disable-colors") {
        colored::control::set_override(false);
    }

    // TODO maybe replace empty with None
    Ok(Config {
        urls,
        methods,
        wordlist: args.value_of("wordlist").unwrap_or("").to_string(),
        custom_parameters,
        proxy: args.value_of("proxy").unwrap_or("").to_string(),
        replay_proxy: args.value_of("replay-proxy").unwrap_or("").to_string(),
        replay_once: args.is_present("replay-once"),
        output_file: args.value_of("output").unwrap_or("").to_string(),
        save_responses: args.value_of("save-responses").unwrap_or("").to_string(),
        output_format: args.value_of("output-format").unwrap_or("").to_string(),
        append: args.is_present("append"),
        force: args.is_present("force"),
        strict: args.is_present("strict"),
        disable_progress_bar: args.is_present("disable-progress-bar"),
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
        http: args.value_of("output").unwrap_or("").to_string(),
        template: convert_to_string_if_some(args.value_of("parameter-template")),
        joiner: convert_to_string_if_some(args.value_of("joiner")),
        encode: args.is_present("encode"),
        disable_custom_parameters: args.is_present("disable-custom-parameters"),
        one_worker_per_host: args.is_present("one-worker-per-host"),
        invert: args.is_present("invert"),
        headers_discovery: args.is_present("headers-discovery"),
        body,
        delay,
        custom_headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        data_type,
        max,
    })
}