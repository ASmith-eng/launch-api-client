//! Base URL constants, endpoint paths, and query parameter building for LL2.

/// Development server URL (used in tests — less restrictive rate limits).
#[cfg(test)]
pub const DEV_BASE_URL: &str = "https://lldev.thespacedevs.com/2.3.0";

/// Production base URL.
pub const PROD_BASE_URL: &str = "https://ll.thespacedevs.com/2.3.0";

// Endpoint paths (appended to the base URL).
const LAUNCHES_UPCOMING: &str = "/launches/upcoming/";
const LAUNCH_DETAIL: &str = "/launches/";
const API_THROTTLE: &str = "/api-throttle/";

/// Build the URL for fetching upcoming launches.
pub fn launches_upcoming_url(base_url: &str, params: &ListParams) -> String {
    let mut url = format!("{base_url}{LAUNCHES_UPCOMING}?mode=normal&ordering=net");

    url.push_str(&format!("&limit={}", params.limit));
    url.push_str(&format!("&offset={}", params.offset));

    if let Some(ref ids) = params.status_ids {
        url.push_str(&format!("&status__ids={ids}"));
    }
    if let Some(crewed) = params.is_crewed {
        url.push_str(&format!("&is_crewed={crewed}"));
    }
    if let Some(ref loc) = params.pad_location {
        url.push_str(&format!("&pad__location__in={loc}"));
    }
    if let Some(ref gt) = params.net_gt {
        url.push_str(&format!("&net__gt={gt}"));
    }
    if let Some(ref lt) = params.net_lt {
        url.push_str(&format!("&net__lt={lt}"));
    }
    if let Some(ref q) = params.search {
        url.push_str(&format!("&search={q}"));
    }

    url
}

/// Build the URL for fetching a single launch detail.
pub fn launch_detail_url(base_url: &str, id: &str) -> String {
    format!("{base_url}{LAUNCH_DETAIL}{id}/")
}

/// Build the URL for the rate limit throttle endpoint.
pub fn api_throttle_url(base_url: &str) -> String {
    format!("{base_url}{API_THROTTLE}")
}

/// Query parameters for the upcoming launches list endpoint.
#[derive(Debug, Clone)]
pub struct ListParams {
    pub limit: u32,
    pub offset: u32,
    pub status_ids: Option<String>,
    pub is_crewed: Option<bool>,
    pub pad_location: Option<String>,
    pub net_gt: Option<String>,
    pub net_lt: Option<String>,
    pub search: Option<String>,
}

impl Default for ListParams {
    fn default() -> Self {
        Self {
            limit: 25,
            offset: 0,
            status_ids: None,
            is_crewed: None,
            pad_location: None,
            net_gt: None,
            net_lt: None,
            search: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launches_url_default_params() {
        let url = launches_upcoming_url(DEV_BASE_URL, &ListParams::default());
        assert!(url.starts_with("https://lldev.thespacedevs.com/2.3.0/launches/upcoming/"));
        assert!(url.contains("mode=normal"));
        assert!(url.contains("ordering=net"));
        assert!(url.contains("limit=25"));
        assert!(url.contains("offset=0"));
        // No filter params in defaults
        assert!(!url.contains("status__ids"));
        assert!(!url.contains("is_crewed"));
    }

    #[test]
    fn launches_url_with_filters() {
        let params = ListParams {
            limit: 10,
            offset: 20,
            status_ids: Some("1,2".into()),
            is_crewed: Some(true),
            pad_location: Some("12".into()),
            net_gt: Some("2026-01-01".into()),
            net_lt: Some("2026-12-31".into()),
            search: Some("starship".into()),
        };

        let url = launches_upcoming_url(PROD_BASE_URL, &params);
        assert!(url.starts_with("https://ll.thespacedevs.com/2.3.0/launches/upcoming/"));
        assert!(url.contains("limit=10"));
        assert!(url.contains("offset=20"));
        assert!(url.contains("status__ids=1,2"));
        assert!(url.contains("is_crewed=true"));
        assert!(url.contains("pad__location__in=12"));
        assert!(url.contains("net__gt=2026-01-01"));
        assert!(url.contains("net__lt=2026-12-31"));
        assert!(url.contains("search=starship"));
    }

    #[test]
    fn detail_url() {
        let url = launch_detail_url(DEV_BASE_URL, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(
            url,
            "https://lldev.thespacedevs.com/2.3.0/launches/e3df2ecd-c239-472f-95e4-2b89b4f75800/"
        );
    }

    #[test]
    fn throttle_url() {
        let url = api_throttle_url(DEV_BASE_URL);
        assert_eq!(url, "https://lldev.thespacedevs.com/2.3.0/api-throttle/");
    }
}
