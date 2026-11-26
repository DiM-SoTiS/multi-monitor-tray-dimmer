//! GitHub Releases update checker and one-click self-updater using native WinINet.

use std::env;
use std::fs;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/DiM-SoTiS/multi-monitor-tray-dimmer/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available { version: String, download_url: String },
    Downloading,
    Error,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn parse_semver(tag: &str) -> Option<(u32, u32, u32)> {
    let clean = tag.trim().trim_start_matches(['v', 'V']);
    let mut parts = clean.split('.');
    let maj = parts.next()?.parse::<u32>().ok()?;
    let min = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let pat = parts
        .next()
        .unwrap_or("0")
        .split(['-', '+'])
        .next()?
        .parse::<u32>()
        .ok()?;
    Some((maj, min, pat))
}

pub fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// Extracts the first string value for `"key": "value"` in a JSON payload.
pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut search_from = 0usize;
    while let Some(pos) = json[search_from..].find(&needle) {
        let after_key = &json[search_from + pos + needle.len()..];
        let trimmed = after_key.trim_start();
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            let val_start = after_colon.trim_start();
            if let Some(rest) = val_start.strip_prefix('"') {
                if let Some(end_quote) = rest.find('"') {
                    return Some(rest[..end_quote].to_string());
                }
            }
        }
        search_from += pos + needle.len();
    }
    None
}

/// Finds the first `.exe` asset `browser_download_url` in GitHub's release JSON.
pub fn find_exe_download_url(json: &str) -> Option<String> {
    let needle = "\"browser_download_url\"";
    let mut pos = 0usize;
    while let Some(idx) = json[pos..].find(needle) {
        let start = pos + idx + needle.len();
        if let Some(url) = extract_json_string(&json[pos + idx..], "browser_download_url") {
            if url.to_ascii_lowercase().ends_with(".exe") {
                return Some(url);
            }
        }
        pos = start;
    }
    None
}

#[cfg(windows)]
mod wininet_http {
    use std::ffi::c_void;
    use std::ptr::null;

    type Hinternet = *mut c_void;

    const INTERNET_OPEN_TYPE_PRECONFIG: u32 = 0;
    const INTERNET_FLAG_RELOAD: u32 = 0x8000_0000;
    const INTERNET_FLAG_NO_CACHE_WRITE: u32 = 0x0400_0000;
    const INTERNET_FLAG_SECURE: u32 = 0x0080_0000;

    #[link(name = "wininet")]
    extern "system" {
        fn InternetOpenW(
            lpsz_agent: *const u16,
            dw_access_type: u32,
            lpsz_proxy: *const u16,
            lpsz_proxy_bypass: *const u16,
            dw_flags: u32,
        ) -> Hinternet;
        fn InternetOpenUrlW(
            h_internet: Hinternet,
            lpsz_url: *const u16,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            dw_flags: u32,
            dw_context: usize,
        ) -> Hinternet;
        fn InternetReadFile(
            h_file: Hinternet,
            lp_buffer: *mut u8,
            dw_number_of_bytes_to_read: u32,
            lpdw_number_of_bytes_read: *mut u32,
        ) -> i32;
        fn InternetCloseHandle(h_internet: Hinternet) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn http_get_bytes(url: &str, accept: Option<&str>) -> Result<Vec<u8>, String> {
        unsafe {
            let agent = to_wide("multi-monitor-tray-dimmer/1.1");
            let h_net = InternetOpenW(agent.as_ptr(), INTERNET_OPEN_TYPE_PRECONFIG, null(), null(), 0);
            if h_net.is_null() {
                return Err("InternetOpenW failed".to_string());
            }

            let url_w = to_wide(url);
            let headers_w = accept.map(to_wide);
            let (hdr_ptr, hdr_len) = match &headers_w {
                Some(h) => (h.as_ptr(), (h.len() - 1) as u32),
                None => (null(), 0),
            };
            let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_SECURE;
            let h_url = InternetOpenUrlW(h_net, url_w.as_ptr(), hdr_ptr, hdr_len, flags, 0);
            if h_url.is_null() {
                InternetCloseHandle(h_net);
                return Err("InternetOpenUrlW failed".to_string());
            }

            let mut out = Vec::new();
            let mut buf = [0u8; 16384];
            let mut read: u32 = 0;
            loop {
                let ok = InternetReadFile(h_url, buf.as_mut_ptr(), buf.len() as u32, &mut read);
                if ok == 0 {
                    InternetCloseHandle(h_url);
                    InternetCloseHandle(h_net);
                    return Err("InternetReadFile failed".to_string());
                }
                if read == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..read as usize]);
            }

            InternetCloseHandle(h_url);
            InternetCloseHandle(h_net);
            Ok(out)
        }
    }
}

pub fn check_latest_release() -> UpdateState {
    #[cfg(windows)]
    {
        let Ok(bytes) = wininet_http::http_get_bytes(
            LATEST_RELEASE_URL,
            Some("Accept: application/vnd.github+json\r\n"),
        ) else {
            return UpdateState::Error;
        };
        let Ok(json) = String::from_utf8(bytes) else {
            return UpdateState::Error;
        };
        let Some(tag) = extract_json_string(&json, "tag_name") else {
            return UpdateState::Error;
        };
        if !is_newer_version(current_version(), &tag) {
            return UpdateState::UpToDate;
        }
        let Some(url) = find_exe_download_url(&json) else {
            return UpdateState::Error;
        };
        UpdateState::Available {
            version: tag.trim_start_matches(['v', 'V']).to_string(),
            download_url: url,
        }
    }
    #[cfg(not(windows))]
    {
        UpdateState::Error
    }
}

pub fn download_and_spawn_replace(download_url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let bytes = wininet_http::http_get_bytes(download_url, None)?;
        if bytes.len() < 50_000 || !bytes.starts_with(b"MZ") {
            return Err("Invalid downloaded executable".to_string());
        }

        let temp_exe = env::temp_dir().join("multi-monitor-tray-dimmer-update.exe");
        fs::write(&temp_exe, &bytes).map_err(|e| e.to_string())?;

        let current_exe = env::current_exe().map_err(|e| e.to_string())?;
        let script = format!(
            "timeout /t 1 /nobreak >nul & move /y \"{}\" \"{}\" >nul & start \"\" \"{}\"",
            temp_exe.display(),
            current_exe.display(),
            current_exe.display()
        );
        Command::new("cmd.exe")
            .args(["/C", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = download_url;
        Err("Unsupported OS".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions() {
        assert!(is_newer_version("1.1.0", "v1.1.1"));
        assert!(is_newer_version("1.1.0", "v1.2.0"));
        assert!(!is_newer_version("1.1.0", "v1.1.0"));
        assert!(!is_newer_version("1.1.0", "v1.0.2"));
    }

    #[test]
    fn extracts_github_release_fields() {
        let sample = r#"{"tag_name":"v1.2.0","assets":[{"browser_download_url":"https://example.com/app.exe"}]}"#;
        assert_eq!(extract_json_string(sample, "tag_name").as_deref(), Some("v1.2.0"));
        assert_eq!(
            find_exe_download_url(sample).as_deref(),
            Some("https://example.com/app.exe")
        );
    }
}
