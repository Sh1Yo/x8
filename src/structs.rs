use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
    convert::TryFrom, error::Error, iter::FromIterator
};
use itertools::Itertools;
use regex::Regex;
use reqwest::{Client, Url};
use lazy_static::lazy_static;


use crate::{
    utils::random_line, diff::diff,
};

#[derive(Debug, Clone)]
pub struct RequestDefaults<'a> {
    pub method: String,
    pub scheme: String,
    pub path: String,
    pub host: String,
    pub port: u16,
    pub custom_headers: HashMap<String, String>,
    pub delay: Duration,
    pub initial_response: Option<Response<'a>>,
    pub client: Client,
    pub template: String,
    pub joiner: String,
    pub is_json: bool, //to replace {"key": "false"} with {"key": false}
    pub body: String,
    pub injection_place: InjectionPlace,
    pub amount_of_reflections: usize
}

impl<'a> Default for RequestDefaults<'a> {
    fn default() -> RequestDefaults<'a> {
        RequestDefaults {
            method: "GET".to_string(),
            scheme: "https".to_string(),
            path: "/".to_string(),
            host: "example.com".to_string(),
            custom_headers: HashMap::new(),
            port: 443,
            delay: Duration::from_millis(0),
            initial_response: None,
            client: Default::default(),
            template: "{k}={v}".to_string(),
            joiner: "&".to_string(),
            is_json: false,
            body: String::new(),
            injection_place: InjectionPlace::Path,
            amount_of_reflections: 0
        }
    }
}

impl<'a> RequestDefaults<'a> {
    pub fn new(
        method: &str,
        url: &str,
        custom_headers: HashMap<&str, String>,
        delay: Duration,
        client: Client,
        template: Option<&str>,
        joiner: Option<&str>,
        data_type: Option<DataType>,
        injection_place: InjectionPlace,
        body: &str
    ) -> Result<Self, Box<dyn Error>> {

        let (guessed_template, guessed_joiner, is_json, data_type) =
            RequestDefaults::guess_data_format(body, &injection_place, data_type);

        let (template, joiner) = (template.unwrap_or(guessed_template), joiner.unwrap_or(guessed_joiner));

        let url = Url::parse(url)?;

        let (path, body) = if data_type.is_some() {
            RequestDefaults::fix_path_and_body(url.path(), body, joiner, &injection_place, data_type.unwrap())
        } else { //injection within headers
            (url.path().to_string(), body.to_owned())
        };

        let custom_headers: HashMap<String, String> = custom_headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

        Ok(Self{
            method: method.to_string(),
            scheme: url.scheme().to_string(),
            path,
            host: url.host().ok_or("Host missing")?.to_string(),
            custom_headers,
            port: url.port_or_known_default().ok_or("Wrong scheme")?,
            delay,
            client,
            template: template.to_string(),
            joiner: joiner.to_string(),
            is_json,
            body,
            injection_place,
            //to fill after the first request
            initial_response: None,
            amount_of_reflections: 0
        })
    }

    /// returns template, joiner, whether the data is json, DataType if the injection point isn't within headers
    fn guess_data_format(
        body: &str, injection_place: &InjectionPlace, data_type: Option<DataType>
    ) -> (&'a str, &'a str, bool, Option<DataType>) {
        if data_type.is_some() {
            match data_type.unwrap() {
                //{v} isn't within quotes because not every json value needs to be in quotes
                DataType::Json => ("\"{k}\": {v}", ", ", true, Some(DataType::Json)),
                DataType::Urlencoded => ("{k}={v}", "&", false, Some(DataType::Urlencoded))
            }
        } else {
            match injection_place {
                InjectionPlace::Body => if body.starts_with("{") {
                    ("\"{k}\": {v}", ", ", true, Some(DataType::Json))
                } else {
                    ("{k}={v}", "&", false, Some(DataType::Urlencoded))
                },
                InjectionPlace::HeaderValue => ("{k}={v}", ";", false, None),
                InjectionPlace::Path => ("{k}={v}", "&", false, Some(DataType::Urlencoded)),
                InjectionPlace::Headers => ("", "", false, None)
            }
        }
    }

