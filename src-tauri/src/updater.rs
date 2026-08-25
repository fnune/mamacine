use mamacine_core::error::{Error, Result};
use mamacine_core::http::HttpClient;
use mamacine_core::updates::{checksum_for, newer_than, GithubReleases, Release};
use std::path::{Path, PathBuf};

/// What the window is told about an update, through the progress poll.
#[derive(Clone, Debug, serde::Serialize)]
pub struct UpdateNews {
    pub version: String,
    /// The new copy is already in place and starts on the next launch.
    pub installed: bool,
}

/// What to do about the newest release, decided from facts alone.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Open this in the browser; the person takes it from there.
    Open { version: String, url: String },
    /// Download the installer, verify it, and run it: the installer asks about the running app.
    RunInstaller {
        version: String,
        installer_url: String,
        checksums_url: Option<String>,
    },
    /// An AppImage replaces its own file, so nobody has to do anything.
    Replace {
        version: String,
        appimage_url: String,
        checksums_url: Option<String>,
        destination: PathBuf,
    },
}

pub fn plan(
    release: &Release,
    running: &str,
    appimage: Option<&Path>,
    windows: bool,
) -> Option<Plan> {
    if !newer_than(&release.version, running) {
        return None;
    }
    if let (Some(destination), Some(appimage_url)) = (appimage, release.appimage_url.clone()) {
        return Some(Plan::Replace {
            version: release.version.clone(),
            appimage_url,
            checksums_url: release.checksums_url.clone(),
            destination: destination.to_path_buf(),
        });
    }
    if windows {
        if let Some(installer_url) = release.installer_url.clone() {
            return Some(Plan::RunInstaller {
                version: release.version.clone(),
                installer_url,
                checksums_url: release.checksums_url.clone(),
            });
        }
    }
    Some(Plan::Open {
        version: release.version.clone(),
        url: release.page_url.clone(),
    })
}

/// The file the running AppImage was started from, which is the file to replace.
pub fn running_appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from)
}

/// Downloads the new AppImage, verifies it against the published checksums when there are any,
/// and renames it over the running copy. The running instance keeps its mounted image; the new
/// one starts on the next launch.
pub fn replace<H: HttpClient>(
    releases: &GithubReleases<H>,
    appimage_url: &str,
    checksums_url: Option<&str>,
    destination: &Path,
) -> Result<()> {
    let bytes = fetched_verified(releases, appimage_url, checksums_url)?;
    let staged = destination.with_extension("new");
    std::fs::write(&staged, &bytes)?;
    executable(&staged)?;
    std::fs::rename(&staged, destination)?;
    Ok(())
}

/// Downloads the installer, verified, into the app's own state folder, and answers with where
/// it landed so the caller can run it.
pub fn fetch_installer<H: HttpClient>(
    releases: &GithubReleases<H>,
    installer_url: &str,
    checksums_url: Option<&str>,
    into: &Path,
) -> Result<PathBuf> {
    let bytes = fetched_verified(releases, installer_url, checksums_url)?;
    let file_name = installer_url.rsplit('/').next().unwrap_or("setup.exe");
    let destination = into.join(file_name);
    std::fs::write(&destination, &bytes)?;
    Ok(destination)
}

