use serde::Serialize;

use crate::{utils::random_line, structs::InjectionPlace, network::request::VALUE_LENGTH};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ReasonKind {
    Code,
    Text,
    Reflected,
    NotReflected
}

#[derive(Debug, Clone, Serialize)]
pub struct FoundParameter {
    pub name: String,

    //is None in case the random parameter name is used
    pub value: Option<String>,
    pub diffs: String,
    pub status: u16,
    pub size: usize,
    pub reason_kind: ReasonKind
}

impl FoundParameter {
    pub fn new<S: Into<String>>(name: S, diffs: &Vec<String>, status: u16, size: usize, reason_kind: ReasonKind) -> Self {

        let name = name.into();

        let (name, value) = if name.contains("=") {
            let mut name = name.split("=");
            (name.next().unwrap().to_string(), Some(name.next().unwrap().to_string()))
        } else {
            (name, None)
        };

        Self {
            name,
            value,
            diffs: diffs.join("|"),
            status,
            size,
            reason_kind
        }
    }

    //just returns (Key, Value) pair
    pub fn get(&self) -> (String, String) {
        (self.name.clone(), self.value.clone().unwrap_or(random_line(VALUE_LENGTH)))
    }
}

pub trait Parameters {
    fn contains_name(&self, key: &str) -> bool;
    fn contains_name_case_insensitive(&self, key: &str) -> bool;
    fn contains_element(&self, el: &FoundParameter) -> bool;
    fn contains_element_case_insensitive(&self, el: &FoundParameter) -> bool;
    fn process(self, injection_place: InjectionPlace) -> Self;
}

impl Parameters for Vec<FoundParameter> {

    /// checks whether the element with the same name exists within the vector
    fn contains_name(&self, key: &str) -> bool {
        self.iter().any(|x| x.name == key)
    }

    fn contains_name_case_insensitive(&self, key: &str) -> bool {
        self.iter().any(|x| x.name.to_lowercase() == key.to_lowercase())
    }

    /// checks whether the combination of name, reason_kind, status exists within the vector
    fn contains_element(&self, el: &FoundParameter) -> bool {
        self.iter().any(|x| x.name == el.name && x.reason_kind == el.reason_kind && x.status == el.status)
    }

    fn contains_element_case_insensitive(&self, el: &FoundParameter) -> bool {
        self.iter().any(|x| x.name.to_lowercase() == el.name.to_lowercase() && x.reason_kind == el.reason_kind && x.status == el.status)
    }

    /// removes duplicates: [debug={random}, Debug={random}, debug=true] -> [debug={random}]
    /// not very fast but we are doing it a few times per run anyway
    fn process(mut self, injection_place: InjectionPlace) -> Self {
        fn capitalize_first(mut x: FoundParameter) -> FoundParameter {
            let mut chars = x.name.chars();
            x.name = chars
                .next()
                .map(|first_letter| first_letter.to_uppercase())
                .into_iter()
                .flatten()
                .chain(chars)
                .collect();
            x
        }

        //in case, for example, 'admin' param is found -- remove params like 'admin=true' or sth
        self = self.iter().filter(
                |x| !(x.name.contains('=') && self.contains_element(x))
        ).map(|x| x.to_owned()).collect();

        //if there's lowercase alternative - remove that parameter
        //so Host & HOST & host are the same parameters and only host should stay
        self = self.iter().filter(
            |x| x.name.to_lowercase() == x.name || !self.contains_name(&x.name.to_lowercase())
        ).map(|x| x.to_owned()).collect();

        //for now reqwest capitalizes first char of every header
        self = if injection_place == InjectionPlace::Headers {
            self.iter().map(|x| capitalize_first(x.to_owned())).collect()
        } else {
            self
        };

        //if there's HOST and Host only one of them should stay
        let mut found_params = vec![];
        for el in self {
            if !found_params.contains_name_case_insensitive(&el.name) {
                found_params.push(el);
            }
        }

        found_params
    }
}