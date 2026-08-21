//! Asking the news server what still exists, before downloading anything.
//!
//! NNTP's `STAT` answers "is this article there?" without transferring a body, and `BODY` on a
//! par2 index file — kilobytes — reveals what the repair set can actually repair. Together they
//! answer in seconds what nzbget can only prove by downloading gigabytes.

use crate::error::{Error, Result};
use crate::settings::NewsServer;
use std::io::{BufRead, BufReader, Read, Write};

/// The questions the chase asks the news server directly. Behind a trait so a test never opens
/// a socket, and so the chase can be handed a fake that answers instantly.
pub trait Prober: Send + Sync {
    /// For each article, whether the server no longer has it. May return fewer answers than
    /// asked when the verdict became certain early; the answers given are in order.
    fn statuses(&self, news: &NewsServer, ids: &[&str]) -> Result<Vec<bool>>;
    /// One article's decoded body: how a par2 index is read without downloading the release.
    fn fetch_body(&self, news: &NewsServer, id: &str) -> Result<Vec<u8>>;
}

pub struct NntpProbe;

impl Prober for NntpProbe {
    fn statuses(&self, news: &NewsServer, ids: &[&str]) -> Result<Vec<bool>> {
        let stream = connect(news)?;
        stat_conversation(stream, news, ids)
    }

    fn fetch_body(&self, news: &NewsServer, id: &str) -> Result<Vec<u8>> {
        let stream = connect(news)?;
        body_conversation(stream, news, id)
    }
}

/// Read and Write together, so TLS and plain sockets walk through the same conversations.
trait Wire: Read + Write {}
impl<T: Read + Write> Wire for T {}

fn connect(news: &NewsServer) -> Result<Box<dyn Wire>> {
    let address = (news.host.as_str(), news.port);
    let unreachable = |detail: String| Error::Unreachable {
        what: news.host.clone(),
        detail,
    };
    let resolved = std::net::ToSocketAddrs::to_socket_addrs(&address)
        .map_err(|failure| unreachable(failure.to_string()))?
        .next()
        .ok_or_else(|| unreachable("no address".into()))?;
    let tcp = std::net::TcpStream::connect_timeout(&resolved, std::time::Duration::from_secs(10))
        .map_err(|failure| unreachable(failure.to_string()))?;
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(15)))
        .map_err(|failure| unreachable(failure.to_string()))?;
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(15)))
        .map_err(|failure| unreachable(failure.to_string()))?;

    if !news.encrypted {
        return Ok(Box::new(tcp));
    }

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|failure| unreachable(failure.to_string()))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(news.host.clone())
        .map_err(|failure| unreachable(failure.to_string()))?;
    let connection = rustls::ClientConnection::new(std::sync::Arc::new(config), name)
        .map_err(|failure| unreachable(failure.to_string()))?;
    Ok(Box::new(rustls::StreamOwned::new(connection, tcp)))
}

/// Bytes rather than text: article bodies are binary, and a text read would choke on them.
fn read_line<S: Read>(reader: &mut BufReader<S>, news: &NewsServer) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    reader
        .read_until(b'\n', &mut line)
        .map_err(|failure| Error::Unreachable {
            what: news.host.clone(),
            detail: failure.to_string(),
        })?;
    while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(line)
}

fn handshake<S: Read + Write>(reader: &mut BufReader<S>, news: &NewsServer) -> Result<()> {
    let unreadable = |detail: String| Error::Unreadable {
        what: news.host.clone(),
        detail,
    };
    let greeting = read_line(reader, news)?;
    if !greeting.starts_with(b"200") && !greeting.starts_with(b"201") {
        return Err(unreadable(format!(
            "greeting: {}",
            String::from_utf8_lossy(&greeting)
        )));
    }
    if news.username.is_empty() {
        return Ok(());
    }
    write!(reader.get_mut(), "AUTHINFO USER {}\r\n", news.username)
        .map_err(|failure| unreadable(failure.to_string()))?;
    let answer = read_line(reader, news)?;
    if answer.starts_with(b"381") {
        write!(reader.get_mut(), "AUTHINFO PASS {}\r\n", news.password)
            .map_err(|failure| unreadable(failure.to_string()))?;
        let answer = read_line(reader, news)?;
        if !answer.starts_with(b"281") {
            return Err(Error::Refused {
                what: news.host.clone(),
                status: 481,
                message: String::from_utf8_lossy(&answer).into_owned(),
            });
        }
    } else if !answer.starts_with(b"281") {
        return Err(Error::Refused {
            what: news.host.clone(),
            status: 481,
            message: String::from_utf8_lossy(&answer).into_owned(),
        });
    }
    Ok(())
}

