pub trait Headers {
    fn contains_key(&self, key: &str) -> bool;
    fn get_value(&self, key: &str) -> Option<String>;
    fn get_value_case_insensitive(&self, key: &str) -> Option<String>;
}

impl Headers for Vec<(String, String)> {
    fn contains_key(&self, key: &str) -> bool {
        for (k, _) in self.iter() {
            if k == key {
                return true
            }
        }
        false
    }

    fn get_value(&self, key: &str) -> Option<String> {
        for (k, v) in self.iter() {
            if k == key {
                return Some(v.to_owned())
            }
        }
        None
    }

    fn get_value_case_insensitive(&self, key: &str) -> Option<String> {
        let key = key.to_lowercase();
        for (k, v) in self.iter() {
            if k.to_lowercase() == key {
                return Some(v.to_owned())
            }
        }
        None
    }

}