[![Twitter](https://img.shields.io/twitter/follow/sh1yo_.svg?logo=twitter)](https://twitter.com/sh1yo_)
![github_downloads](https://img.shields.io/github/downloads/sh1yo/x8/total?label=downloads&logo=github)
![stars](https://img.shields.io/github/stars/Sh1Yo/x8)
<!-- ![crates.io](https://img.shields.io/crates/v/x8.svg) -->
<!-- ![lang](https://img.shields.io/github/languages/top/sh1yo/x8) -->
<!-- ![crates_downloads](https://img.shields.io/crates/d/x8?logo=rust) -->

<h1 align="center">x8</h1>

<h3 align="center">Hidden parameters discovery suite written in Rust.</h3>

<p align="center"><a href="https://asciinema.org/a/HwNa4PC2OZODxyazxCWORF9wB" target="_blank"><img src="https://asciinema.org/a/HwNa4PC2OZODxyazxCWORF9wB.svg" /></a></p>

The tool helps to find hidden parameters that can be vulnerable or can reveal interesting functionality that other testers miss. Great accuracy is achieved thanks to the line-by-line comparison of pages, comparison of response code and reflections.

# Documentation

The documentation that explains every feature can be found on [https://sh1yo.art/x8docs/](https://sh1yo.art/x8docs/).

- [Features](#features)
- [Examples](#examples)
- [Test site](#test-site)
- [Usage](#usage)
- [Wordlists](#wordlists)
- [Burp Suite integration](#burp-suite-integration)
- [Installation](#installation)

# Features

- Fast.
- Has flexible request configuration thanks to the concept of templates and injection points.
- Scalability. The tool can check up to thousands of urls per run.
- More accurate than analogs, especially in diffictult cases.
- Can discover parameters with not random values, like admin=true.
- Highly configurable.
- Almost raw requests were achieved due to the external lib modification.

# Examples

#### Check parameters in query

```bash
x8 -u "https://example.com/" -w <wordlist>
```

With default parameters:
```bash
x8 -u "https://example.com/?something=1" -w <wordlist>
```

`/?something=1` equals to `/?something=1&%s`

#### Send parameters via body

```bash
x8 -u "https://example.com/" -X POST -w <wordlist>
```

Or with a custom body:
```bash
x8 -u "https://example.com/" -X POST -b '{"x":{%s}}' -w <wordlist>
```
`%s` will be replaced with different parameters like `{"x":{"a":"b3a1a", "b":"ce03a", ...}}`

#### Check multiple urls in paralell

```bash
x8 -u "https://example.com/" "https://4rt.one/" -W0
```

#### Custom template

```bash
x8 -u "https://example.com/" --param-template "user[%k]=%v" -w <wordlist>
```

Now every request would look like `/?user[a]=hg2s4&user[b]=a34fa&...`

#### Percent encoding

Sometimes parameters should be encoded. It is also possible:

```bash
x8 -u "https://example.com/?path=..%2faction.php%3f%s%23" --encode -w <wordlist>
```

```http
GET /?path=..%2faction.php%3fWTDa8%3Da7UOS%26rTIDA%3DexMFp...%23 HTTP/1.1
Host: example.com
```

#### Search for headers

```bash
x8 -u "https://example.com" --headers -w <wordlist>
```

#### Search for header values

You can also target single headers:

```bash
x8 -u "https://example.com" -H "Cookie: %s" -w <wordlist>
```

# Test site

You can check the tool and compare it with other tools on the following urls:

`https://4rt.one/level1` (GET)
`https://4rt.one/level2` (POST JSON)
`https://4rt.one/level3` (GET)

# Usage

```
USAGE:
    x8 [FLAGS] [OPTIONS]

FLAGS:
        --append                       Append to the output file instead of overwriting it.
    -B                                 Equal to -x http://localhost:8080
        --disable-colors
        --disable-custom-parameters    Do not automatically check parameters like admin=true
        --disable-progress-bar
        --encode                       Encodes query or body before making a request, i.e & -> %26, = -> %3D
                                       List of chars to encode: ", `, , <, >, &, #, ;, /, =, %
    -L, --follow-redirects             Follow redirections
        --force                        Force searching for parameters on pages > 25MB. Remove an error in case there's 1
                                       worker with --one-worker-per-host option.
    -h, --help                         Prints help information
        --headers                      Switch to header discovery mode.
                                       NOTE Content-Length and Host headers are automatically removed from the list
        --invert                       By default, parameters are sent within the body only in case PUT or POST methods
                                       are used.
                                       It's possible to overwrite this behavior by specifying the option
        --mimic-browser                Add default headers that browsers usually set.
        --one-worker-per-host          Multiple urls with the same host will be checked one after another,
                                       while urls with different hosts - are in parallel.
                                       Doesn't increase the number of workers
        --reflected-only               Disable page comparison and search for reflected parameters only.
        --remove-empty                 Skip writing to file outputs of url:method pairs without found parameters
        --replay-once                  If a replay proxy is specified, send all found parameters within one request.
        --strict                       Only report parameters that have changed the different parts of a page
        --test                         Prints request and response
    -V, --version                      Prints version information
        --verify                       Verify found parameters.

OPTIONS:
    -b, --body <body>                                       Example: --body '{"x":{%s}}'
                                                            Available variables: {{random}}
    -c <concurrency>                                        The number of concurrent requests per url [default: 1]
        --custom-parameters <custom-parameters>
            Check these parameters with non-random values like true/false yes/no
            (default is "admin bot captcha debug disable encryption env show sso test waf")
        --custom-values <custom-values>
            Values for custom parameters (default is "1 0 false off null true yes no")

    -t, --data-type <data-type>
            Available: urlencode, json
            Can be detected automatically if --body is specified (default is "urlencode")
    -d, --delay <Delay between requests in milliseconds>     [default: 0]
    -H <headers>                                            Example: -H 'one:one' 'two:two'
        --http <http>                                       HTTP version. Supported versions: --http 1.1, --http 2
    -j, --joiner <joiner>
            How to join parameter templates. Example: --joiner '&'
            Default: urlencoded - '&', json - ', ', header values - '; '
        --learn-requests <learn-requests-count>             Set the custom number of learn requests. [default: 9]
    -m, --max <max>
            Change the maximum number of parameters per request.
            (default is 128/192/256 for query, 64 for headers and 512 for body)
    -X, --method <methods>                                  Multiple values are supported: -X GET POST
    -o, --output <file>
    -O, --output-format <output-format>                     standart, json, url, request [default: standart]
    -P, --param-template <parameter-template>
            %k - key, %v - value. Example: --param-template 'user[%k]=%v'
            Default: urlencoded - <%k=%v>, json - <"%k":%v>, headers - <%k=%v>
    -p, --port <port>                                       Port to use with request file
        --progress-bar-len <progress-bar-len>                [default: 26]
        --proto <proto>                                     Protocol to use with request file (default is "https")
    -x, --proxy <proxy>
        --recursion-depth <recursion-depth>
            Check the same list of parameters with the found parameters until there are no new parameters to be found.
            Conflicts with --verify for now.
        --replay-proxy <replay-proxy>
            Request target with every found parameter via the replay proxy at the end.

    -r, --request <request>                                 The file with the raw http request
        --save-responses <save-responses>
            Save request and response to a directory when a parameter is found

        --split-by <split-by>
            Split the request into lines by the provided sequence. By default splits by \r, \n and \r\n

        --timeout <timeout>                                 HTTP request timeout in seconds. [default: 15]
    -u, --url <url>
            You can add a custom injection point with %s.
            Multiple urls and filenames are supported:
            -u filename.txt
            -u https://url1 http://url2
    -v, --verbose <verbose>                                 Verbose level 0/1/2 [default: 1]
    -w, --wordlist <wordlist>
            The file with parameters (leave empty to read from stdin) [default: ]

    -W, --workers <workers>
            The number of concurrent url checks.
            Use -W0 to run everything in parallel [default: 1]
```

# Wordlists
Parameters:
- [samlists](https://github.com/the-xentropy/samlists)
- [arjun](https://github.com/s0md3v/Arjun/tree/master/arjun/db)

Headers:
- [Param Miner](https://github.com/danielmiessler/SecLists/tree/master/Discovery/Web-Content/BurpSuite-ParamMiner)

# Burp Suite integration

The burpsuite integration is done via the [send to](https://portswigger.net/bappstore/f089f1ad056545489139cb9f32900f8e) extension.

### Setting up

1. Open Burp Suite and go to the extender tab.
2. Find and install the "Custom Send To" extension from the BApp Store.
3. Go to the "Send to" tab and click Add.

Name the entry and insert the following line to the command:

```
/path/to/x8 --progress-bar-len 20 -c 3 -r %R -w /path/to/wordlist --proto %T --port %P
```

You can also add your frequently used arguments like `--output-format`,`--replay-proxy`, `--recursion-depth`, ..

**NOTE** if the progress bar doesn't work properly --- try to decrease the value of `--progress-bar-len`.

Switch from Run in background to Run in terminal.

![image](https://user-images.githubusercontent.com/54232788/201471567-2a388157-e2f1-4d68-aebe-5ecc3c1090ee.png)

If you experience bad fonts within the terminal, you can change the `xterm` options in Send To Miscellaneous Options. Just replace the content with `xterm -rv -fa 'Monospace' -fs 10 -hold -e %C`

Now you can go to the proxy/repeater tab and send the request to the tool:

![image](https://user-images.githubusercontent.com/54232788/201518132-87fd0c40-5877-4f46-a036-590967759b3f.png)

In the next dialog, you can change the command and run it in a new terminal window.

![image](https://user-images.githubusercontent.com/54232788/201518230-3d7959c4-3530-497d-9aca-b20de80321cb.png)

And a new terminal window with the running tool should open.

![image](https://user-images.githubusercontent.com/54232788/201518309-895054dc-b0b7-4892-907a-664e494bcd4f.png)

# Installation

**NOTE** starting with v4.0.0 the installation via `cargo install` isn't possible because I've changed a few http libs. I'll try to return this installation method in the future.

- Linux
    - from releases
    - from blackarch repositories (repositories should be installed)
        ```bash
        # pacman -Sy x8
        ```
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```
- Mac
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        cargo build --release
        ```

- Windows
    - from releases

- Docker
    - installation
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        docker build -t x8 .
        ```
    - [usage](https://github.com/Sh1Yo/x8/pull/29)
