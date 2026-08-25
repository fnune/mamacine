//! What GitHub Releases says the newest version is.

use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use serde_json::Value;

/// The newest published release, reduced to what updating needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Release {
    /// The tag with any leading `v` removed: "0.2.0".
    pub version: String,
    /// The release's own page, for when no asset fits.
    pub page_url: String,
    pub appimage_url: Option<String>,
    pub installer_url: Option<String>,
    pub checksums_url: Option<String>,
}

pub struct GithubReleases<H> {
    repo: &'static str,
    http: H,
}

impl<H: HttpClient> GithubReleases<H> {
    pub fn new(repo: &'static str, http: H) -> Self {
        GithubReleases { repo, http }
    }

    /// The latest release, or nothing when none has been published yet.
    pub fn latest(&self) -> Result<Option<Release>> {
        let request = Request::get(format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repo
        ))
        .header("User-Agent", "MamaCine/1.0")
        .header("Accept", "application/vnd.github+json");
        let response = self.http.send(request)?;
        if response.status == 404 {
            return Ok(None);
        }
        let response = expect_success("github releases", response)?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "github releases".into(),
                detail: failure.to_string(),
            })?;
        Ok(parse_release(&answer))
    }

    pub fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        let request = Request::get(url.to_string()).header("User-Agent", "MamaCine/1.0");
        Ok(expect_success("github releases", self.http.send(request)?)?.body)
    }
}

pub fn parse_release(answer: &Value) -> Option<Release> {
    let tag = answer.get("tag_name")?.as_str()?;
    let mut release = Release {
        version: tag.trim_start_matches('v').to_string(),
        page_url: answer
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..Release::default()
    };
    for asset in answer.get("assets").and_then(Value::as_array)? {
        let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        if name.ends_with(".AppImage") {
            release.appimage_url = Some(url.to_string());
        } else if name.ends_with("-setup.exe") {
            release.installer_url = Some(url.to_string());
        } else if name == "checksums.txt" {
            release.checksums_url = Some(url.to_string());
        }
    }
    Some(release)
}

/// Whether `found` is a strictly newer version than `running`. Anything unparseable is not
/// newer: an update must never be offered on a guess.
pub fn newer_than(found: &str, running: &str) -> bool {
    match (numbers_of(found), numbers_of(running)) {
        (Some(found), Some(running)) => found > running,
        _ => false,
    }
}

fn numbers_of(version: &str) -> Option<(u64, u64, u64)> {
    let plain = version.trim().trim_start_matches('v');
    let plain = plain.split(['-', '+']).next()?;
    let mut parts = plain.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// The recorded hash for one file, out of a `sha256sum` listing.
pub fn checksum_for(checksums: &str, file_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file_name).then(|| hash.to_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;

    fn answer() -> &'static str {
        r#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/fnune/mamacine/releases/tag/v0.2.0",
            "assets": [
                {"name": "MamaCine-x86_64.AppImage",
                 "browser_download_url": "https://github.com/fnune/mamacine/releases/download/v0.2.0/MamaCine-x86_64.AppImage"},
                {"name": "MamaCine-x64-setup.exe",
                 "browser_download_url": "https://github.com/fnune/mamacine/releases/download/v0.2.0/MamaCine-x64-setup.exe"},
                {"name": "checksums.txt",
                 "browser_download_url": "https://github.com/fnune/mamacine/releases/download/v0.2.0/checksums.txt"}
            ]
        }"#
    }

    #[test]
    fn reads_the_release_and_its_assets() {
        let service = GithubReleases::new(
            "fnune/mamacine",
            FakeHttp::answering(vec![FakeHttp::status(200, answer())]),
        );
        let release = service.latest().expect("an answer").expect("a release");
        assert_eq!(release.version, "0.2.0");
        assert!(release
            .appimage_url
            .expect("appimage")
            .ends_with(".AppImage"));
        assert!(release
            .installer_url
            .expect("installer")
            .ends_with("-setup.exe"));
        assert!(release
            .checksums_url
            .expect("checksums")
            .ends_with("checksums.txt"));
    }

    #[test]
    fn no_release_yet_is_an_answer_and_not_a_failure() {
        let service = GithubReleases::new(
            "fnune/mamacine",
            FakeHttp::answering(vec![FakeHttp::status(404, r#"{"message":"Not Found"}"#)]),
        );
        assert_eq!(service.latest().expect("an answer"), None);
    }

    #[test]
    fn a_version_is_newer_only_when_it_actually_is() {
        assert!(newer_than("v0.2.0", "0.1.0"));
        assert!(newer_than("1.0.0", "0.9.9"));
        assert!(!newer_than("0.1.0", "0.1.0"));
        assert!(!newer_than("0.1.0", "0.2.0"));
        assert!(!newer_than("garbage", "0.1.0"), "a guess is not an update");
        assert!(newer_than("0.2", "0.1.9"), "short versions still compare");
    }

    #[test]
    fn the_recorded_checksum_is_found_by_file_name() {
        let listing = "abc123  MamaCine-x86_64.AppImage\ndef456  MamaCine-x64-setup.exe\n";
        assert_eq!(
            checksum_for(listing, "MamaCine-x86_64.AppImage"),
            Some("abc123".into())
        );
        assert_eq!(checksum_for(listing, "nothing.txt"), None);
    }
}
