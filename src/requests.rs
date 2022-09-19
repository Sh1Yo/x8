use crate::{
    structs::{Config, Stable, RequestDefaults, Request, FoundParameter, InjectionPlace, Response, Headers, DataType}, utils::{random_line, self, progress_bar},
};
use itertools::Itertools;
use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use reqwest::Client;
use url::Url;
use std::{
    error::Error, collections::HashMap, iter::FromIterator, time::{Duration, Instant}, convert::TryFrom,
};

const MAX_PAGE_SIZE: usize = 25 * 1024 * 1024; //25MB usually

lazy_static! {
    static ref FRAGMENT: AsciiSet = CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'&')
        .add(b'#')
        .add(b';')
        .add(b'/')
        .add(b'=')
        .add(b'%');
}

///makes first requests and checks page behavior
pub async fn empty_reqs<'a>(
    config: &Config,
    request_defaults: &'a RequestDefaults<'a>,
    count: usize,
    max: usize,
) -> Result<(Vec<String>, Stable), Box<dyn Error>> {
    let mut stable = Stable {
        body: true,
        reflections: true,
    };
    let mut diffs: Vec<String> = Vec::new();

    for i in 0..count {
        let response =
            Request::new_random(request_defaults, max)
                .send()
                .await?;

        progress_bar(config, i, count);

        //do not check pages >25MB because usually its just a binary file or sth
        if response.text.len() > MAX_PAGE_SIZE && !config.force {
            Err("The page is too huge")?;
        }

        if !response.reflected_parameters.is_empty() {
            stable.reflections = false;
        }

        let (is_code_diff, mut new_diffs) = response.compare(&diffs)?;

        if is_code_diff {
            Err("The page is not stable (code)")?
        }

        diffs.append(&mut new_diffs);
    }

    //check the last time
    let response =
        Request::new_random(request_defaults, max)
            .send()
            .await?;

    //in case the page is still different from other random ones - the body isn't stable
    if !response.compare(&diffs)?.1.is_empty() {
        utils::info(config, "The page is not stable (body)");
        stable.body = false;
    }

    Ok((diffs, stable))
}

pub async fn verify<'a>(
    request_defaults: &RequestDefaults<'a>, found_params: &Vec<FoundParameter>, diffs: &Vec<String>, stable: &Stable
) -> Result<Vec<FoundParameter>, Box<dyn Error>> {
    //TODO maybe implement sth like similar patters? At least for reflected parameters
    //struct Pattern {kind: PatterKind, pattern: String}
    //
    //let mut similar_patters: HashMap<Pattern, Vec<String>> = HashMap::new();
    //
    //it would allow to fold parameters like '_anything1', '_anything2' (all that starts with _)
    //to just one parameter in case they have the same diffs
    //sth like a light version of --strict

    let mut filtered_params = Vec::with_capacity(found_params.len());

    for param in found_params {

        let mut response = Request::new(&request_defaults, vec![param.name.clone()])
                                    .send()
                                    .await?;

        let (is_code_the_same, new_diffs) = response.compare(&diffs)?;
        let mut is_the_body_the_same = true;

        if !new_diffs.is_empty() {
            is_the_body_the_same = false;
        }

        response.fill_reflected_parameters();

        if !is_code_the_same || !(!stable.body || is_the_body_the_same) || !response.reflected_parameters.is_empty() {
            filtered_params.push(param.clone());
        }
    }

    Ok(filtered_params)
}

pub async fn replay<'a>(
    config: &Config, request_defaults: &RequestDefaults<'a>, replay_client: &Client, found_params: &Vec<FoundParameter>
) -> Result<(), Box<dyn Error>> {
     //get cookies
    Request::new(request_defaults, vec![])
        .send_by(replay_client)
        .await?;

    if config.replay_once {
        Request::new(request_defaults, found_params.iter().map(|x| x.name.to_owned()).collect::<Vec<String>>())
            .send_by(replay_client)
            .await?;
    } else {
        for param in found_params {
            Request::new(request_defaults, vec![param.name.to_string()])
                .send_by(replay_client)
                .await?;
        }
    }

    Ok(())
}

impl <'a>Request<'a> {