    /// adds injection points where necessary
    fn fix_path_and_body(
        path: &str, body: &str, joiner: &str, injection_place: &InjectionPlace, data_type: DataType
    ) -> (String, String) {

        match injection_place {
            InjectionPlace::Body => {
                if body.contains("%s") {
                    (path.to_string(), body.to_string())
                } else if body.is_empty() {
                    match data_type {
                        DataType::Urlencoded => (path.to_string(), format!("%s")),
                        DataType::Json => (path.to_string(), format!("{{%s}}"))
                    }
                } else {
                    match data_type {
                        DataType::Urlencoded => (path.to_string(), format!("{}{}%s", body, joiner)),
                        DataType::Json => {
                            let mut body = body.to_owned();
                            body.pop(); //remove the last '}'
                            (path.to_string(), format!("{}, %s}}", body))
                        }
                    }
                }
            },
            InjectionPlace::Path => {
                if path.contains("%s") {
                    (path.to_string(), body.to_string())
                } else if path.contains("?") {
                    (format!("{}{}%s", joiner, path), body.to_string())
                } else if joiner == "&" {
                    (format!("{}?%s", path), body.to_string())
                } else { //some very non-standart configuration
                    (format!("{}%s", path), body.to_string())
                }
            }
            _ => (path.to_string(), body.to_string())
        }
    }

    /// recreates url
    pub fn url(&self) -> String {
        format!("{}{}:{}{}", self.scheme, self.host, self.port, self.path)
    }

    /// for testing purposes only
    pub fn recreate(&self, data_type: Option<DataType>, template: Option<&str>, joiner: Option<&str>) -> Self {

        let custom_headers: HashMap<&str, String> = HashMap::from_iter(self.custom_headers.iter().map(|(k, v)| (k.as_str(), v.to_owned())));

        RequestDefaults::new(
            &self.method,
            &format!("{}://{}:{}{}", &self.scheme, &self.host, self.port, &self.path),
            custom_headers,
            self.delay,
            self.client.clone(),
            template,
            joiner,
            data_type,
            self.injection_place.clone(),
            &self.body
        ).unwrap()
    }
}

pub enum DataType {
    Json,
    Urlencoded
}

#[derive(Debug, Clone, PartialEq)]
pub enum InjectionPlace {
    Path,
    Body,
    Headers,
    HeaderValue
}

//TODO add references where possible because the request is often cloned
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub defaults: &'a RequestDefaults<'a>,
    pub path: String,
    pub method: String,

    headers: HashMap<String, String>,
    parameters: Box<Vec<String>>, //vector of supplied parameters
    prepared_parameters: HashMap<String, String>, //parsed parameters
    non_random_parameters: HashMap<String, String>, //parameters with not random values (in order to remove false positive reflections)
    body: String,
    delay: Duration,
    prepared: bool
}

impl <'a>Request<'a> {