fn fetched_verified<H: HttpClient>(
    releases: &GithubReleases<H>,
    url: &str,
    checksums_url: Option<&str>,
) -> Result<Vec<u8>> {
    let bytes = releases.fetch(url)?;
    if let Some(checksums_url) = checksums_url {
        let listing = String::from_utf8_lossy(&releases.fetch(checksums_url)?).into_owned();
        let file_name = url.rsplit('/').next().unwrap_or_default();
        let Some(recorded) = checksum_for(&listing, file_name) else {
            return Err(Error::Unreadable {
                what: "github releases".into(),
                detail: format!("checksums.txt does not name {file_name}"),
            });
        };
        if sha256_hex(&bytes) != recorded {
            return Err(Error::Unreadable {
                what: "github releases".into(),
                detail: format!("{file_name} does not match its recorded checksum"),
            });
        }
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamacine_core::http::{Request, Response};
    use std::sync::Mutex;

    struct Script(Mutex<Vec<Response>>);

    impl Script {
        fn answering(answers: Vec<(&str, u16)>) -> Self {
            Script(Mutex::new(
                answers
                    .into_iter()
                    .map(|(body, status)| Response {
                        status,
                        content_type: "application/octet-stream".into(),
                        body: body.as_bytes().to_vec(),
                    })
                    .collect(),
            ))
        }
    }

    impl HttpClient for Script {
        fn send(&self, _request: Request) -> mamacine_core::error::Result<Response> {
            Ok(self.0.lock().expect("not poisoned").remove(0))
        }
    }

    fn release() -> Release {
        Release {
            version: "0.2.0".into(),
            page_url: "https://github.com/fnune/mamacine/releases/tag/v0.2.0".into(),
            appimage_url: Some("https://example.test/MamaCine-x86_64.AppImage".into()),
            installer_url: Some("https://example.test/MamaCine-x64-setup.exe".into()),
            checksums_url: Some("https://example.test/checksums.txt".into()),
        }
    }

    #[test]
    fn each_platform_gets_the_update_the_way_it_can_take_it() {
        let appimage = PathBuf::from("/opt/MamaCine.AppImage");
        assert!(matches!(
            plan(&release(), "0.1.0", Some(&appimage), false),
            Some(Plan::Replace { version, .. }) if version == "0.2.0"
        ));
        assert!(matches!(
            plan(&release(), "0.1.0", None, true),
            Some(Plan::RunInstaller { .. })
        ));
        assert!(
            matches!(
                plan(&release(), "0.1.0", None, false),
                Some(Plan::Open { .. })
            ),
            "neither an AppImage nor Windows: the release page"
        );
        assert_eq!(plan(&release(), "0.2.0", Some(&appimage), false), None);
        assert_eq!(
            plan(&release(), "0.3.0", None, true),
            None,
            "never downgrade"
        );
    }

    #[test]
    fn the_new_copy_lands_verified_and_executable() {
        let directory = std::env::temp_dir().join("mama-cine-update-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let destination = directory.join("MamaCine-x86_64.AppImage");
        std::fs::write(&destination, b"the old version").expect("the old copy");

        let new_bytes = "the new version";
        let listing = format!(
            "{}  MamaCine-x86_64.AppImage\n",
            sha256_hex(new_bytes.as_bytes())
        );
        let releases = GithubReleases::new(
            "fnune/mamacine",
            Script::answering(vec![(new_bytes, 200), (&listing, 200)]),
        );
        replace(
            &releases,
            "https://example.test/MamaCine-x86_64.AppImage",
            Some("https://example.test/checksums.txt"),
            &destination,
        )
        .expect("replaced");

        assert_eq!(
            std::fs::read(&destination).expect("the file"),
            new_bytes.as_bytes()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "executable for everyone");
        }
    }

    #[test]
    fn a_download_that_fails_its_checksum_leaves_the_old_copy_alone() {
        let directory = std::env::temp_dir().join("mama-cine-update-badsum");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let destination = directory.join("MamaCine-x86_64.AppImage");
        std::fs::write(&destination, b"the old version").expect("the old copy");

        let releases = GithubReleases::new(
            "fnune/mamacine",
            Script::answering(vec![
                ("tampered bytes", 200),
                ("0000000000  MamaCine-x86_64.AppImage\n", 200),
            ]),
        );
        let outcome = replace(
            &releases,
            "https://example.test/MamaCine-x86_64.AppImage",
            Some("https://example.test/checksums.txt"),
            &destination,
        );
        assert!(outcome.is_err());
        assert_eq!(
            std::fs::read(&destination).expect("the file"),
            b"the old version"
        );
    }
}
