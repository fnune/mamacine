//! The single seam to the network.

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

/// A per-host floor between calls; callers queue.
pub struct Throttle<H> {
    inner: H,
    floor: std::time::Duration,
    reserved: std::sync::Mutex<BTreeMap<String, std::time::Instant>>,
    sleep: Box<dyn Fn(std::time::Duration) + Send + Sync>,
}

impl<H: HttpClient> Throttle<H> {
    pub fn new(inner: H, floor: std::time::Duration) -> Self {
        Throttle {
            inner,
            floor,
            reserved: std::sync::Mutex::new(BTreeMap::new()),
            sleep: Box::new(std::thread::sleep),
        }
    }

    #[cfg(test)]
    fn sleeping(mut self, sleep: impl Fn(std::time::Duration) + Send + Sync + 'static) -> Self {
        self.sleep = Box::new(sleep);
        self
    }
}

impl<H: HttpClient> HttpClient for Throttle<H> {
    fn send(&self, request: Request) -> Result<Response> {
        let host = host_of(&request.url);
        let wait = {
            let mut reserved = self.reserved.lock().expect("not poisoned");
            let now = std::time::Instant::now();
            let slot = match reserved.get(&host) {
                Some(previous) if *previous + self.floor > now => *previous + self.floor,
                _ => now,
            };
            reserved.insert(host, slot);
            slot.duration_since(now)
        };
        if !wait.is_zero() {
            (self.sleep)(wait);
        }
        self.inner.send(request)
    }
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .to_lowercase()
}

/// Refuses anything that is not a success.
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
mod tests {
    use super::fake::FakeHttp;
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn throttled(
        floor: Duration,
        answers: usize,
    ) -> (Throttle<FakeHttp>, Arc<Mutex<Vec<Duration>>>) {
        let slept = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&slept);
        let throttle = Throttle::new(
            FakeHttp::answering(vec![FakeHttp::ok("answered"); answers]),
            floor,
        )
        .sleeping(move |wait| recorded.lock().expect("not poisoned").push(wait));
        (throttle, slept)
    }

    #[test]
    fn a_second_call_to_the_same_host_waits_out_the_floor() {
        let (throttle, slept) = throttled(Duration::from_secs(2), 2);
        throttle
            .send(Request::get("https://api.test/one"))
            .expect("first");
        throttle
            .send(Request::get("https://api.test/two"))
            .expect("second");
        let slept = slept.lock().expect("not poisoned");
        assert_eq!(slept.len(), 1, "only the second call waits");
        assert!(slept[0] <= Duration::from_secs(2));
        assert!(
            slept[0] >= Duration::from_secs(1),
            "close to the floor: {:?}",
            slept[0]
        );
    }

    #[test]
    fn different_hosts_never_queue_behind_each_other() {
        let (throttle, slept) = throttled(Duration::from_secs(2), 2);
        throttle
            .send(Request::get("https://api.test/one"))
            .expect("first");
        throttle
            .send(Request::get("https://images.test/two"))
            .expect("second");
        assert!(slept.lock().expect("not poisoned").is_empty());
    }

    #[test]
    fn eager_callers_queue_one_slot_apiece_rather_than_pile_up() {
        let (throttle, slept) = throttled(Duration::from_secs(2), 3);
        throttle
            .send(Request::get("https://api.test/one"))
            .expect("first");
        throttle
            .send(Request::get("https://api.test/two"))
            .expect("second");
        throttle
            .send(Request::get("https://api.test/three"))
            .expect("third");
        let slept = slept.lock().expect("not poisoned");
        assert_eq!(slept.len(), 2);
        assert!(
            slept[1] > slept[0] + Duration::from_millis(1500),
            "the third waits a whole floor behind the second: {slept:?}"
        );
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

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