    pub fn new(l: &'a RequestDefaults, parameters: Vec<String>) -> Self {

        let mut headers = HashMap::from([
            ("User-Agent".to_string(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/99.0.4844.82 Safari/537.36".to_string()),
            ("Host".to_string(), l.host.to_owned())
        ]);

        for (k, v) in l.custom_headers.to_owned() {
            headers.insert(k, v);
        }

        Self{
            defaults: l,
            method: "GET".to_string(),
            path: l.path.to_owned(),
            headers,
            body: String::new(),
            parameters: Box::new(parameters),
            prepared_parameters: HashMap::new(),
            non_random_parameters: HashMap::new(),
            delay: l.delay,
            prepared: false
        }
    }

    pub fn new_random(l: &'a RequestDefaults, max: usize) -> Self {
        let parameters = Vec::from_iter((0..max).map(|_| random_line(5)));
        Request::new(l, parameters)
    }

    pub fn set_header<S: Into<String>>(&mut self, key: S, value: S) {
        self.headers.insert(key.into(), value.into());
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        for (k, v) in headers {
            self.headers.insert(k, v);
        }
    }

    pub fn url(&self) -> String {
        format!("{}{}:{}{}", &self.defaults.scheme, &self.defaults.host, &self.defaults.port, &self.path)
    }

    pub fn make_query(&self) -> String {
        self.prepared_parameters
            .iter()
            .map(|(k, v)| self.defaults.template
                                    .replace("{k}", k)
                                    .replace("{v}", v)
            )
            .collect::<Vec<String>>()
            .join(&self.defaults.joiner)
    }

    /// replace injection points with parameters
    /// replace templates ({{random}}) with random values
    /// additional param is for reflection counting
    ///
    /// in case self.parameters contains parameter with "%=%"
    /// it gets splitted by %=%  and the default random value gets replaced with the right part:
    /// admin%=%true -> (admin, true) vs admin -> (admin, df32w)
    fn prepare(&mut self, additional_param: Option<&String>) {
        if self.prepared {
            return
        }
        self.prepared = true;

        self.non_random_parameters = HashMap::from_iter(
            self.parameters
                .iter()
                .filter(|x| x.contains("%=%"))
                .map(|x| x.split("%=%"))
                .map(|mut x| (x.next().unwrap().to_owned(), x.next().unwrap_or("").to_owned()))
        );

        self.prepared_parameters = HashMap::from_iter(
            self.parameters
                .iter()
                .chain([additional_param.unwrap_or(&String::new())])
                .filter(|x| !x.is_empty() && !x.contains("%=%"))
                .map(|x| (x.to_owned(), random_line(5)))
                //append parameters with not random values
                .chain(
                    self.non_random_parameters
                        .iter()
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                )
        );

        if self.defaults.injection_place != InjectionPlace::HeaderValue {
            for (k, v) in self.defaults.custom_headers.iter() {
                self.set_header(
                    k,
                    &v.replace("{{random}}", &random_line(5))
                );
            }
        }
        self.path = self.defaults.path.replace("{{random}}", &random_line(5));
        self.body = self.defaults.body.replace("{{random}}", &random_line(5));

       match self.defaults.injection_place {
            InjectionPlace::Path => self.path = self.path.replace("%s", &self.make_query()),
            InjectionPlace::Body => {
                self.body = self.body.replace("%s", &self.make_query());

                if !self.defaults.custom_headers.contains_key("Content-Type") {
                    if self.defaults.is_json {
                        self.set_header("Content-Type", "application/json");
                    } else {
                        self.set_header("Content-Type", "application/x-www-form-urlencoded");
                    }
                }
            },
            InjectionPlace::HeaderValue => {
                for (k, v) in self.defaults.custom_headers.iter() {
                    self.set_header(
                        k,
                        &v.replace("{{random}}", &random_line(5)).replace("%s", &self.make_query())
                    );
                }
            },
            InjectionPlace::Headers => {
                let headers: HashMap<String, String>
                    = self.parameters.iter().map(|x| (x.to_string(), random_line(5).to_string())).collect();

                self.set_headers(headers);
            }
       }
    }

    pub async fn send_by(self, clients: &Client) -> Result<Response<'a>, Box<dyn Error>> {

        match self.clone().request(clients).await {
            Ok(val) => Ok(val),
            Err(err) => {
                log::info!("send_by:request error - {}\nRepeat in 30 secs", err);
                std::thread::sleep(Duration::from_secs(30));
                Ok(self.clone().request(clients).await?)
            }
        }
    }

    pub async fn send(self) -> Result<Response<'a>, Box<dyn Error>> {
        let dc = &self.defaults.client;
        self.send_by(dc).await
    }

    async fn request(mut self, client: &Client) -> Result<Response<'a>, reqwest::Error> {

        let additional_parameter = random_line(7);

        self.prepare(Some(&additional_parameter));

        let mut request = http::Request::builder()
            .method(self.method.as_str())
            .uri(self.url());

        for (k, v) in &self.headers {
            request = request.header(k,v)
        };

        let request = request
            .body(self.body.to_owned())
            .unwrap();

        std::thread::sleep(self.delay);

        let reqwest_req = reqwest::Request::try_from(request).unwrap();

        let start = Instant::now();

        let res = client.execute(reqwest_req).await?;

        let duration = start.elapsed();

        let mut headers: BTreeMap<String, String> = BTreeMap::new();

        for (k, v) in res.headers() {
            let k = k.to_string();
            let v = v.to_str().unwrap().to_string();

            if headers.contains_key(&k) {
                //mostly for Set-Cookie headers
                headers.insert(k.to_owned(), [headers[&k].to_owned(), v].join("\n"));
            } else {
                headers.insert(k, v);
            }
        }

        let code = res.status().as_u16();

        let body_bytes = res.bytes().await?.to_vec();

        let body = String::from_utf8_lossy(&body_bytes).to_string();

        //TODO beautify body
        let mut response = Response{
            code,
            headers,
            time: duration.as_millis(),
            body,
            request: self,
            reflected_parameters: HashMap::new(),
            additional_parameter: additional_parameter
        };

        response.beautify_body();
        response.fill_reflected_parameters();