    pub fn new(l: &'a RequestDefaults, parameters: Vec<String>) -> Self {

        let mut headers = Vec::from([
            ("User-Agent".to_string(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/99.0.4844.82 Safari/537.36".to_string()),
            //We don't need Host header in http/2. In http/1 it should be added automatically
            //("Host".to_string(), l.host.to_owned())
        ]);

        for (k, v) in l.custom_headers.to_owned() {
            headers.push((k, v));
        }

        Self{
            defaults: l,
            method: l.method.to_owned(),
            path: l.path.to_owned(),
            headers,
            body: String::new(),
            parameters: parameters,
            prepared_parameters: l.parameters.clone(),
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
        self.headers.push((key.into(), value.into()));
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        for (k, v) in headers {
            self.headers.push((k, v));
        }
    }

    pub fn url(&self) -> String {
        format!("{}://{}:{}{}", &self.defaults.scheme, &self.defaults.host, &self.defaults.port, &self.path)
    }

    pub fn make_query(&self) -> String {
        let query = self.prepared_parameters
            .iter()
            .map(|(k, v)| self.defaults.template
                                    .replace("{k}", k)
                                    .replace("{v}", v)
            )
            .collect::<Vec<String>>()
            .join(&self.defaults.joiner);

        if self.defaults.encode {
            utf8_percent_encode(&query, &FRAGMENT).to_string()
        } else {
            query
        }
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
            //append self.prepared_parameters (can be set from RequestDefaults using recursive search)
            self.prepared_parameters
                .iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                //append parameters with not random values
                .chain(
                    self.non_random_parameters
                        .iter()
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                )
                //append random parameters
                .chain(
                    self.parameters
                        .iter()
                        .chain([additional_param.unwrap_or(&String::new())])
                        .filter(|x| !x.is_empty() && !x.contains("%=%"))
                        .map(|x| (x.to_owned(), random_line(5)))
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
            Err(_) => {
                std::thread::sleep(Duration::from_secs(10));
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

        let mut headers: Vec<(String, String)> = Vec::new();

        for (k, v) in res.headers() {
            let k = k.to_string();
            let v = v.to_str().unwrap().to_string();

            headers.push((k, v));
        }

        let code = res.status().as_u16();

        let body_bytes = res.bytes().await?.to_vec();

        let text = String::from_utf8_lossy(&body_bytes).to_string();

        let mut response = Response{
            code,
            headers,
            time: duration.as_millis(),
            text,
            request: self,
            reflected_parameters: HashMap::new(),
            additional_parameter: additional_parameter
        };

        response.beautify_body();
        response.add_headers();
        response.fill_reflected_parameters();

        Ok(response)
    }

    /// the function is used when there was a error during the request
    pub fn empty_response(mut self) -> Response<'a> {
        self.prepare(None);
        Response {
            time: 0,
            code: 0,
            headers: Vec::new(),
            text: String::new(),
            reflected_parameters: HashMap::new(),
            additional_parameter: String::new(),
            request: self,
        }
    }

    pub fn print(&mut self) -> String {
        self.prepare(Some(&random_line(5)));

        let mut str_req = format!("{} {} HTTP/x\nHost: {}\n", &self.method, self.path, self.defaults.host); //TODO identify HTTP version

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

    use crate::structs::{RequestDefaults, Request, InjectionPlace, DataType, Headers};

    #[test]
    fn query_creation() {
        let mut l = RequestDefaults::default();
        l.template = "{k}=payload".to_string();
        l.joiner = "&".to_string();
        let parameters = vec!["test1".to_string()];
        let mut request = Request::new(&l, parameters);
        request.prepare(None);

        assert_eq!(request.make_query(), "test1=payload");
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
            false,
            None,
            super::InjectionPlace::Path,
            ""
        ).unwrap();

        assert_eq!(defaults.scheme, "https");
        assert_eq!(defaults.host, "example.com");
        assert_eq!(defaults.port, 8443);
        assert_eq!(defaults.path, "/path?%s");
        assert_eq!(defaults.custom_headers.get_value("X-Header").unwrap(), "Value");
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
        let params = vec!["param1".to_string()];
        let mut request = Request::new(&defaults, params.clone());
        request.prepare(None);
        assert_eq!(request.body, "{\"something\":[\"param1\"]}");

        template.body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><note>%s</note>".to_string();
        let defaults = template.recreate(None, Some("<{k}>sth</{k}>"), Some(""));
        let mut request = Request::new(&defaults, params);
        request.prepare(None);
        assert_eq!(request.body, "<?xml version=\"1.0\" encoding=\"UTF-8\"?><note><param1>sth</param1></note>");
    }
}

impl<'a> Default for RequestDefaults<'a> {
    fn default() -> RequestDefaults<'a> {
        RequestDefaults {
            method: "GET".to_string(),
            scheme: "https".to_string(),
            path: "/".to_string(),
            host: "example.com".to_string(),
            custom_headers: Vec::new(),
            port: 443,
            delay: Duration::from_millis(0),
            initial_response: None,
            client: Default::default(),
            template: "{k}={v}".to_string(),
            joiner: "&".to_string(),
            is_json: false,
            encode: false,
            body: String::new(),
            parameters: HashMap::new(),
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
        encode: bool,
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

        let custom_headers: Vec<(String, String)> = custom_headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

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
            encode,
            is_json,
            body,
            injection_place,

            //to fill after the first request
            initial_response: None,
            amount_of_reflections: 0,

            parameters: HashMap::new(),
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
        format!("{}://{}:{}{}", self.scheme, self.host, self.port, self.path)
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
            self.encode,
            data_type,
            self.injection_place.clone(),
            &self.body
        ).unwrap()
    }
}