/// The STAT conversation, over any stream, so the whole exchange is testable with a script.
/// Pipelined in batches: three hundred sequential round trips took a minute against a real
/// server, while a batch of commands costs one.
fn stat_conversation<S: Read + Write>(
    stream: S,
    news: &NewsServer,
    ids: &[&str],
) -> Result<Vec<bool>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut reader = BufReader::new(stream);
    handshake(&mut reader, news)?;

    let mut statuses = Vec::with_capacity(ids.len());
    let mut missing = 0usize;
    for chunk in ids.chunks(60) {
        let mut commands = String::new();
        for id in chunk {
            commands.push_str(&format!("STAT <{id}>\r\n"));
        }
        reader
            .get_mut()
            .write_all(commands.as_bytes())
            .map_err(|failure| Error::Unreachable {
                what: news.host.clone(),
                detail: failure.to_string(),
            })?;
        for _ in chunk {
            let answer = read_line(&mut reader, news)?;
            if answer.starts_with(b"223") {
                statuses.push(false);
            } else if answer.starts_with(b"430") {
                statuses.push(true);
                missing += 1;
            } else {
                // an answer we do not understand poisons the estimate: better no answer than a
                // wrong one, since the caller falls back to nzbget's slow certainty
                return Err(Error::Unreadable {
                    what: news.host.clone(),
                    detail: format!("STAT: {}", String::from_utf8_lossy(&answer)),
                });
            }
        }
        // a fifth of the sample gone exceeds any real par2 coverage: the verdict cannot change,
        // so the rest of the sample is not worth the server's time
        if statuses.len() >= 60 && missing as f64 / statuses.len() as f64 >= 0.2 {
            let _ = write!(reader.get_mut(), "QUIT\r\n");
            return Ok(statuses);
        }
    }
    let _ = write!(reader.get_mut(), "QUIT\r\n");
    Ok(statuses)
}

