[![Twitter](https://img.shields.io/twitter/follow/sh1yo_.svg?logo=twitter)](https://twitter.com/sh1yo_)

![crates.io](https://img.shields.io/crates/v/x8.svg)
![stars](https://img.shields.io/github/stars/Sh1Yo/x8)
![crates_downloads](https://img.shields.io/crates/d/x8?logo=rust)
![github_downloads](https://img.shields.io/github/downloads/sh1yo/x8/total?label=downloads&logo=github)
![lang](https://img.shields.io/github/languages/top/sh1yo/x8)

<h1 align="center">x8</h1>

<h3 align="center">Hidden parameters discovery suite written in Rust.</h3>

<p align="center"><a href="https://asciinema.org/a/oAMn0LK0NciNHgzirYJClyB2v" target="_blank"><img src="https://asciinema.org/a/oAMn0LK0NciNHgzirYJClyB2v.svg" /></a></p>

The tool helps to find hidden parameters that can be vulnerable or can reveal interesting functionality that other hunters miss. Greater accuracy is achieved thanks to the line-by-line comparison of pages, comparison of response code and reflections.


- [Features](#features)
- [Examples](#examples)
    - [Send parameters via query](#send-parameters-via-query)
    - [Send parameters via body](#send-parameters-via-body)
    - [Custom template](#custom-template)
    - [Variables](#variables)
    - [Percent encoding](#percent-encoding)
- [Test](#test)
- [Usage](#usage)
- [Troubleshooting](#troubleshooting)
- [Installation](#installation)
- [Donation](#donation)

# Features
- A lot of things to customize: key template, value template, encodings, and even injection points.
- Supports 6 main methods: GET, POST, PUT, PATCH, DELETE, HEAD.
- Has built in 2 main body types: json, urlencode.
- Able to discover parameters with not random value, like admin=true
- Compares responses line-by-line.
- Adds to every request cachebuster by default.

# Examples
#### Send parameters via query
```bash
x8 -u "https://example.com/" -w <wordlist>
```

With some default parameters:
```bash
x8 -u "https://example.com/?something=1" -w <wordlist>
```

`/?something=1` equals to `/?something=1&%s`

#### Send parameters via body
```bash
x8 -u "https://example.com/" -X POST --as-body -w <wordlist>
```

Or with a custom body:
```bash
x8 -u "https://example.com/" -X POST --as-body -b '{"x":{%s}}' -w <wordlist>
```
`%s` will be replaced with different parameters like `{"x":{"a":"b3a1a", "b":"ce03a", ...}}`

#### Custom template
```bash
x8 -u "https://example.com/" --param-template "user[%k]=%v&" -w <wordlist>
```

Now every request would look like `/?user[a]=hg2s4&user[b]=a34fa&...`

It is even possible to imitate not included body types, for example, application/xml:

```bash
x8 -u "https://example.com/" --as-body --param-template "<%k>%v</%k>" -H "Content-Type: application/xml" -b "<?xml version="1.0" ?>%s" -w <wordlist>
```

#### Variables
In the next example, `something` will take on new values every request:
```bash
x8 -u "https://example.com/?something={{random}}&%s" -w <wordlist>
```

#### Percent encoding
Sometimes parameters should be encoded. It is also possible:

```bash
x8 -u "https://example.com/?path=..%2faction.php%3f%s%23" --encode -w <wordlist>
```

```http
GET /?path=..%2faction.php%3fWTDa8%3Da7UOS%26rTIDA%3DexMFp...%23 HTTP/1.1
Host: example.com
```

# Test
Feel free to check whether the tool works as expected and compare it with other tools at https://4rt.one/.
There are 2 reflected parameters, 4 parameters that change code/headers/body, and one extra parameter with a not random value.

# Usage

```
USAGE:
    x8 [FLAGS] [OPTIONS]

FLAGS:
        --as-body                        Send parameters via body.
                                         Built in body types that can be detected automatically: json, urlencode
        --disable-cachebuster
        --disable-colors
        --disable-custom-parameters      Do not check automatically parameters like admin=true
        --disable-progress-bar
    -C, --disable-response-correction    Do not beautify responses before processing. Reduces accuracy.
        --encode                         Encodes query or body before a request, i.e & -> %26, = -> %3D
                                         List of chars to encode: ", `, , <, >, &, #, ;, /, =, %
    -L, --follow-redirects               Follow redirections
        --force                          Ignore 'binary data detected', 'the page is too huge', 'param_template lacks
                                         variables' error messages
    -h, --help                           Prints help information
        --http2                          Use http/2 instead of http/1.1
        --insecure                       Use http instead of https when the request file is used
        --is-json                        If the output is valid json and the content type does not contain 'json'
                                         keyword - specify this argument for a more accurate search
        --replay-once                    If replay proxy is specified, send all found parameters within one request
        --test                           Prints request and response
    -V, --version                        Prints version information

OPTIONS:
    -b, --body <body>                                       Example: --body '{"x":{%s}}'
                                                            Available variables: {{random}}
    -t, --body-type <body type>
            Available: urlencode, json. (default is "urlencode")
            Can be detected automatically if --body is specified
    -c <concurrency>                                        The number of concurrent requests (default is 1)
        --custom-parameters <custom-parameters>
            Check these parameters with non-random values like true/false yes/no
            (default is "admin bot captcha debug disable encryption env show sso test waf")
        --custom-values <custom-values>
            Check custom parameters with these values (default is "1 0 false off null true yes no")

    -d, --delay <Delay between requests in milliseconds>
    -H, --header <headers>                                  Example: -H 'one:one' 'two:two'
        --learn-requests <learn_requests_count>             Set the custom number of learning requests. (default is 9)
    -m, --max <max>
            Change the maximum number of parameters. (default is 128/192/256 for query and 512 for body)

    -X, --method <method>
            Available: GET, POST, PUT, PATCH, DELETE, HEAD. (default is "GET")

    -o, --output <file>
    -O, --output-format <output-format>                     standart, json, url (default is "standart")
    -P, --param-template <parameter_template>
            %k - key, %v - value. Example: --param-template 'user[%k]=%v&'

    -x, --proxy <proxy>
        --replay-proxy <replay-proxy>
            Request target with every found parameter via replay proxy at the end

    -r, --request <request>                                 The file with raw http request
        --save-responses <save-responses>                   Save matched responses to a directory
    -u, --url <url>                                         You can add a custom injection point with %s
        --value-size <value_size>
            Custom value size. Affects {{random}} variables as well (default is 5)

    -v, --verbose <verbose>                                 Verbose level 0/1/2 (default is 1)
    -w, --wordlist <wordlist>                               The file with parameters
```


# Troubleshooting
I chose the POST/PUT method and/or provided a body, but the tool sends parameters via query.
- make sure you are adding --as-body flag.

The tool fails to send requests via <a href="https://portswigger.net/burp">burp suite proxy</a>.
- try to use --http2 flag.

# Installation
- Linux
    - from releases
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```
    - using cargo install
        ```bash
        cargo install x8
        ```
- Mac
    - currently, there are no binaries for Mac OS
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```
    - using cargo install
        ```bash
        cargo install x8
        ```

- Windows
    - from releases

# Donation
Want to support the project? You can donate to the following addresses:

Monero: 46pni5AY9Ra399sivBykVucaK6KdU3rYiSqFsZinfaEgd3qUkeZvRxjEdhPPmsmZQwTDPBSrvSpkaj4LsHqLH6GG7zMmgiW

Bitcoin: bc1q8q9hfmejxd65jcrszwpgj7xnwhy32gpxay2h604xwvjwtw8jh8vq8kev5r
