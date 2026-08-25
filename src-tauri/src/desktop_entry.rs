use std::path::{Path, PathBuf};

/// Where the AppImage lives once adopted: a home no cleanup habit empties.
pub fn stable_home(data_home: &Path) -> PathBuf {
    data_home.join("mamacine").join("Mamá Cine.AppImage")
}

/// A download folder is a place things get deleted from, and the menu entry would die with the
/// file. Launched from anywhere else, the AppImage copies itself into its stable home, and that
/// copy is what the menu points at. The copy she launched wins: if it differs from the adopted
/// one, it replaces it, and the updater sorts out versions within the day.
///
/// Answers with the stable path and whether anything was copied.
pub fn adopt(appimage: &Path, data_home: &Path) -> std::io::Result<(PathBuf, bool)> {
    let home = stable_home(data_home);
    if appimage == home {
        return Ok((home, false));
    }
    if same_bytes(appimage, &home) {
        return Ok((home, false));
    }
    std::fs::create_dir_all(home.parent().expect("the home has a parent"))?;
    let staged = home.with_extension("new");
    std::fs::copy(appimage, &staged)?;
    executable(&staged)?;
    std::fs::rename(&staged, &home)?;
    Ok((home, true))
}

fn same_bytes(one: &Path, other: &Path) -> bool {
    let sizes = (
        one.metadata().map(|data| data.len()),
        other.metadata().map(|data| data.len()),
    );
    match sizes {
        (Ok(here), Ok(there)) if here == there => {}
        _ => return false,
    }
    match (std::fs::read(one), std::fs::read(other)) {
        (Ok(here), Ok(there)) => digest(&here) == digest(&there),
        _ => false,
    }
}

fn digest(bytes: &[u8]) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .to_vec()
}

#[cfg(unix)]
fn executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// An AppImage is invisible to the desktop's menus until somebody says otherwise, and nobody
/// less technical ever says otherwise. The AppImage carries the desktop entry Tauri wrote; this
/// installs it, pointed at the AppImage itself, so the app appears in the menu after the first
/// launch and stays reachable however the file was double-clicked.
///
/// Answers whether anything changed, so the caller can log it once rather than daily.
pub fn install(appdir: &Path, appimage: &Path, data_home: &Path) -> std::io::Result<bool> {
    let Some(template) = bundled_entry(appdir) else {
        return Ok(false);
    };
    let icon = data_home.join("icons").join("mamacine.png");
    let entry = entry_for(&template, appimage, &icon);

    let target = data_home
        .join("applications")
        .join("com.fnune.mamacine.desktop");
    if std::fs::read_to_string(&target).ok().as_deref() == Some(entry.as_str()) {
        return Ok(false);
    }

    if let Some(source) = bundled_icon(appdir) {
        std::fs::create_dir_all(icon.parent().expect("icons has a parent"))?;
        std::fs::copy(source, &icon)?;
    }
    std::fs::create_dir_all(target.parent().expect("applications has a parent"))?;
    std::fs::write(&target, entry)?;
    Ok(true)
}

/// The desktop file Tauri packed into the AppDir, wherever it filed it.
fn bundled_entry(appdir: &Path) -> Option<String> {
    let mut places = vec![appdir.to_path_buf()];
    places.push(appdir.join("usr/share/applications"));
    for place in places {
        let Ok(entries) = std::fs::read_dir(place) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|kind| kind == "desktop") {
                return std::fs::read_to_string(path).ok();
            }
        }
    }
    None
}

/// The biggest icon the AppDir carries, or its `.DirIcon`.
fn bundled_icon(appdir: &Path) -> Option<std::path::PathBuf> {
    let hicolor = appdir.join("usr/share/icons/hicolor");
    let mut best: Option<(u64, std::path::PathBuf)> = None;
    if let Ok(sizes) = std::fs::read_dir(hicolor) {
        for size in sizes.flatten() {
            let apps = size.path().join("apps");
            let Ok(icons) = std::fs::read_dir(apps) else {
                continue;
            };
            for icon in icons.flatten() {
                let bytes = icon.metadata().map(|data| data.len()).unwrap_or(0);
                if best.as_ref().map(|(kept, _)| bytes > *kept).unwrap_or(true) {
                    best = Some((bytes, icon.path()));
                }
            }
        }
    }
    best.map(|(_, path)| path)
        .or_else(|| Some(appdir.join(".DirIcon")).filter(|icon| icon.exists()))
}

