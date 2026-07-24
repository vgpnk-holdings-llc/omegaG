//! Check for updates via the GitHub Releases API.
//!
//! Platform flows:
//!   * Windows (`win` module): download `DS4CC-Setup.exe`, prompt, run installer.
//!   * Linux (`linux` module): pick the release asset containing "linux" AND
//!     "x86_64", download the tarball, extract with the system `tar`, chmod +x,
//!     and atomically rename over the running exe — then notify via the tray.

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const API_URL: &str = "https://api.github.com/repos/VeigaPunk/DS4CC/releases/latest";

/// Entry point — called from the tray via `std::thread::spawn`.
pub fn check_for_update() {
    #[cfg(windows)]
    win::check_for_update();
    #[cfg(target_os = "linux")]
    linux::check_for_update();
}

/// Returns true if `remote` is newer than `current` (semver comparison).
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };

    match (parse(remote), parse(current)) {
        (Some(r), Some(c)) => r > c,
        _ => false,
    }
}

/// Pick the Linux x86_64 asset from a GitHub release JSON document.
///
/// Matches any asset whose name contains "linux" AND "x86_64"
/// (case-insensitive) — expected: `ds4cc-linux-x86_64.tar.gz`.
/// Pure and shared with the test suite on every OS.
#[cfg(any(target_os = "linux", test))]
fn find_linux_asset(json: &serde_json::Value) -> Option<(String, String)> {
    json["assets"].as_array()?.iter().find_map(|asset| {
        let name = asset["name"].as_str()?;
        let lower = name.to_ascii_lowercase();
        if lower.contains("linux") && lower.contains("x86_64") {
            let url = asset["browser_download_url"].as_str()?;
            Some((name.to_string(), url.to_string()))
        } else {
            None
        }
    })
}

// ── Windows flow (installer download + prompt) ─────────────────────────

/// Downloads and runs the installer if a newer version is available.
#[cfg(windows)]
mod win {
    use super::{API_URL, CURRENT_VERSION, is_newer};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_YESNO, MessageBoxW,
    };

    const INSTALLER_NAME: &str = "DS4CC-Setup.exe";

    pub fn check_for_update() {
        match check_inner() {
            Ok(()) => {}
            Err(e) => {
                log::error!("Update check failed: {e}");
                show_msg(
                    "Could not check for updates.\nCheck your internet connection.",
                    "Update Check Failed",
                    MB_ICONWARNING,
                );
            }
        }
    }

    fn check_inner() -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Checking for updates...");

        let body: String = ureq::get(API_URL)
            .header("User-Agent", "DS4CC")
            .header("Accept", "application/vnd.github+json")
            .call()?
            .body_mut()
            .read_to_string()?;

        let json: serde_json::Value = serde_json::from_str(&body)?;

        let tag = json["tag_name"].as_str().ok_or("missing tag_name")?;
        let remote_version = tag.strip_prefix('v').unwrap_or(tag);

        if !is_newer(remote_version, CURRENT_VERSION) {
            log::info!("Already on latest version (v{CURRENT_VERSION})");
            show_msg(
                &format!("You're on the latest version (v{CURRENT_VERSION})."),
                "DS4CC Update",
                MB_ICONINFORMATION,
            );
            return Ok(());
        }

        // Find installer download URL
        let download_url = json["assets"]
            .as_array()
            .and_then(|assets| {
                assets.iter().find_map(|a| {
                    let name = a["name"].as_str()?;
                    if name == INSTALLER_NAME {
                        a["browser_download_url"].as_str().map(String::from)
                    } else {
                        None
                    }
                })
            })
            .ok_or("installer asset not found in release")?;

        log::info!("New version available: v{remote_version} (current: v{CURRENT_VERSION})");

        let msg = format!(
            "Version v{remote_version} is available (you have v{CURRENT_VERSION}).\n\nDownload and install?"
        );

        if !ask_yes_no(&msg, "DS4CC Update Available") {
            return Ok(());
        }

        // Download installer to %TEMP%
        let temp = std::env::temp_dir().join(INSTALLER_NAME);
        log::info!("Downloading installer to {}", temp.display());

        let bytes = ureq::get(&download_url)
            .header("User-Agent", "DS4CC")
            .call()?
            .body_mut()
            .with_config()
            .limit(50 * 1024 * 1024)
            .read_to_vec()?;

        std::fs::write(&temp, &bytes)?;
        log::info!("Installer downloaded ({} bytes)", bytes.len());

        // Run installer and exit
        log::info!("Launching installer...");
        std::process::Command::new(&temp).spawn()?;
        std::process::exit(0);
    }

    fn show_msg(text: &str, caption: &str, flags: u32) {
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let cap_w: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text_w.as_ptr(), cap_w.as_ptr(), flags);
        }
    }

    fn ask_yes_no(text: &str, caption: &str) -> bool {
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let cap_w: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
        let result = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text_w.as_ptr(),
                cap_w.as_ptr(),
                MB_YESNO | MB_ICONINFORMATION,
            )
        };
        result == IDYES
    }
}

