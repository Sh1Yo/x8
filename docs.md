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

The file with the raw http request.

When the request file is used -- no default headers (`Accept`, `User-Agent`, etc) are added to the request.

For now, an url gets created directly from the `Host` header; therefore, it's not possible to set an arbitrary `Host` header from within a request file. If you want to set a different `Host` header, see `-H` from [this](#http-request-from-command-line-arguments) category.

```
--proto <http/https>
```

An additional and required to the `--request` argument.

Either http or https can be specified.

```
--split-by <value>
```

How to split the request file. By default, the `.lines()` method is used that treats `\r`, `\n`, and `\r\n` as line separators.

Example to split only by `\n`: `--split-by '\n'`

### http request from command-line arguments [conflicts with -\-request]

```
-u --url <values>
```

The target url. Multiple urls can be provided via `-u https://example.com https://4rt.one` or via a filename: `-u targets.txt`.

To specify an injection point use **%s**. `-u https://4rt.one?a=b` equals to `-u https://4rt.one/?a=b&%s`

Supported variables -- **{{random}}**. `-u https://4rt.one/?something={{random}}` the **something** parameter will take on new values every request.

```
-X --method <values>
```

The request method.

An example with multiple values: `-X GET POST`

```
-b --body <value>
```

The request body.

To specify an injection point use **%s**. `-b '{"some":"value"}'` equals to `-b '{"some":"value", %s}'`

Supported variables -- **{{random}}**.

```
-H <values>
```

The request headers.

Example: `-H "User-Agent: Mozilla" "X-Something: awesome"`

You can overwrite the default `Host` header as well.

**NOTE** The overwriting of the `Host` header works properly only with `HTTP/1.1` because there's no `Host` header for `HTTP/2`. Instead, for `HTTP/2` there's a special `:authority` header, but the tool currently can't change special `HTTP/2` headers.

**NOTE** You can encounter some case-related problems. The library that I'm using for requests is reqwest. It uppercases the first letter of the header name(or one after `-`) and lowercases other ones for `HTTP/1.1`. On the other hand, for `HTTP/2` requests, reqwest lowercases every header name (as per `HTTP/2` specs)

```
--http <1.1/2>
```

To force the specific http version.

### Parameters

The main purpose of the tool is to handle all the different situations. To achieve it, I added a few options that allow accurate control of how and where the parameters are inserted.

To insert parameters into specific place use **%s** variable.

```
-P --param-template <value>
```

**%k** --- key, **%v** -- value.

For ordinary get requests the parameter template is usually `%k=%v`.

Default values: for urlencoded `%k=%v`, for json `"%k":%v`, and for header values `%k=%v`

Examples:

- To search for specific object fields: `-P user[%k]=%v`
- To search for json array values: `-P "%k"`, with `--body '{"arr":[%s]}' --joiner ', '`


```
-j --joiner <value>
```

This argument determines how to join parameters together. For ordinary get requests it's `&`.

Default values: for urlencoded `&`, for json `, `, for header values `; `

- Custom made xml discovery format: `--body "<root>%s</root>" --joiner "\n" --param-template "<%k>%v</%k>"`

```
-t --data-type <json/urlencoded>
```

Sometimes you need to tell the tool the data type.

For example, when the body isn't provided with the `POST` method. By default, **urlencoded** format will be used. You can change this behavior with `-t json`

```
--encode
```
In some contexts, you may need to encode special chars. `&` -> `%26`

List of chars to encode: **["` <>&#;/=%]**

- For example, when you find an app that forwards a specific parameter to the backend: `-u 'https://4rt.one/v?uid=00000%26%s' --encode`

`https://4rt.one/v?uid=<value>%26param%3dvalue` -> makes request to -> `http://internal/secret?uid=<value>&param=value`

```
--custom-parameters <values> --custom-values <values>
```

Some parameters can often have non-random values like `debug=1`. The tool automatically checks for these cases, but you can overwrite the default values.

Default values:

`--custom-parameters admin bot captcha debug disable encryption env show sso test waf`

`--custom-values 1 0 false off null true yes no`

*Usually, adding an additional custom parameter is free, while adding a custom value costs 1 req per value.*

```
--disable-custom-parameters
```

Disables checking for custom parameters by default.

```
-m --max <uint>
```

How many parameters to send in every request.

By default: for query <=256 (starts with 128 and tries to increase up to 256. With v4.2.0 the logic was improved and the value may even be less than 128), 64 for headers and header values, and 512 for body.

### Behavior

```
--headers
```

Search for headers.

**Note** You may encounter with all the limitations described in `-H` from [this](#http-request-from-command-line-arguments) section.

By default sends 64 headers per requests. Can be configured with `-m`.

```
--invert
```

Sometimes you may need to send parameters via body with the `GET` method or via query with the `POST` method.

By default, parameters are sent within the request body only with the `PUT` and `POST` methods, but it can be overwriten with the `--invert` option.

```
--recursion-depth <uint> [default: 1]
```

Checks the same list of parameters over and over, adding found parameters every run.

*Only parameters that don't change the page's code are added to the next run*

```
--reflected-only
```

To search only for reflected parameters.

Reduces the amount of sent requests.

```
--strict
```

Do not report parameters that change the same part of the page. Helps to get rid of mass false positives like when all the parameters containing `admin` cause page differences.

Can lead to a few false negatives as well. In the future, will be replaced with a bit better logic.

### Concurrency

Implemented using async/awaits.

```
-W --workers <uint> [default: 1]
```

The number of concurrect url check.

`-W 0` -- check all the urls in parallel.

```
--one-worker-per-host
```

Only urls with different hosts will be checked in parallel.

**NOTE** can be a bit misleading, but this option doesn't increase the number of workers in case there're fewer workers than hosts. You can use `-W 0` for the actual one **worker** per **host**.

```
-c --concurrency <uint> [default: 1]
```

The amount of concurrent jobs for every worker.

### Output

```
-v --verbose <0/1/2> [default: 1]
```

Determines how much information to print into the console.

This depends on the amount of parallel url checks as well.

- 0 --- print only the initial config, urls' config, and their found parameters. The progress bar stays but can be disabled with `--disable-progress-bar`.
- 1 --- 0 + prints every discovered parameter's kind if only 1 url is being checked in parallel.
- 2 --- 0 + prints every discovered parameter's kind always.

```
-o --output <filename>
```

A file with the final output.

By default, the file overwrites unless`--append` is provided.

The file is dynamically populated unless the json output is used.

```
-O --output-format <standart/json/url/request>
```

The final message about found parameters is printed to the console with this output format. In case `--output` is defined, the same message is printed to the file.

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

Do not write entries without found parameters to the output file.