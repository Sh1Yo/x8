#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use crate::{structs::{InjectionPlace, DataType, Headers}, network::request::{RequestDefaults, Request}};

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
            InjectionPlace::Path,
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
        assert!(request.defaults.path.starts_with("/?param="));
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