#[cfg(test)]
mod tests {
    use tokio::time::Duration;

    use crate::network::{
        request::{Request, RequestDefaults},
        utils::{Headers, InjectionPlace, is_binary_content},
    };

    #[test]
    fn check_is_binary_content(){
        assert!(is_binary_content(Some("application/pdf".to_string())));
        assert!(is_binary_content(Some("something/zip".to_string())));
        assert!(is_binary_content(Some("image/png".to_string())));
        assert!(is_binary_content(Some("image/something".to_string())));

        assert!(!is_binary_content(Some("application/json".to_string())));
        assert!(!is_binary_content(Some("application/html".to_string())));
    }

    #[test]
    fn query_creation() {
        let mut l = RequestDefaults::default();
        l.template = "%k=payload".to_string();
        l.joiner = "&".to_string();
        let parameters = vec!["test1".to_string()];
        let mut request = Request::new(&l, parameters);
        request.prepare();

        assert_eq!(request.make_query(), "test1=payload");
    }

    #[test]
    fn request_defaults_generation() {
        let defaults = RequestDefaults::new::<String>(
            "GET",
            "https://example.com:8443/path",
            Vec::from([("X-Header".to_string(), "Value".to_string())]),
            Duration::from_millis(0),
            Default::default(),
            None,
            None,
            false,
            None,
            false,
            false,
            "",
            false,
            false,
        )
        .unwrap();

        assert_eq!(defaults.scheme, "https");
        assert_eq!(defaults.host, "example.com");
        assert_eq!(defaults.port, 8443);
        assert_eq!(defaults.path, "/path?%s");
        assert_eq!(
            defaults.custom_headers.get_value("X-Header").unwrap(),
            "Value"
        );
        assert_eq!(defaults.template, "%k=%v");
        assert_eq!(defaults.joiner, "&");
        assert_eq!(defaults.injection_place, InjectionPlace::Path);
    }

    #[test]
    fn json_request_body_generation() {
        let defaults = RequestDefaults::new::<String>(
            "POST",
            "https://example.com:8443/path",
            Vec::from([("X-Header".to_string(), "Value".to_string())]),
            Duration::from_millis(0),
            Default::default(),
            None,
            None,
            false,
            None,
            false,
            false,
            "{\"something\":1}",
            false,
            false,
        )
        .unwrap();

        assert!(defaults.is_json);
        assert_eq!(defaults.body, "{\"something\":1, %s}");
        assert_eq!(defaults.template, "\"%k\": %v");
    }
}