        Ok(response)
    }

    /// the function is used when there was a error during the request
    pub fn empty_response(mut self) -> Response<'a> {
        self.prepare(None);
        Response {
            time: 0,
            code: 0,
            headers: BTreeMap::new(),
            body: String::new(),
            reflected_parameters: HashMap::new(),
            additional_parameter: String::new(),
            request: self,
        }
    }

    pub fn print(&mut self) -> String {
        self.prepare(Some(&random_line(5)));

        let mut str_req = format!("{} {} HTTP/1.1\n", &self.method, self.path); //TODO identify HTTP version

        for (k, v) in self.headers.iter().sorted() {
            str_req += &format!("{}: {}\n", k, v)
        }

        str_req += &format!("\n{}", self.body);

        str_req
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use crate::structs::{RequestDefaults, Request, InjectionPlace, DataType};

    #[test]
    fn query_creation() {
        let mut l = RequestDefaults::default();
        l.template = "{k}=payload".to_string();
        l.joiner = "&".to_string();
        let parameters = vec!["test1".to_string(), "test2".to_string()];
        let request = Request::new(&l, parameters);

        assert_eq!(request.make_query(), "test1=payload&test2=payload");
    }

    #[test]
    fn request_defaults_generation() {
        let defaults = RequestDefaults::new(
            "GET",
            "https://example.com:8443/path",
            HashMap::from([("X-Header", "Value".to_string())]),
            Duration::from_millis(0),
            Default::default(),
            None,
            None,
            None,
            super::InjectionPlace::Path,
            ""
        ).unwrap();

        assert_eq!(defaults.scheme, "https");
        assert_eq!(defaults.host, "example.com");
        assert_eq!(defaults.port, 8443);
        assert_eq!(defaults.path, "/path?%s");
        assert_eq!(defaults.custom_headers["X-Header"], "Value");
        assert_eq!(defaults.template, "{k}={v}");
        assert_eq!(defaults.joiner, "&");
        assert_eq!(defaults.injection_place, InjectionPlace::Path);
    }

    #[test]
    fn request_body_generation() {
        let mut template = RequestDefaults::default();

        template.injection_place = InjectionPlace::Body;
        let defaults = template.recreate(Some(DataType::Json), None, None);
        assert!(defaults.is_json);
        assert_eq!(defaults.body, "{%s}");
        assert_eq!(defaults.template, "\"{k}\": {v}");

        template.body = "{\"something\":1}".to_string();
        let defaults = template.recreate(None, None, None);
        assert_eq!(defaults.body, "{\"something\":1, %s}");
        assert_eq!(defaults.template, "\"{k}\": {v}");

        template.body = String::new();
        let defaults = template.recreate(None, None, None);
        assert_eq!(defaults.body, "%s");

        template.body = "a=b".to_string();
        let defaults = template.recreate(None, None, None);
        assert_eq!(defaults.body, "a=b&%s");
    }

    #[test]
    fn request_generation() {
        let mut template = RequestDefaults::default();

        let defaults = template.recreate(None, None, None);
        assert_eq!(defaults.path, "/?%s");
        let params = vec!["param".to_string()];
        let mut request = Request::new(&defaults, params);
        request.prepare(None);
        assert!(request.path.starts_with("/?param="));
        assert!(request.url().starts_with("https://example.com:443/?param="));

        template.injection_place = InjectionPlace::Body;
        template.body = "{\"something\":[%s]}".to_string();
        let defaults = template.recreate(None, Some("\"{k}\""), Some(", "));
        let params = vec!["param1".to_string(), "param2".to_string()];
        let mut request = Request::new(&defaults, params.clone());
        request.prepare(None);
        assert_eq!(request.body, "{\"something\":[\"param1\", \"param2\"]}");

        template.body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><note>%s</note>".to_string();
        let defaults = template.recreate(None, Some("<{k}>sth</{k}>"), Some(""));
        let mut request = Request::new(&defaults, params);
        request.prepare(None);
        assert_eq!(request.body, "<?xml version=\"1.0\" encoding=\"UTF-8\"?><note><param1>sth</param1><param2>sth</param2></note>");
    }
}

#[derive(Debug, Clone)]
pub struct Response<'a> {
    pub time: u128,
    pub code: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub reflected_parameters: HashMap<String, usize>, //<parameter, amount of reflections>
    pub additional_parameter: String,
    pub request: Request<'a>,
}

impl<'a> Response<'a> {

