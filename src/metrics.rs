use prometheus_client::{
    metrics::{
        counter::Counter,
        family::Family,
        histogram::{Histogram, exponential_buckets},
    },
    registry::Registry,
};

#[derive(Clone, Debug)]
pub struct AppMetrics {
    pub uploads: Counter,
    pub pastes: Counter,
    pub upload_bytes: Counter,
    pub served_files: Counter,
    pub reports: Counter,
    pub scanner_outcomes: Family<Vec<(String, String)>, Counter>,
    pub rate_limit_rejections: Counter,
    pub request_latency: Histogram,
}

impl AppMetrics {
    pub fn new() -> Self {
        Self {
            uploads: Counter::default(),
            pastes: Counter::default(),
            upload_bytes: Counter::default(),
            served_files: Counter::default(),
            reports: Counter::default(),
            scanner_outcomes: Family::default(),
            rate_limit_rejections: Counter::default(),
            request_latency: Histogram::new(exponential_buckets(0.005, 2.0, 12)),
        }
    }

    pub fn register(&self, registry: &mut Registry) {
        registry.register(
            "uploads",
            "Completed file uploads accepted by the application.",
            self.uploads.clone(),
        );
        registry.register(
            "pastes",
            "Completed paste creations accepted by the application.",
            self.pastes.clone(),
        );
        registry.register(
            "upload_bytes",
            "Bytes accepted through completed file uploads.",
            self.upload_bytes.clone(),
        );
        registry.register(
            "served_files",
            "Stored files served through public or raw file routes.",
            self.served_files.clone(),
        );
        registry.register(
            "reports",
            "Reports submitted through web or API routes.",
            self.reports.clone(),
        );
        registry.register(
            "scanner_outcomes",
            "Upload scanner decisions by decision label.",
            self.scanner_outcomes.clone(),
        );
        registry.register(
            "rate_limit_rejections",
            "Requests rejected by configured rate limits.",
            self.rate_limit_rejections.clone(),
        );
        registry.register(
            "request_latency_seconds",
            "End-to-end HTTP request latency in seconds.",
            self.request_latency.clone(),
        );
    }

    pub fn record_scanner_outcome(&self, decision: &str) {
        self.scanner_outcomes
            .get_or_create(&vec![("decision".to_string(), decision.to_string())])
            .inc();
    }
}

impl Default for AppMetrics {
    fn default() -> Self {
        Self::new()
    }
}
