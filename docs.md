- [User Interface](#user-interface)
- [Command line arguments](#command-line-arguments)
    - [http request from file](#http-request-from-file)
    - [http request from command-line arguments](#http-request-from-command-line-arguments-conflicts-with---request)
    - [Parameters](#parameters)
    - [Behavior](#behavior)
    - [Concurrency](#concurrency)
    - [Output](#output)


## User Interface

Usually, the tool's output looks like this:

![img](https://sh1yo.art/images/si9kh81QRe4PF9JF7g-4D0jGJ0ZAeDAW)

## Command line arguments

### http request from file

```
-r --request <filename>
```

This option specifies the file containing the raw HTTP request.

When using a request file, the tool does not add default headers such as `Accept` and `User-Agent` to the request.

At present, the URL is created directly from the Host header, so it is not possible to set an arbitrary Host header from within a request file. If you want to set a different Host header, see the `-H` option in the [HTTP Request from Command Line Arguments](#http-request-from-command-line-arguments) category.

```
--proto <http/https>
```

This argument is additional and required when using the `--request` option.

Specify either `http` or `https`.

```
--split-by <value>
```

This option specifies how to split the request file. By default, the `.lines()` method is used, which treats `\r`, `\n`, and `\r\n` as line separators.

For example, to split only by `\n`, use `--split-by '\n'`.

### http request from command-line arguments [conflicts with -\-request]

```
-u --url <values>
```

This option specifies the target URL. Multiple URLs can be provided using `-u https://example.com https://4rt.one`, or by using a filename: `-u targets.txt`.

To specify an injection point, use `%s`. For example, `-u https://4rt.one?a=b` is equivalent to `-u https://4rt.one/?a=b&%s`.

Supported variables include {{random}}. For instance, `-u https://4rt.one/?something={{random}}` will cause the something parameter to take on new values for every request.

```
-X --method <values>
```

This option specifies the request method.

An example with multiple values: `-X GET POST`

```
-b --body <value>
```

This option specifies the request body.

To specify an injection point, use `%s`. For example, `-b '{"some":"value"}'` is equivalent to `-b '{"some":"value", %s}'`.

Supported variables include `{{random}}`.

```
-H <values>
```

This option specifies the request headers.

For example, `-H "User-Agent: Mozilla" "X-Something: awesome"`.

You can overwrite the default Host header as well.

**NOTE**: Overwriting the `Host` header works properly only with `HTTP/1.1` because there is no `Host` header for `HTTP/2`. Instead, for `HTTP/2`, there is a special `:authority` header, but the tool currently cannot change special `HTTP/2` headers.

**NOTE**: You may encounter some case-related problems. The library that I am using for requests is `reqwest`. It capitalizes the first letter of the header name (or one after `-`) and lowers the rest for `HTTP/1.1`. However, for `HTTP/2` requests, `reqwest` lowers every header name (as per `HTTP/2` specs).

```
--http <1.1/2>
```

This option forces the use of a specific HTTP version. You can specify either `1.1` or `2`.

For example, `--http 1.1` will force the use of `HTTP/1.1`, while `--http 2` will force the use of `HTTP/2`.

### Parameters

The tool's primary purpose is to handle a wide range of situations. To accomplish this, several options have been added that provide precise control over how and where parameters are inserted.

To insert parameters into specific locations, use the `%s` variable.

```
-P --param-template <value>
```

Here, `%k` represents the key, and `%v` represents the value.

For standard GET requests, the parameter template is typically `%k=%v`.

Default values are `%k=%v` for URL-encoded data, `"%k":%v` for JSON, and `%k=%v` for header values.

Examples:

- To search for specific object fields: `-P user[%k]=%v`
- To search for json array values: `-P "%k"`, with `--body '{"arr":[%s]}' --joiner ', '`


```
-j --joiner <value>
```

This argument determines how to join parameters together. For ordinary GET requests, it's `&`.

Default values: for urlencoded `&`, for JSON `,`, for header values `; `

- Custom made XML discovery format: `--body "<root>%s</root>" --joiner "\n" --param-template "<%k>%v</%k>"`


```
-t --data-type <json/urlencoded>
```

Sometimes you need to tell the tool the data type.

For example, when the body isn't provided with the `POST` method. By default, **urlencoded** format will be used. You can change this behavior with `-t json`

```
--encode
```
In some contexts, you may need to encode special characters. `&` becomes `%26`

List of characters to encode: **["`<>&#;/=%]**

For example, when you find an app that forwards a specific parameter to the backend: `-u 'https://4rt.one/v?uid=00000%26%s' --encode`

`https://4rt.one/v?uid=<value>%26param%3dvalue` -> makes request to -> `http://internal/secret?uid=<value>&param=value`

```
--custom-parameters <values> --custom-values <values>
```

Some parameters can often have non-random values like `debug=1`. The tool automatically checks for these cases, but you can overwrite the default values.

Default values:

`--custom-parameters admin bot captcha debug disable encryption env show sso test waf`

`--custom-values 1 0 false off null true yes no`

*Usually, adding an additional custom parameter is free, while adding a custom value costs 1 request per value.*

```
--disable-custom-parameters
```

Disables checking for custom parameters by default.

```
-m --max <uint>
```

Determines how many parameters to send in every request.

By default: for query parameters, it starts with 128 and tries to increase up to 256. With v4.2.0, the logic was improved and the value may even be less than 128. For headers and header values, the default is 64. For the body, the default is 512.

### Behavior

```
--headers
```

Search for headers. By default, the tool sends 64 headers per requests, but this can be configured with the `-m` option.

**Note**: You may encounter all the limitations described in `-H` from [HTTP Request From Command-Line Arguments](#http-request-from-command-line-arguments) section.

```
--invert
```

Sometimes you may need to send parameters via the body with the `GET` method or via query with the `POST` method. By default, parameters are sent within the request body only with the `PUT` and `POST` methods, but it can be overwritten with the `--invert` option.

```
--recursion-depth <uint> [default: 1]
```

Checks the same list of parameters over and over, adding found parameters every run.

*Only parameters that don't change the page's code are added to the next run.*

```
--reflected-only
```

Search only for reflected parameters to reduce the amount of sent requests.

```
--strict
```

Do not report parameters that change the same part of the page. This helps to get rid of mass false positives, such as when all the parameters containing `admin` cause page differences. Note that this can lead to a few false negatives as well. In the future, this option will be replaced with a bit better logic.

### Concurrency

Implemented using async/awaits.

```
-W --workers <uint> [default: 1]
```

This specifies the number of concurrent URL checks.

`-W 0` -- checks all URLs in parallel.

```
--one-worker-per-host
```

This option only checks URLs with different hosts in parallel.

**Note**: This option does not increase the number of workers if there are fewer workers than hosts. You can use `-W 0` for one **worker** per **host**.

```
-c --concurrency <uint> [default: 1]
```

This specifies the number of concurrent jobs for each worker.

### Output

```
-v --verbose <0/1/2> [default: 1]
```

This option determines how much information to print to the console.

The output also depends on the number of parallel URL checks.

- 0 --- prints only the initial configuration, URL configuration, and their found parameters. The progress bar remains but can be disabled with `--disable-progress-bar`.
- 1 --- 0 + prints every discovered parameter's kind if only one URL is being checked in parallel.
- 2 --- 0 + prints every discovered parameter's kind always.

```
-o --output <filename>
```

This option specifies the file where the final output is written.

By default, the file overwrites unless `--append` is provided.

The file is dynamically populated unless the JSON output is used.

```
-O --output-format <standart/json/url/request>
```

This option specifies the output format for the final message about found parameters.

If `--output` is defined, the same message is printed to the file.

**standart**: `<METHOD> <URL> % <PARAMETERS devided by ', '>`

**json**:
```json
[
  {
    "method": "<method>",
    "url": "<url>",
    "status": <status code>,
    "size": <initial page size>,
    "found_params": [
      {
        "name": "<parameter name>",
        "value": "<null or parameter value>",
        "diffs": "<empty or diffs>",
        "status": <status code with this parameter>,
        "size": <page size with this parameter>,
        "reason_kind": "<explained below>"
      }
    ],
    "injection_place": "<where the injection point is -- Path, Body, Headers, HeaderValue>"
  }
]
```

reason_kind can take on 4 values:

- Code --- the parameter changes the page's code.
- Text --- the parameter changes the page's body or headers.
- Reflected --- the parameter reflects on the page different amount of times (compared to non-existing parameters).
- NotReflected --- the parameter causes other parameters to reflect different amount of times.

**url**: `<url>?<parameters devided by '&' with random or specific values>`

**request**: The http request with parameters. Parameter values can be either random or specific like 'true'.

```
--remove-empty
```

This option excludes entries without found parameters from the output file.