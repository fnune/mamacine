//! The one place a real socket is opened. Everything else takes an `HttpClient`.

use crate::error::{Error, Result};
use crate::http::{HttpClient, Method, Request, Response};
use std::time::Duration;

pub struct Network {
    agent: ureq::Agent,
}

impl Network {
    pub fn new() -> Self {
        Network {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .timeout(Duration::from_secs(60))
                .build(),
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Network::new()
    }
}

impl HttpClient for Network {
    fn send(&self, request: Request) -> Result<Response> {
        let mut call = match request.method {
            Method::Get => self.agent.get(&request.url),
            Method::Post => self.agent.post(&request.url),
        };
        for (name, value) in &request.headers {
            call = call.set(name, value);
        }

        let outcome = match &request.body {
            Some(body) => call.send_bytes(body),
            None => call.call(),
        };

        // a refusal is an answer, not a failure to reach: let the caller phrase it
        let response = match outcome {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(transport)) => {
                return Err(Error::Unreachable {
                    what: host_of(&request.url),
                    detail: transport.to_string(),
                })
            }
        };

        let status = response.status();
        let content_type = response.content_type().to_string();
        let mut body = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|failure| Error::Unreachable {
                what: host_of(&request.url),
                detail: failure.to_string(),
            })?;

        Ok(Response {
            status,
            content_type,
            body,
        })
    }
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .to_string()
}

use std::io::Read;