// ── Linux flow (tarball → atomic replace of current_exe) ───────────────

#[cfg(target_os = "linux")]
mod linux {
    use super::{API_URL, CURRENT_VERSION, find_linux_asset, is_newer};
    use std::path::{Path, PathBuf};

    pub fn check_for_update() {
        if let Err(e) = check_inner() {
            log::error!("Update check failed: {e}");
            crate::tray::notify(
                "DS4CC Update",
                "Could not check for updates. Check your internet connection.",
            );
        }
    }

    fn check_inner() -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Checking for updates...");

        let body: String = ureq::get(API_URL)
            .header("User-Agent", "DS4CC")
            .header("Accept", "application/vnd.github+json")
            .call()?
            .body_mut()
            .read_to_string()?;

        let json: serde_json::Value = serde_json::from_str(&body)?;

        let tag = json["tag_name"].as_str().ok_or("missing tag_name")?;
        let remote_version = tag.strip_prefix('v').unwrap_or(tag);

        if !is_newer(remote_version, CURRENT_VERSION) {
            log::info!("Already on latest version (v{CURRENT_VERSION})");
            crate::tray::notify(
                "DS4CC Update",
                &format!("You're on the latest version (v{CURRENT_VERSION})."),
            );
            return Ok(());
        }

        let Some((asset_name, download_url)) = find_linux_asset(&json) else {
            // Graceful: the release exists but ships no Linux build.
            log::info!("Release v{remote_version} has no Linux x86_64 asset — no update available");
            crate::tray::notify(
                "DS4CC Update",
                &format!("v{remote_version} is available but has no Linux build yet."),
            );
            return Ok(());
        };

        log::info!(
            "New version available: v{remote_version} (current: v{CURRENT_VERSION}); downloading {asset_name}"
        );

        let bytes = ureq::get(&download_url)
            .header("User-Agent", "DS4CC")
            .call()?
            .body_mut()
            .with_config()
            .limit(100 * 1024 * 1024)
            .read_to_vec()?;