    /// count how many times we can see the string in the response
    pub fn count(&self, string: &str) -> usize {
        //TODO maybe optimize a bit and merge headers with body before counting
        let mut count: usize = 0;
        for (key, value) in self.headers.iter() {
            count += key.matches(string).count() + value.matches(string).count();
        }
        count += self.body.matches(string).count();
        count
    }

    /// calls check_diffs & returns code and found diffs
    pub fn compare(&self, old_diffs: &Vec<String>) -> Result<(bool, Vec<String>), Box<dyn Error>> {

        let mut is_code_diff: bool = false;
        let mut diffs: Vec<String> = Vec::new();

        if self.request.defaults.initial_response.as_ref().unwrap().code != self.code {
            is_code_diff = true
        }

        //just push every found diff to the vector of diffs
        for diff in diff(
            &self.print(),
            &self.request.defaults.initial_response.as_ref().unwrap().print(),
        )? {
            if !diffs.contains(&diff) && old_diffs.contains(&diff) {
                diffs.push(diff);
            } else {
                let mut c = 1;
                while diffs.contains(&[&diff, "(", &c.to_string(), ")"].concat()) {
                    c += 1
                }
                diffs.push([&diff, " (", &c.to_string(), ")"].concat());
            }
        }

        Ok((is_code_diff, diffs))
    }

