
<h1 align="center">x8</h1>

<h3 align="center">Hidden parameters discovery suite written in Rust.</h3>

<p align="center"><a href="https://asciinema.org/a/6bLxIIDdBqcgws84clSO35C2y" target="_blank"><img src="https://asciinema.org/a/6bLxIIDdBqcgws84clSO35C2y.svg" /></a></p>

- [How does it work](#how-does-it-work)
- [Features](#features)
- [Examples](#examples)
    - [Send parameters via query]("send-parameters-via-query")
    - [Send parameters via body](#send-parameters-via-body)
    - [Custom template](#custom-template)
    - [Variables](#variables)
    - [Percent encoding](#percent-encoding)
- [Test](#test)
- [Usage](#usage)
- [Installation](#installation)
- [Donation](#donation)

# How does it work
Firstly, it makes a few basic requests to learn the target, and then it tries to adjust the optimal amount of parameters in every request. Next requests contain parameters from the user-supplied list. If the response has unique differences - parameters from the request are split into two heaps and added to the queue for another cycle. Cycles repeat until there remains one parameter in every heap that causes a unique difference.

# Features
- A lot of things to customize: key template, value template, encodings, and even injection points.
- Supports 6 main methods: GET, POST, PUT, PATH, DELETE, HEAD.
- Has built in 2 main body types: json, urlencode.
- Able to discover parameters with not random value, like admin=true
- Uses fast GNU diff as a response comparer.
- Adds to every request cachebuster by default.

# Examples
#### Send parameters via query
```x8 -u https://example.com/ -w <wordlist>```

With some default parameters:

```x8 -u https://example.com/?something=1 -w <wordlist>```

`/?something=1` equals to `/?something=1&%s`

#### Send parameters via body
`x8 -u https://example.com/ -X POST --as-body -w <wordlist>`

Or with a custom body:

```x8 -u https://example.com/ -X POST --as-body -b '{"x":{%s}}' -w <wordlist>```

`%s` will be replaced with different parameters like `{"x":{"a":"b3a1a", "b":"ce03a", ...}}`

#### Custom template
```x8 -u https://example.com/ --key-template user[%s] -w <wordlist>```

Now every request would look like `/?user[a]=hg2s4&user[b]=a34fa&...`

#### Variables
In the next example, `something` will take on new values every request:

```x8 -u https://example.com/?something={{random}}&%s -w <wordlist>```

#### Percent encoding
Sometimes parameters should be encoded. It is also possible:

```x8 -u https://example.com/?path=..%2faction.php%3f%s%23 --encode -w <wordlist>```

```http
GET /?path=..%2faction.php%3fWTDa8%3Da7UOS%26rTIDA%3DexMFp...%23 HTTP/1.1
Host: example.com
```

# Test
Feel free to check whether the tool works as expected and compare it with other tools at https://4rt.one/.
There are 2 reflected parameters, 4 parameters that change code/headers/body, and one extra parameter with a not random value.

# Usage

```
    x8 [FLAGS] [OPTIONS]

FLAGS:
        --as-body                        Send parameters via body
        --disable-cachebuster
        --disable-colors
        --disable-custom-parameters      Do not check automatically parameters like admin=true
        --disable-progress-bar
    -c, --disable-response-correction    Do not beautify responses before processing. Reduces accuracy.
        --encode                         Encodes query or body before a request, i.e & -> %26, = -> %3D
                                         List of chars to encode: ", `, , <, >, &, #, ;, /, =, %
    -L, --follow-redirects               Follow redirections
        --force-binary                   Ignore 'binary data detected' message
    -h, --help                           Prints help information
        --insecure                       Use http instead of https when the request file is used
        --is-json                        If the output is valid json and the content type does not contain 'json'
                                         keyword - specify this argument for a more accurate search
        --replay-once                    If replay proxy is specified, send all found parameters within one request
        --test                           Prints request and response
        --version                        Prints version information

OPTIONS:
    -b, --body <body>                                       Example: --body '{"x":{%s}}'
                                                            Available variables: {{random}}
    -t, --body-type <body type>
            Available: urlencode, json. (default is "urlencode")
            Can be detected automatically if --body is specified
    -l, --diff-location <custom-diff-location>              Default: takes from $PATH
        --custom-parameters <custom-parameters>
            Check these parameters with non-random values like true/false yes/no
            (default is "admin bot captcha debug disable encryption env show sso test waf")
        --custom-values <custom-values>
            Check custom parameters with these values (default is "1 0 false off null true yes no")

    -d, --delay <Delay between requests in milliseconds>
    -H, --header <headers>                                  Example: -H 'one:one' 'two:two'
    -K, --key-template <key_template>                       Example: --key-template 'user[%s]'
    --learn-requests <learn_requests_count>                 Set the custom number of learning requests. (default is 10)
    -m, --max <max>
            Change the maximum number of parameters. (default is 128/192/256 for query and 512 for body)

    -X, --method <method>
            Available: GET, POST, PUT, PATH, DELETE, HEAD. (default is "GET")

    -o, --output <file>
    -x, --proxy <proxy>
        --replay-proxy <replay-proxy>
            Request target with every found parameter via replay proxy at the end

    -r, --request <request>                                 The file with raw http request
        --save-responses <save-responses>                   Save matched responses to a directory
        --tmp-directory <tmp-directory>                     Directory for response comparing. Default: /tmp
    -u, --url <url>                                         You can add a custom injection point with %s
        --value-size <value_size>
            Custom value size. Affects {{random}} variables as well (default is 5)

    -V, --value-template <value_template>                   Example: --value-template 'https://example.com/%s'
    -v, --verbose <verbose>                                 Verbose level 0/1/2 (default is 1)
    -w, --wordlist <wordlist>                               The file with parameters
```

<a name="Installation"/>

# Installation
**You need gnu diff**. If you are using a Linux distributive then most likely it is already installed in your system. You can check whether it is installed or not by running `diff --help` if you see `command not found: diff` install diffutils package. Unfortunately, Windows does not support gnu diff.

- Linux
    - from releases
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```
- Mac
    - currently, there are no binaries for Mac OS
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```

- Windows
    - Windows is not supported for this moment, but it is still possible to run the tool via wsl.

# Donation
Want to support the project? You can donate to the following addresses:

Monero: 46pni5AY9Ra399sivBykVucaK6KdU3rYiSqFsZinfaEgd3qUkeZvRxjEdhPPmsmZQwTDPBSrvSpkaj4LsHqLH6GG7zMmgiW

Bitcoin: bc1q8q9hfmejxd65jcrszwpgj7xnwhy32gpxay2h604xwvjwtw8jh8vq8kev5r
