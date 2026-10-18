//! HTTP execution boundary shared by legacy and recovery gateways.
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};

/// Executes an encoded request. Implementations may supply deterministic response
/// streams, including body failures, without opening a connection.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    /// Execute one wire attempt without hidden retries.
    async fn execute(&self, request: reqwest::Request) -> reqwest::Result<reqwest::Response>;
}

#[derive(Clone)]
pub(crate) struct Client {
    inner: reqwest::Client,
    transport: Option<Arc<dyn HttpTransport>>,
}
impl From<reqwest::Client> for Client {
    fn from(inner: reqwest::Client) -> Self {
        Self {
            inner,
            transport: None,
        }
    }
}
impl Client {
    pub(crate) fn new() -> Self {
        reqwest::Client::new().into()
    }
    pub(crate) fn with_transport(mut self, transport: Arc<dyn HttpTransport>) -> Self {
        self.transport = Some(transport);
        self
    }
    pub(crate) fn transport(&self) -> Option<Arc<dyn HttpTransport>> {
        self.transport.clone()
    }
    pub(crate) async fn execute(
        &self,
        request: reqwest::Request,
    ) -> reqwest::Result<reqwest::Response> {
        match &self.transport {
            Some(transport) => transport.execute(request).await,
            None => self.inner.execute(request).await,
        }
    }
    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        url: impl reqwest::IntoUrl,
    ) -> RequestBuilder {
        RequestBuilder {
            inner: self.inner.request(method, url),
            client: self.clone(),
        }
    }
    pub(crate) fn get(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.request(reqwest::Method::GET, url)
    }
    pub(crate) fn post(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.request(reqwest::Method::POST, url)
    }
}

pub(crate) struct RequestBuilder {
    inner: reqwest::RequestBuilder,
    client: Client,
}
impl RequestBuilder {
    pub(crate) fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.inner = self.inner.json(body);
        self
    }
    pub(crate) fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.header(key.as_ref(), value.as_ref());
        self
    }
    pub(crate) fn bearer_auth(mut self, token: impl std::fmt::Display) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }
    pub(crate) fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }
    pub(crate) fn build(self) -> reqwest::Result<reqwest::Request> {
        self.inner.build()
    }
    pub(crate) async fn send(self) -> reqwest::Result<reqwest::Response> {
        self.client.execute(self.inner.build()?).await
    }
}
