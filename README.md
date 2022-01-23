[![Twitter](https://img.shields.io/twitter/follow/sh1yo_.svg?logo=twitter)](https://twitter.com/sh1yo_)

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/B0B858X5E)

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
    - [Headers](#headers)
    - [Header values](#header-values)
- [Test](#test)
- [Usage](#usage)
- [Troubleshooting](#troubleshooting)
- [Limitations](#limitations)
- [Wordlists](#wordlists)
- [Burp Suite integrations](#burp-suite-integrations)
- [Installation](#installation)

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

#### Headers

With v3.0.0 it is possible to discover headers as well:

```bash
x8 -u "https://example.com" --headers -w <wordlist>
```

#### Header values

You can also target single headers:

```bash
x8 -u "https://example.com" -H "Cookie: %s" -w <wordlist>
```

# Test

Feel free to check whether the tool works as expected and compare it with other tools at https://4rt.one/index.html.
There are 2 reflected parameters, 4 parameters that change code/headers/body, and one extra parameter with a not random value.

# Usage

```
USAGE:
    x8 [FLAGS] [OPTIONS]

FLAGS:
        --append                         Append to the output file instead of overwriting it.
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
        --headers                        Switch to header discovery mode.
                                         Forbidden chars would be automatically removed from headers names
        --http2                          Prefer http/2 over http/1.1
        --is-json                        If the output is valid json and the content type does not contain 'json'
                                         keyword - specify this argument for a more accurate search
        --keep-newlines                  --body 'a\r\nb' -> --body 'a{{new_line}}b'.
                                         Works with body and parameter templates only.
        --reflected-only                 Disable page comparison and search for reflected parameters only.
        --replay-once                    If replay proxy is specified, send all found parameters within one request.
        --test                           Prints request and response
    -V, --version                        Prints version information
        --verify                         Verify found parameters one more time.

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
    -H <headers>                                            Example: -H 'one:one' 'two:two'
        --learn-requests <learn_requests_count>             Set the custom number of learning requests. (default is 9)
    -m, --max <max>
            Change the maximum number of parameters.
            (default is 128/192/256 for query, 64/128/196 for headers and 512 for body)
    -X, --method <method>
            Available: GET, POST, PUT, PATCH, DELETE, HEAD. (default is "GET")

    -o, --output <file>
    -O, --output-format <output-format>                     standart, json, url, request (default is "standart")
    -P, --param-template <parameter_template>
            %k - key, %v - value. Example: --param-template 'user[%k]=%v&'

        --proto <proto>                                     Protocol to use with request file (default is "https")
    -x, --proxy <proxy>
        --replay-proxy <replay-proxy>
            Request target with every found parameter via replay proxy at the end.

    -r, --request <request>                                 The file with the raw http request
        --save-responses <save-responses>                   Save matched responses to a directory
    -u, --url <url>                                         You can add a custom injection point with %s.
        --value-size <value_size>
            Custom value size. Affects {{random}} variables as well (default is 7)

    -v, --verbose <verbose>                                 Verbose level 0/1/2 (default is 1)
    -w, --wordlist <wordlist>                               The file with parameters
```


# Troubleshooting

I chose the POST/PUT method and/or provided a body, but the tool sends parameters via query.
- make sure you are adding --as-body flag.

The tool fails to send requests via <a href="https://portswigger.net/burp">burp suite proxy</a>.
- try to use --http2 flag.

# Limitations

- Currently, it is impossible to use some non-regular paths like `/sth1/../sth2`.

# Wordlists
Parameters:
- [samlists](https://github.com/the-xentropy/samlists)
- [arjun](https://github.com/s0md3v/Arjun/tree/master/arjun/db)

Headers:
- [Param Miner](https://github.com/danielmiessler/SecLists/tree/master/Discovery/Web-Content/BurpSuite-ParamMiner)

# Burp Suite integrations

It is possible to run parameter discovery in a few clicks using burp suite extensions:

## [x8-Burp](https://github.com/Impact-I/x8-Burp)
![preview](https://user-images.githubusercontent.com/54232788/126073100-ed09e8b1-0ffa-4432-aa34-f0451586a992.png)

## [Send To](https://portswigger.net/bappstore/f089f1ad056545489139cb9f32900f8e)

### Setting up

1. Open Burp Suite and go to the extender tab.
2. Find and install the "Custom Send To" extension in BApp Store.
3. Go to the "Send to" tab and click Add.

Name - x8 query.

Command - `/path/to/x8 -r %R -w wordlist.txt --proto %T`. You can also add your frequently used arguments like `--output-format`,`--replay-proxy`, `-c`...

Then switch from Run in background to Run in terminal.

![command](https://user-images.githubusercontent.com/54232788/125414936-edf8872d-d3ba-4a7e-8bb1-3af0d7685e48.png)

4. Repeat step 3 with Name - "x8 body" and add `--as-body` flag to the Command.

Now you can go to the proxy/repeater tab and send the request to the tool:

![extension_tab](https://user-images.githubusercontent.com/54232788/124345628-2f29d400-dbeb-11eb-991e-7ba1a2800522.png)

In the next dialog, you can change the command and run it in a new terminal window.

![dialog](https://user-images.githubusercontent.com/54232788/125414941-9404ac7b-e1e0-4a33-ac1a-aaf2cad0c231.png)


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