    /// adds new lines where necessary in order to increase accuracy in diffing
    fn beautify_body(&mut self) {
        lazy_static! {
            static ref RE_JSON_WORDS_WITHOUT_QUOTES: Regex =
                Regex::new(r#"^(\d+|null|false|true)$"#).unwrap();
            static ref RE_JSON_BRACKETS: Regex =
                Regex::new(r#"(?P<bracket>(\{"|"\}|\[("|\d)|("|\d)\]))"#).unwrap();
            static ref RE_JSON_COMMA_AFTER_DIGIT: Regex =
                Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>\d+),"#).unwrap();
            static ref RE_JSON_COMMA_AFTER_BOOL: Regex =
                Regex::new(r#"(?P<first>"[\w\.-]*"):(?P<second>(false|null|true)),"#).unwrap();
        }

        self.body
            = if (self.headers.contains_key("content-type") && self.headers["content-type"].contains("json"))
            || (self.body.starts_with("{") && self.body.ends_with("}")) {
            let body = self.body
                                    .replace("\\\"", "'")
                                    .replace("\",", "\",\n");
            let body = RE_JSON_BRACKETS.replace_all(&body, "${bracket}\n");
            let body = RE_JSON_COMMA_AFTER_DIGIT.replace_all(&body, "$first:$second,\n");
            let body = RE_JSON_COMMA_AFTER_BOOL.replace_all(&body, "$first:$second,\n");

            body.to_string()
        } else {
            self.body.replace('>', ">\n")
        }
    }

    /// find parameters with the different amount of reflections and add them to self.reflected_parameters
    pub fn fill_reflected_parameters(&mut self) {
        //let base_count = self.count(&self.request.prepared_parameters[additional_param]);

        //remove non random parameters from prepared parameters because they would cause false positives in this check
        let prepated_parameters: HashMap<&String, &String> = if !self.request.non_random_parameters.is_empty() {
            HashMap::from_iter(
                self.request.prepared_parameters
                    .iter()
                    .filter(|x| !self.request.non_random_parameters.contains_key(x.0))
            )
        } else {
            HashMap::from_iter(
                self.request.prepared_parameters.iter()
            )
        };

        for (k, v) in prepated_parameters.iter() {
            let new_count = self.count(v);
            if self.request.defaults.amount_of_reflections != new_count {
                self.reflected_parameters.insert(k.to_string(), new_count);
            }
        }
    }

    /// returns parameters with different amount of reflections and tells whether we need to recheck the remaining parameters
    pub fn proceed_reflected_parameters(&self) -> (Option<&str>, bool) {

        // only one reflected parameter - return it
        if self.reflected_parameters.len() == 1 {
            return (Some(self.reflected_parameters.keys().next().unwrap()), false)
        }

        // only one reflected parameter besides additional one - return it
        // this parameter caused other parameters to reflect different amount of times
        if self.request.prepared_parameters.len() == 2 && self.reflected_parameters.len() == 2 {
            return (Some(self.reflected_parameters.keys().filter(|x| x != &&self.additional_parameter).next().unwrap()), false)
        }

        //save parameters by their amount of reflections
        let mut parameters_by_reflections: HashMap<usize, Vec<&str>> = HashMap::new();

        for (k, v) in self.reflected_parameters.iter() {
            if parameters_by_reflections.contains_key(v) {
                parameters_by_reflections.get_mut(v).unwrap().push(k);
            } else {
                parameters_by_reflections.insert(*v, vec![k]);
            }
        }

        //try to find a parameter with different amount of reflections between all of them
        if parameters_by_reflections.len() == 2 {
            for (_, v) in parameters_by_reflections.iter() {
                if v.len() == 1 {
                    return (Some(v[0]), true)
                }
            }
        }

        // the reflections weren't stable. It's better to recheck the parameters
        (None, true)
    }

    /// get possible parameters from the page itself
    pub fn get_possible_parameters(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let body = &self.body;

        let re_special_chars = Regex::new(r#"[\W]"#).unwrap();

        let re_name = Regex::new(r#"(?i)name=("|')?"#).unwrap();
        let re_inputs = Regex::new(r#"(?i)name=("|')?[\w-]+"#).unwrap();
        for cap in re_inputs.captures_iter(body) {
            found.push(re_name.replace_all(&cap[0], "").to_string());
        }

        let re_var = Regex::new(r#"(?i)(var|let|const)\s+?"#).unwrap();
        let re_full_vars = Regex::new(r#"(?i)(var|let|const)\s+?[\w-]+"#).unwrap();
        for cap in re_full_vars.captures_iter(body) {
            found.push(re_var.replace_all(&cap[0], "").to_string());
        }

        let re_words_in_quotes = Regex::new(r#"("|')[a-zA-Z0-9]{3,20}('|")"#).unwrap();
        for cap in re_words_in_quotes.captures_iter(body) {
            found.push(re_special_chars.replace_all(&cap[0], "").to_string());
        }

        let re_words_within_objects = Regex::new(r#"[\{,]\s*[[:alpha:]]\w{2,25}:"#).unwrap();
        for cap in re_words_within_objects.captures_iter(body){
            found.push(re_special_chars.replace_all(&cap[0], "").to_string());
        }

        found.sort();
        found.dedup();
        found
    }

    ///print the whole response
    pub fn print(&self) -> String {
        let mut text: String = String::new();
        text.push_str(&("Code: ".to_owned()+&(self.code.to_string()+"\n")));
        for (k,v) in self.headers.iter() {
            text.push_str(&k);
            text.push_str(": ");
            text.push_str(&v);
            text.push('\n');
        }
        text.push('\n');
        text.push_str(&self.body);
        text
    }
}


#[derive(Debug, Clone)]
pub struct FuturesData {
    pub remaining_params: Vec<String>,
    pub found_params: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    //default url without any changes (except from when used from request file, maybe change this logic TODO)
    pub url: String,

    //user supplied wordlist file
    pub wordlist: String,

    //proxy server with schema or http:// by default.
    pub proxy: String,

    //file to output
    pub output_file: String,
    //whether to append to the output file or overwrite
    pub append: bool,

    //output format for file & stdout outputs
    pub output_format: String,

    //a directory for saving request & responses with found parameters
    pub save_responses: String,

    //ignore errors like 'Page is too huge'
    pub force: bool,

    //custom parameters to check like <admin, [true, 1, false, ..]>
    pub custom_parameters: HashMap<String, Vec<String>>,
    pub disable_custom_parameters: bool,

    //disable progress bar for high verbosity
    pub disable_progress_bar: bool,

    //proxy to resend requests with found parameter
    pub replay_proxy: String,
    //whether to resend the request once with all parameters or once per every parameter
    pub replay_once: bool,

    //print request & response and exit.
    //Can be useful for checking whether the program parsed the input parameters successfully
    pub test: bool,

    //0 - print only critical errors and output
    //1 - print intermediate results and progress bar
    //2 - print all response changes
    pub verbose: usize,

    //determines how much learning requests should be made on the start
    //doesn't include first two requests made for cookies and initial response
    pub learn_requests_count: usize,

    //amount of concurrent requests
    pub concurrency: usize,

    //whether the verify found parameters one time more.
    //in future - check for _false_potives like when every parameter that starts with _ is found
    pub verify: bool,

    //check only for reflected parameters in order to decrease the amount of requests
    //usually makes 2+learn_request_count+words/max requests
    //but in rare cases its number may be higher
    pub reflected_only: bool,

    pub follow_redirects: bool,
}

#[derive(Debug)]
pub struct Stable {
    pub body: bool,
    pub reflections: bool,
}