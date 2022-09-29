use crate::{
    structs::{InjectionPlace, Headers, DataType}, utils::{random_line},
};
use itertools::Itertools;
use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use reqwest::Client;
use url::Url;
use std::{
    error::Error, collections::HashMap, iter::FromIterator, time::{Duration, Instant}, convert::TryFrom,
};

use super::response::Response;

lazy_static! {
    //characters to encode
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

#[derive(Debug, Clone)]
pub struct RequestDefaults {
    //default request data
    pub method: String,
    pub scheme: String,
    pub path: String,
    pub host: String,
    pub port: u16,

    //custom user supplied headers or default ones
    pub custom_headers: Vec<(String, String)>,

    //how much to sleep between requests in millisecs
    pub delay: Duration, //MOVE to config

    //the initial response to compare with.
    //can be None at the start when no requests were made yet
    //probably better to adjust the logic and keep just Response
    //pub initial_response: Option<Response<'a>>,

    //default reqwest client
    pub client: Client,

    //parameter template, for example %k=%v
    pub template: String,

    //how to join parameters, for example '&'
    pub joiner: String,

    //whether to encode the query like param1=value1&param2=value2 -> param1%3dvalue1%26param2%3dvalue2
    pub encode: bool,

    //to replace {"key": "false"} with {"key": false}
    pub is_json: bool,

    //default body
    pub body: String,

    //parameters to add to every request
    //it is used in recursion search
    pub parameters: Vec<(String, String)>,

    //where the injection point is
    pub injection_place: InjectionPlace,

    //the default amount of reflection per non existing parameter
    pub amount_of_reflections: usize

}

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub defaults: &'a RequestDefaults,

    //vector of supplied parameters
    pub parameters: Vec<String>,

    //parsed parameters (key, value)
    pub prepared_parameters: Vec<(String, String)>,

    //parameters with not random values
    //we need this vector to ignore searching for reflections for these parameters
    //for example admin=1 - its obvious that 1 can be reflected unpredictable amount of times
    pub non_random_parameters: Vec<(String, String)>,

    pub headers: Vec<(String, String)>,

    pub body: String,

    //we can't use defaults.path because there can be {{random}} variable that need to be replaced
    pub path: String,

    //whether the request was prepared
    //{{random}} things replaced, prepared_parameters filled
    pub prepared: bool
}

impl<'a> Request<'a> {

    pub fn new(l: &'a RequestDefaults, parameters: Vec<String>) -> Self {
        Self{
            path: l.path.to_owned(),
            defaults: l,
            headers: Vec::new(),
            body: String::new(),
            parameters: parameters,
            prepared_parameters: l.parameters.clone(),
            non_random_parameters: Vec::new(),
            prepared: false,
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
    pub fn prepare(&mut self, additional_param: Option<&String>) {
        if self.prepared {
            return
        }
        self.prepared = true;

        self.non_random_parameters = Vec::from_iter(
            self.parameters
                .iter()
                .filter(|x| x.contains("%=%"))
                .map(|x| x.split("%=%"))
                .map(|mut x| (x.next().unwrap().to_owned(), x.next().unwrap_or("").to_owned()))
        );

        self.prepared_parameters = Vec::from_iter(
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
        self.path = self.path.replace("{{random}}", &random_line(5));
        self.body = self.body.replace("{{random}}", &random_line(5));

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
            .method(self.defaults.method.as_str())
            .uri(self.url());

        for (k, v) in &self.headers {
            request = request.header(k,v)
        };

        let request = request
            .body(self.body.to_owned())
            .unwrap();

        std::thread::sleep(self.defaults.delay);

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
            request: Some(self),
            reflected_parameters: HashMap::new(),
            additional_parameter: additional_parameter
        };

        response.beautify_body();
        response.add_headers();

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
            request: Some(self),
        }
    }

    pub fn print(&mut self) -> String {
        self.prepare(Some(&random_line(5)));

        let mut str_req = format!("{} {} HTTP/x\nHost: {}\n", &self.defaults.method, self.path, self.defaults.host); //TODO identify HTTP version

        for (k, v) in self.headers.iter().sorted() {
            str_req += &format!("{}: {}\n", k, v)
        }

        str_req += &format!("\n{}", self.body);

        str_req
    }
}

impl<'a> Default for RequestDefaults {
    fn default() -> RequestDefaults {
        RequestDefaults {
            method: "GET".to_string(),
            scheme: "https".to_string(),
            path: "/".to_string(),
            host: "example.com".to_string(),
            custom_headers: Vec::new(),
            port: 443,
            delay: Duration::from_millis(0),
            client: Default::default(),
            template: "{k}={v}".to_string(),
            joiner: "&".to_string(),
            is_json: false,
            encode: false,
            body: String::new(),
            parameters: Vec::new(),
            injection_place: InjectionPlace::Path,
            amount_of_reflections: 0
        }
    }
}

impl<'a> RequestDefaults {
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

            amount_of_reflections: 0,

            parameters: Vec::new(),
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