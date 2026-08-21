//! The single seam through which this crate reaches the network.
//!
//! Every remote service is built on top of this trait, so a test never needs a socket and can
//! never accidentally reach the real service: the real client is simply not constructed.

use crate::error::{Error, Result};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Request {
            method: Method::Get,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
        }
    }

    pub fn post_json(url: impl Into<String>, body: &serde_json::Value) -> Self {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: BTreeMap::from([("Content-Type".into(), "application/json".into())]),
            body: Some(body.to_string().into_bytes()),
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

pub trait HttpClient: Send + Sync {
    fn send(&self, request: Request) -> Result<Response>;
}

impl<T: HttpClient + ?Sized> HttpClient for &T {
    fn send(&self, request: Request) -> Result<Response> {
        (**self).send(request)
    }
}

/// Refuses anything that is not a success, so callers do not have to remember to check.
pub fn expect_success(what: &str, response: Response) -> Result<Response> {
    if (200..300).contains(&response.status) {
        return Ok(response);
    }
    let message = response.text();
    Err(Error::Refused {
        what: what.to_string(),
        status: response.status,
        message: message.chars().take(200).collect(),
    })
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// Answers from a script of canned responses and records what it was asked.
    #[derive(Default)]
    pub struct FakeHttp {
        answers: Mutex<Vec<Result<Response>>>,
        pub seen: Mutex<Vec<Request>>,
    }

    impl FakeHttp {
        pub fn answering(answers: Vec<Response>) -> Self {
            FakeHttp {
                answers: Mutex::new(answers.into_iter().map(Ok).collect()),
                seen: Mutex::new(Vec::new()),
            }
        }

        pub fn ok(body: &str) -> Response {
            Response {
                status: 200,
                content_type: "application/xml".into(),
                body: body.as_bytes().to_vec(),
            }
        }

        pub fn status(status: u16, body: &str) -> Response {
            Response {
                status,
                content_type: "application/json".into(),
                body: body.as_bytes().to_vec(),
            }
        }

        pub fn requests(&self) -> Vec<Request> {
            self.seen.lock().expect("not poisoned").clone()
        }

        pub fn last_url(&self) -> String {
            self.requests().last().expect("a request").url.clone()
        }
    }

    impl HttpClient for FakeHttp {
        fn send(&self, request: Request) -> Result<Response> {
            self.seen
                .lock()
                .expect("not poisoned")
                .push(request.clone());
            let mut answers = self.answers.lock().expect("not poisoned");
            if answers.is_empty() {
                return Err(Error::Unreachable {
                    what: "the fake".into(),
                    detail: format!("no answer scripted for {}", request.url),
                });
            }
            answers.remove(0)
        }
    }
}