        install_from_tarball(&bytes)?;
        log::info!("Updated to v{remote_version} — restart required");
        crate::tray::notify(
            "DS4CC Update",
            &format!("Updated to v{remote_version}. Use tray → Restart to apply."),
        );
        Ok(())
    }

    /// Extract the tarball with the system `tar`, chmod +x the binary, and
    /// atomically rename it over the currently running executable.
    ///
    /// The new binary is staged next to `current_exe` first so the final
    /// rename is on the same filesystem (temp dirs may be tmpfs).
    /// Linux permits renaming over a running binary.
    fn install_from_tarball(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let exe = std::env::current_exe()?;
        let work = std::env::temp_dir().join(format!("ds4cc-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work)?;

        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let archive = work.join("release.tar.gz");
            std::fs::write(&archive, bytes)?;
            log::info!("Downloaded {} bytes; extracting", bytes.len());

            let status = std::process::Command::new("tar")
                .arg("xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&work)
                .status()?;
            if !status.success() {
                return Err(format!("tar xzf failed (exit {status})").into());
            }

            let new_bin = find_extracted_binary(&work, &archive)?;
            log::info!("Extracted binary: {}", new_bin.display());

            // Stage beside the exe, then rename over it atomically.
            let staged = exe.with_extension("new");
            std::fs::copy(&new_bin, &staged)?;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
            std::fs::rename(&staged, &exe)?;
            Ok(())
        })();

        let _ = std::fs::remove_dir_all(&work);
        result
    }

    /// Locate the ds4cc binary inside the extracted tarball.
    ///
    /// Prefers a file literally named `ds4cc`; otherwise falls back to the
    /// first regular file that isn't the archive itself.
    fn find_extracted_binary(
        dir: &Path,
        archive: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut fallback: Option<PathBuf> = None;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() && path != archive {
                    if path.file_name().is_some_and(|n| n == "ds4cc") {
                        return Ok(path);
                    }
                    if fallback.is_none() {
                        fallback = Some(path);
                    }
                }
            }
        }
        fallback.ok_or_else(|| "no binary found in extracted release tarball".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_newer() {
        assert!(is_newer("2.7.0", "2.6.0"));
        assert!(is_newer("3.0.0", "2.6.0"));
        assert!(is_newer("2.6.1", "2.6.0"));
    }

    #[test]
    fn version_same_or_older() {
        assert!(!is_newer("2.6.0", "2.6.0"));
        assert!(!is_newer("2.5.0", "2.6.0"));
        assert!(!is_newer("1.0.0", "2.6.0"));
    }

    // ── Linux asset matcher (SPEC §10: fake release JSON) ────────────────

    fn fake_release(asset_names: &[&str]) -> serde_json::Value {
        let assets: Vec<serde_json::Value> = asset_names
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "browser_download_url": format!("https://example.com/dl/{name}"),
                })
            })
            .collect();
        serde_json::json!({ "tag_name": "v9.9.9", "assets": assets })
    }

    #[test]
    fn linux_asset_matcher_picks_linux_x86_64() {
        let json = fake_release(&[
            "DS4CC-Setup.exe",
            "ds4cc-linux-aarch64.tar.gz",
            "ds4cc-linux-x86_64.tar.gz",
        ]);
        let (name, url) = find_linux_asset(&json).expect("must find linux x86_64 asset");
        assert_eq!(name, "ds4cc-linux-x86_64.tar.gz");
        assert_eq!(url, "https://example.com/dl/ds4cc-linux-x86_64.tar.gz");
    }

    #[test]
    fn linux_asset_matcher_case_insensitive() {
        let json = fake_release(&["DS4CC-Linux-X86_64.TAR.GZ"]);
        assert!(find_linux_asset(&json).is_some());
    }

    #[test]
    fn linux_asset_matcher_no_linux_asset_is_graceful() {
        // Windows-only release → None → "no update available" path.
        let json = fake_release(&["DS4CC-Setup.exe", "ds4cc-windows-x86_64.zip"]);
        assert!(find_linux_asset(&json).is_none());
    }

    #[test]
    fn linux_asset_matcher_rejects_wrong_arch() {
        // Linux but ARM — must not match x86_64 builds.
        let json = fake_release(&["ds4cc-linux-aarch64.tar.gz"]);
        assert!(find_linux_asset(&json).is_none());
    }

    #[test]
    fn linux_asset_matcher_empty_assets() {
        let json = serde_json::json!({ "tag_name": "v9.9.9", "assets": [] });
        assert!(find_linux_asset(&json).is_none());
        let json = serde_json::json!({ "tag_name": "v9.9.9" });
        assert!(find_linux_asset(&json).is_none());
    }
}
