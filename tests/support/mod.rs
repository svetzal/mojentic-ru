//! In-process HTTP fixtures. Matching is applied to the encoded public request;
//! responses come from each suite's versioned contracts or captured wire fixtures.
use async_trait::async_trait;
use mojentic::llm::transport::HttpTransport;
use serde_json::Value;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock, Weak,
};

type Registry = std::collections::HashMap<String, Weak<State>>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
pub fn transport_for(url: &str) -> Arc<dyn HttpTransport> {
    let origin = reqwest::Url::parse(url).expect("fixture URL").origin().ascii_serialization();
    registry()
        .lock()
        .expect("registry")
        .get(&origin)
        .and_then(Weak::upgrade)
        .expect("registered in-process fixture")
}
#[derive(Clone, Debug)]
pub enum Matcher {
    Any,
    Missing,
    Regex(String),
    Json(Value),
    JsonString(String),
    PartialJson(Value),
    Exact(String),
}
impl From<&str> for Matcher {
    fn from(value: &str) -> Self {
        Self::Exact(value.into())
    }
}
impl From<String> for Matcher {
    fn from(value: String) -> Self {
        Self::Exact(value)
    }
}
fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            e.iter().all(|(k, v)| a.get(k).is_some_and(|a| subset(v, a)))
        }
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(e, a)| subset(e, a))
        }
        _ => expected == actual,
    }
}
impl Matcher {
    fn matches(&self, text: Option<&str>) -> bool {
        match self {
            Self::Any => true,
            Self::Missing => text.is_none(),
            Self::Exact(e) => text == Some(e),
            Self::Regex(e) => {
                text.is_some_and(|s| regex::Regex::new(e).expect("fixture regex").is_match(s))
            }
            Self::Json(e) => {
                text.and_then(|s| serde_json::from_str::<Value>(s).ok()).as_ref() == Some(e)
            }
            Self::JsonString(e) => {
                Self::Json(serde_json::from_str(e).expect("fixture JSON")).matches(text)
            }
            Self::PartialJson(e) => text
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|a| subset(e, &a)),
        }
    }
}
pub struct Request {
    inner: reqwest::Request,
}
impl Request {
    pub fn body(&self) -> Result<&[u8], &'static str> {
        self.inner.body().and_then(reqwest::Body::as_bytes).ok_or("non-buffered body")
    }
}
type Predicate = Box<dyn Fn(&Request) -> bool + Send + Sync>;
type ResponseFactory = Arc<dyn Fn(reqwest::Request) -> reqwest::Response + Send + Sync>;
type BodyFactory = Box<dyn Fn(&Request) -> Vec<u8> + Send + Sync>;
struct Rule {
    method: String,
    path: Matcher,
    body_match: Matcher,
    headers_match: Vec<(String, Matcher)>,
    predicate: Option<Predicate>,
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    body_factory: Option<BodyFactory>,
    hits: usize,
    min: usize,
    max: usize,
}
#[derive(Default)]
struct State {
    failure: Mutex<Option<std::io::ErrorKind>>,
    handler: Mutex<Option<ResponseFactory>>,
    rules: Mutex<Vec<Arc<Mutex<Rule>>>>,
    requests: Mutex<Vec<reqwest::Request>>,
}
#[async_trait]
impl HttpTransport for State {
    async fn execute(&self, request: reqwest::Request) -> reqwest::Result<reqwest::Response> {
        let failure = *self.failure.lock().expect("failure");
        if let Some(kind) = failure {
            let response: reqwest::Response =
                http::Response::new(reqwest::Body::wrap_stream(futures::stream::iter([Err::<
                    Vec<u8>,
                    _,
                >(
                    std::io::Error::new(kind, "in-process transport failure"),
                )])))
                .into();
            return Err(response.bytes().await.expect_err("failed transport"));
        }
        let request = Request { inner: request };
        self.requests
            .lock()
            .expect("requests")
            .push(request.inner.try_clone().expect("buffered request"));
        if let Some(handler) = self.handler.lock().expect("handler").clone() {
            return Ok(handler(request.inner));
        }
        let rules = self.rules.lock().expect("rules").clone();
        for rule in rules {
            let mut rule = rule.lock().expect("rule");
            let mut path = request.inner.url().path().to_owned();
            if let Some(query) = request.inner.url().query() {
                path.push('?');
                path.push_str(query);
            }
            if rule.hits >= rule.max
                || rule.method != request.inner.method().as_str()
                || !rule.path.matches(Some(&path))
                || !rule
                    .body_match
                    .matches(request.body().ok().and_then(|b| std::str::from_utf8(b).ok()))
                || !rule.headers_match.iter().all(|(k, m)| {
                    m.matches(request.inner.headers().get(k).and_then(|v| v.to_str().ok()))
                })
                || rule.predicate.as_ref().is_some_and(|p| !p(&request))
            {
                continue;
            }
            rule.hits += 1;
            let body =
                rule.body_factory.as_ref().map_or_else(|| rule.body.clone(), |f| f(&request));
            let mut response = http::Response::builder().status(rule.status);
            for (k, v) in &rule.headers {
                response = response.header(k, v);
            }
            return Ok(response.body(reqwest::Body::from(body)).expect("fixture response").into());
        }
        panic!(
            "unexpected in-process request: {} {} body={:?}",
            request.inner.method(),
            request.inner.url(),
            request.body()
        );
    }
}
pub struct Fixture {
    state: Arc<State>,
    url: String,
}
pub type FixtureGuard = Fixture;
impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let url = format!("http://fixture-{}.invalid", NEXT.fetch_add(1, Ordering::Relaxed));
        let state = Arc::new(State::default());
        registry().lock().expect("registry").insert(url.clone(), Arc::downgrade(&state));
        Self { state, url }
    }
    pub async fn new_async() -> Self {
        Self::new()
    }
    pub fn url(&self) -> String {
        self.url.clone()
    }
    pub fn fail(&self, kind: std::io::ErrorKind) {
        *self.state.failure.lock().expect("failure") = Some(kind);
    }
    pub fn respond(
        &self,
        handler: impl Fn(reqwest::Request) -> reqwest::Response + Send + Sync + 'static,
    ) {
        *self.state.handler.lock().expect("handler") = Some(Arc::new(handler));
    }
    pub fn transport(&self) -> Arc<dyn HttpTransport> {
        self.state.clone()
    }
    pub fn requests(&self) -> std::sync::MutexGuard<'_, Vec<reqwest::Request>> {
        self.state.requests.lock().expect("requests")
    }
    pub fn mock(&mut self, method: &str, path: impl Into<Matcher>) -> Mock {
        Mock {
            state: self.state.clone(),
            rule: Arc::new(Mutex::new(Rule {
                method: method.into(),
                path: path.into(),
                body_match: Matcher::Any,
                headers_match: vec![],
                predicate: None,
                status: 200,
                headers: vec![],
                body: vec![],
                body_factory: None,
                hits: 0,
                min: 1,
                max: 1,
            })),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        registry().lock().expect("registry").remove(&self.url);
    }
}
pub struct Mock {
    state: Arc<State>,
    rule: Arc<Mutex<Rule>>,
}
impl Mock {
    pub fn with_status(self, status: usize) -> Self {
        self.rule.lock().expect("rule").status = status as u16;
        self
    }
    pub fn with_header(self, key: &str, value: &str) -> Self {
        self.rule.lock().expect("rule").headers.push((key.into(), value.into()));
        self
    }
    pub fn with_body(self, body: impl AsRef<[u8]>) -> Self {
        self.rule.lock().expect("rule").body = body.as_ref().to_vec();
        self
    }
    pub fn with_body_from_request(
        self,
        f: impl Fn(&Request) -> Vec<u8> + Send + Sync + 'static,
    ) -> Self {
        self.rule.lock().expect("rule").body_factory = Some(Box::new(f));
        self
    }
    pub fn match_body(self, matcher: impl Into<Matcher>) -> Self {
        self.rule.lock().expect("rule").body_match = matcher.into();
        self
    }
    pub fn match_header(self, key: &str, matcher: impl Into<Matcher>) -> Self {
        self.rule.lock().expect("rule").headers_match.push((key.into(), matcher.into()));
        self
    }
    pub fn match_request(self, f: impl Fn(&Request) -> bool + Send + Sync + 'static) -> Self {
        self.rule.lock().expect("rule").predicate = Some(Box::new(f));
        self
    }
    pub fn expect(self, count: usize) -> Self {
        {
            let mut rule = self.rule.lock().expect("rule");
            rule.min = count;
            rule.max = count;
        }
        self
    }
    pub fn expect_at_least(self, count: usize) -> Self {
        {
            let mut rule = self.rule.lock().expect("rule");
            rule.min = count;
            rule.max = usize::MAX;
        }
        self
    }
    pub fn create(self) -> Self {
        self.state.rules.lock().expect("rules").push(self.rule.clone());
        self
    }
    pub async fn create_async(self) -> Self {
        self.create()
    }
    pub fn assert(&self) {
        let rule = self.rule.lock().expect("rule");
        assert!(
            rule.hits >= rule.min && rule.hits <= rule.max,
            "{} {:?}: expected {}..={}, got {}",
            rule.method,
            rule.path,
            rule.min,
            rule.max,
            rule.hits
        );
    }
    pub async fn assert_async(&self) {
        self.assert()
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}