/// One article, decoded. A refusal here is an ordinary answer — the index may simply be gone —
/// and the caller treats it as "no evidence" rather than a verdict.
fn body_conversation<S: Read + Write>(stream: S, news: &NewsServer, id: &str) -> Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);
    handshake(&mut reader, news)?;
    write!(reader.get_mut(), "BODY <{id}>\r\n").map_err(|failure| Error::Unreachable {
        what: news.host.clone(),
        detail: failure.to_string(),
    })?;
    let answer = read_line(&mut reader, news)?;
    if !answer.starts_with(b"222") {
        return Err(Error::Refused {
            what: news.host.clone(),
            status: 430,
            message: String::from_utf8_lossy(&answer).into_owned(),
        });
    }
    let mut body = Vec::new();
    loop {
        let line = read_line(&mut reader, news)?;
        if line == b"." {
            break;
        }
        if body.len() > 4 * 1024 * 1024 {
            // an "index" this size is not an index; stop before it becomes a download
            return Err(Error::Unreadable {
                what: news.host.clone(),
                detail: "article too large for an index".into(),
            });
        }
        body.extend_from_slice(&line);
        body.push(b'\n');
    }
    let _ = write!(reader.get_mut(), "QUIT\r\n");
    Ok(crate::yenc::decode(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted server: hands out canned bytes, records everything said to it.
    struct Script {
        answers: std::io::Cursor<Vec<u8>>,
        said: Vec<u8>,
    }

    impl Script {
        fn answering(lines: &[&str]) -> Script {
            Script {
                answers: std::io::Cursor::new(
                    lines
                        .iter()
                        .map(|line| format!("{line}\r\n"))
                        .collect::<String>()
                        .into_bytes(),
                ),
                said: Vec::new(),
            }
        }
    }

    impl Read for &mut Script {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.answers.read(buffer)
        }
    }
    impl Write for &mut Script {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.said.extend_from_slice(buffer);
            Ok(buffer.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn news() -> NewsServer {
        NewsServer {
            host: "news.test".into(),
            port: 563,
            username: "reader".into(),
            password: "secret".into(),
            encrypted: true,
            connections: 8,
            retention_days: 0,
        }
    }

    #[test]
    fn counts_what_the_server_no_longer_has() {
        let mut script = Script::answering(&[
            "200 news.test ready",
            "381 password required",
            "281 welcome",
            "223 0 <a@x> exists",
            "430 no such article",
            "223 0 <c@x> exists",
            "430 no such article",
        ]);
        let statuses = stat_conversation(&mut script, &news(), &["a@x", "b@x", "c@x", "d@x"])
            .expect("answers");
        assert_eq!(statuses, [false, true, false, true]);

        let said = String::from_utf8(script.said).expect("utf8");
        assert!(said.contains("AUTHINFO USER reader\r\n"), "{said}");
        assert!(said.contains("AUTHINFO PASS secret\r\n"), "{said}");
        assert!(said.contains("STAT <a@x>\r\n"), "brackets added: {said}");
        assert!(said.ends_with("QUIT\r\n"), "{said}");
    }

    #[test]
    fn a_rejected_login_is_a_refusal_not_an_answer() {
        let mut script = Script::answering(&[
            "200 ready",
            "381 password required",
            "481 authentication failed",
        ]);
        let refused = stat_conversation(&mut script, &news(), &["a@x"]);
        assert!(matches!(refused, Err(Error::Refused { .. })));
    }

    #[test]
    fn an_answer_we_do_not_understand_poisons_the_estimate() {
        let mut script =
            Script::answering(&["200 ready", "381 ok", "281 in", "999 what even is this"]);
        assert!(stat_conversation(&mut script, &news(), &["a@x"]).is_err());
    }

    // The copy that ate the morning was 100% scrubbed: sixty answers said so, and asking the
    // remaining two hundred and forty would have been the server's time for nothing.
    #[test]
    fn certainty_ends_the_conversation_early() {
        let mut lines = vec!["200 ready", "381 ok", "281 in"];
        lines.extend(std::iter::repeat_n("430 gone", 60));
        // deliberately no answers beyond the first batch: needing them would fail the read
        let mut script = Script::answering(&lines);
        let ids: Vec<String> = (0..300).map(|n| format!("id-{n}@x")).collect();
        let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
        let statuses =
            stat_conversation(&mut script, &news(), &borrowed).expect("an early verdict");
        assert_eq!(statuses.len(), 60);
        assert!(statuses.iter().all(|missing| *missing));
    }

    #[test]
    fn nothing_to_sample_is_answered_without_a_conversation() {
        let mut script = Script::answering(&[]);
        assert!(stat_conversation(&mut script, &news(), &[])
            .expect("zero")
            .is_empty());
    }

    #[test]
    fn a_body_is_fetched_and_decoded_from_its_yenc_wrapping() {
        // assembled as bytes: yEnc is binary, and a string detour would mangle it — which is
        // the very reason the conversation reads bytes
        let mut wire = b"200 ready\r\n381 ok\r\n281 in\r\n222 0 <par@x> body follows\r\n".to_vec();
        wire.extend(crate::yenc::encode(b"PAR2 evidence bytes"));
        wire.extend(b".\r\n");
        let mut script = Script {
            answers: std::io::Cursor::new(wire),
            said: Vec::new(),
        };

        let body = body_conversation(&mut script, &news(), "par@x").expect("a body");
        assert_eq!(body, b"PAR2 evidence bytes");
        let said = String::from_utf8(script.said).expect("utf8");
        assert!(said.contains("BODY <par@x>\r\n"), "{said}");
    }

    #[test]
    fn a_missing_index_is_a_refusal_the_caller_treats_as_no_evidence() {
        let mut script =
            Script::answering(&["200 ready", "381 ok", "281 in", "430 no such article"]);
        assert!(matches!(
            body_conversation(&mut script, &news(), "gone@x"),
            Err(Error::Refused { .. })
        ));
    }
}