/// The bundled entry, retargeted: `Exec` names the AppImage itself (quoted, because her name in
/// the path has a space), `Icon` names an installed file outright, and an empty `Categories`
/// becomes the film shelf it belongs on.
fn entry_for(template: &str, appimage: &Path, icon: &Path) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in template.lines() {
        if line.starts_with("Exec=") {
            lines.push(format!("Exec=\"{}\"", appimage.display()));
        } else if line.starts_with("Icon=") {
            lines.push(format!("Icon={}", icon.display()));
        } else if line.trim() == "Categories=" {
            lines.push("Categories=AudioVideo;Video;".to_string());
        } else {
            lines.push(line.to_string());
        }
    }
    let mut entry = lines.join("\n");
    entry.push('\n');
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        directory
    }

    fn appdir_in(directory: &Path) -> PathBuf {
        let appdir = directory.join("Mamá Cine.AppDir");
        let apps = appdir.join("usr/share/icons/hicolor/128x128/apps");
        std::fs::create_dir_all(&apps).expect("icon folders");
        std::fs::write(apps.join("mamacine.png"), b"a big icon and then some").expect("an icon");
        let small = appdir.join("usr/share/icons/hicolor/32x32/apps");
        std::fs::create_dir_all(&small).expect("icon folders");
        std::fs::write(small.join("mamacine.png"), b"small").expect("an icon");
        std::fs::write(
            appdir.join("Mamá Cine.desktop"),
            "[Desktop Entry]\nCategories=\nComment=Mamá Cine\nExec=mamacine\n\
             StartupWMClass=mamacine\nIcon=mamacine\nName=Mamá Cine\nTerminal=false\n\
             Type=Application\n",
        )
        .expect("the bundled entry");
        appdir
    }

    #[test]
    fn the_menu_learns_where_the_appimage_lives() {
        let directory = scratch("mama-cine-desktop-entry");
        let appdir = appdir_in(&directory);
        let appimage = directory.join("Aplicaciones/Mamá Cine.AppImage");
        let data_home = directory.join("data");

        assert!(install(&appdir, &appimage, &data_home).expect("installed"));

        let entry =
            std::fs::read_to_string(data_home.join("applications/com.fnune.mamacine.desktop"))
                .expect("the entry");
        assert!(
            entry.contains(&format!("Exec=\"{}\"", appimage.display())),
            "{entry}"
        );
        assert!(
            entry.contains(&format!(
                "Icon={}",
                data_home.join("icons/mamacine.png").display()
            )),
            "{entry}"
        );
        assert!(entry.contains("Categories=AudioVideo;Video;"), "{entry}");
        assert!(entry.contains("StartupWMClass=mamacine"), "{entry}");
        assert_eq!(
            std::fs::read(data_home.join("icons/mamacine.png")).expect("the icon"),
            b"a big icon and then some",
            "the biggest icon wins"
        );
    }

    #[test]
    fn an_entry_already_in_place_is_left_alone_and_a_moved_appimage_is_followed() {
        let directory = scratch("mama-cine-desktop-entry-again");
        let appdir = appdir_in(&directory);
        let appimage = directory.join("Mamá Cine.AppImage");
        let data_home = directory.join("data");

        assert!(install(&appdir, &appimage, &data_home).expect("installed"));
        assert!(
            !install(&appdir, &appimage, &data_home).expect("looked"),
            "nothing to do twice"
        );

        let moved = directory.join("Escritorio/Mamá Cine.AppImage");
        assert!(install(&appdir, &moved, &data_home).expect("updated"));
        let entry =
            std::fs::read_to_string(data_home.join("applications/com.fnune.mamacine.desktop"))
                .expect("the entry");
        assert!(entry.contains("Escritorio"), "{entry}");
    }

    // Downloads folders get emptied; the adopted copy is the one the menu can trust. The copy
    // she launched wins, and an unchanged relaunch from the download copies nothing.
    #[test]
    fn a_launched_appimage_settles_into_a_home_no_cleanup_empties() {
        let directory = scratch("mama-cine-adopt");
        let downloaded = directory.join("Descargas/Mamá Cine.AppImage");
        std::fs::create_dir_all(downloaded.parent().expect("a parent")).expect("folders");
        std::fs::write(&downloaded, b"version one").expect("the download");
        let data_home = directory.join("data");

        let (home, adopted) = adopt(&downloaded, &data_home).expect("adopted");
        assert!(adopted);
        assert_eq!(home, stable_home(&data_home));
        assert_eq!(std::fs::read(&home).expect("the copy"), b"version one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&home)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "executable for everyone");
        }

        assert!(
            !adopt(&downloaded, &data_home).expect("looked").1,
            "an unchanged relaunch copies nothing"
        );
        assert!(
            !adopt(&home, &data_home).expect("looked").1,
            "launched from its home, there is nothing to do"
        );

        std::fs::write(&downloaded, b"version two").expect("a newer download");
        let (_, readopted) = adopt(&downloaded, &data_home).expect("readopted");
        assert!(readopted, "the copy she launched wins");
        assert_eq!(std::fs::read(&home).expect("the copy"), b"version two");
    }

    #[test]
    fn an_appdir_with_no_entry_installs_nothing() {
        let directory = scratch("mama-cine-desktop-entry-none");
        let appimage = directory.join("Mamá Cine.AppImage");
        assert!(!install(&directory, &appimage, &directory.join("data")).expect("looked"));
    }
}
