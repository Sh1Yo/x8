[![Twitter](https://img.shields.io/twitter/follow/sh1yo_.svg?logo=twitter)](https://twitter.com/sh1yo_)
![stars](https://img.shields.io/github/stars/Sh1Yo/x8)
[![issues](https://img.shields.io/github/issues/sh1yo/x8?color=%20%237fb3d5%20)](https://github.com/sh1yo/x8/issues)

[![Latest Version](https://img.shields.io/github/release/sh1yo/x8.svg?style=flat-square)](https://github.com/sh1yo/x8/releases)
[![crates.io](https://img.shields.io/crates/v/x8.svg)](https://crates.io/crates/x8)
[![crates_downloads](https://img.shields.io/crates/d/x8?logo=rust)](https://crates.io/crates/x8)
[![github_downloads](https://img.shields.io/github/downloads/sh1yo/x8/total?label=downloads&logo=github)](https://github.com/sh1yo/x8/releases)

<!-- ![lang](https://img.shields.io/github/languages/top/sh1yo/x8) -->

<h1 align="center">x8</h1>

<h3 align="center">Hidden parameters discovery suite written in Rust.</h3>

<p align="center"><img src=https://user-images.githubusercontent.com/54232788/212553519-f4cdfb1c-f5f2-4238-a6a8-e9393b98b529.gif></p>

The tool aids in identifying hidden parameters that could potentially be vulnerable or reveal interesting functionality that may be missed by other testers. Its high accuracy is achieved through line-by-line comparison of pages, comparison of response codes, and reflections.

# Documentation

The documentation that explains every feature can be accessed at [https://sh1yo.art/x8docs/](https://sh1yo.art/x8docs/). The source of the documentation is located at [/docs.md](docs.md).

# Tree

- [Features](#features)
- [Examples](#examples)
- [Test site](#test-site)
- [Usage](#usage)
- [Wordlists](#wordlists)
- [Burp Suite integration](#burp-suite-integration)
- [Installation](#installation)

# Features

- Fast.
- Offers flexible request configuration through the use of templates and injection points.
- Highly scalable, capable of checking thousands of URLs per run.
- Provides higher accuracy compared to similar tools, especially in difficult cases.
- Capable of discovering parameters with non-random values, such as admin=true.
- Highly configurable with a wide range of customizable options.
- Achieves almost raw requests through external library modification.

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
<!-- `https://4rt.one/level2` (POST JSON) -->
`https://4rt.one/level3` (GET)

# Usage

```
USAGE:
    x8 [FLAGS] [OPTIONS]

FLAGS:
        --append                       Append to the output file instead of overwriting it.
    -B                                 Equal to -x http://localhost:8080
        --check-binary                 Check the body of responses with binary content types
        --disable-additional-checks    Private
        --disable-colors
        --disable-custom-parameters    Do not automatically check parameters like admin=true
        --disable-progress-bar
        --disable-trustdns             Can solve some dns related problems
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
            (default is <= 256 for query, 64 for headers and 512 for body)
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

1. Launch Burp Suite and navigate to the 'Extender' tab.
2. Locate and install the 'Custom Send To' extension from the BApp Store.
3. Open the 'Send to' tab and click on the 'Add' button to configure the extension.

Give a name to the entry and insert the following line into the command:

```
/path/to/x8 --progress-bar-len 20 -c 3 -r %R -w /path/to/wordlist --proto %T --port %P
```

You can also add your frequently used arguments like `--output-format`,`--replay-proxy`, `--recursion-depth`, ..

**NOTE** if the progress bar doesn't work properly --- try to reducing the value of `--progress-bar-len`.

Switch from Run in background to Run in terminal.

![image](https://user-images.githubusercontent.com/54232788/201471567-2a388157-e2f1-4d68-aebe-5ecc3c1090ee.png)

If you encounter issues with font rendering in the terminal, you can adjust the `xterm` options in **Send to Miscellaneous Options**. Simply replace the existing content with `xterm -rv -fa 'Monospace' -fs 10 -hold -e %C`, or substitute `xterm` with your preferred terminal emulator.

Now you can go to the proxy/repeater tab and send the request to the tool:

![image](https://user-images.githubusercontent.com/54232788/201518132-87fd0c40-5877-4f46-a036-590967759b3f.png)

In the next dialog, you can modify the command and execute it in a new terminal window.

![image](https://user-images.githubusercontent.com/54232788/201518230-3d7959c4-3530-497d-9aca-b20de80321cb.png)

After executing the command, a new terminal window will appear, displaying the running tool.

![image](https://user-images.githubusercontent.com/54232788/224473570-cabbd4ee-8c15-4a09-bc2a-c660c534a429.jpg)

# Installation

**NOTE**: Starting with v4.0.0, installing via `cargo install` uses the `crate` branch instead of `main`. This branch includes the original `reqwest` library that performs HTTP normalizations and prevents sending invalid requests. If you want to use the modified reqwest version without these limitations, I recommend installing via the `Releases` page or building the sources.

- Docker
    - installation
        ```bash
        git clone https://github.com/Sh1Yo/x8
        cd x8
        docker build -t x8 .
        ```
    - [usage](https://github.com/Sh1Yo/x8/pull/29)

- Linux
    - from releases
    - from blackarch repositories (repositories should be installed)
        ```bash
        # pacman -Sy x8
        ```
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/sh1yo/x8
        cd x8
        cargo build --release
        # move the binary to $PATH so you can use it without specifying the full path
        cp ./target/release/x8 /usr/local/bin 
        # if it says that /usr/local/bin doesn't exists you can try
        # sudo cp ./target/release/x8 /usr/bin
        ```
    - via cargo install
        ```bash
        cargo install x8
        ```
- Mac
    - from source code (rust should be installed)
        ```bash
        git clone https://github.com/sh1yo/x8
        cd x8
        cargo build --release
        # move the binary to $PATH so you can use it without specifying the full path
        cp ./target/release/x8 /usr/local/bin 
        ```
    - via cargo install
        ```bash
        cargo install x8
        ```

- Windows
    - from releases
