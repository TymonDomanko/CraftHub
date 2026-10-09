//! Talking to GitHub: latest releases, downloads and checksums.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use ureq::ResponseExt;

/// API and web base URLs. Overridable for testing against a local mock server.
pub fn api_base() -> String {
    std::env::var("CRAFTHUB_API").unwrap_or_else(|_| "https://api.github.com".into())
}
pub fn web_base() -> String {
    std::env::var("CRAFTHUB_WEB").unwrap_or_else(|_| "https://github.com".into())
}

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .user_agent(format!("CraftHub/{}", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build()
            .into()
    })
}

/// Name tokens that identify this machine's CPU in release asset names.
pub fn arch_tokens() -> &'static [&'static str] {
    match std::env::consts::ARCH {
        "x86_64" => &["x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        other => {
            static OTHER: OnceLock<[&'static str; 1]> = OnceLock::new();
            OTHER.get_or_init(|| [Box::leak(other.to_string().into_boxed_str())])
        }
    }
}

pub fn arch_name() -> &'static str {
    arch_tokens()[0]
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Debug, Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// A release's tag and its downloadable assets (name, URL).
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<(String, String)>,
    /// True when assets were guessed from the naming scheme (API rate-limited).
    pub guessed: bool,
}

impl Release {
    pub fn asset_url(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The latest release of `owner/repo`. Uses the API, falling back to the web
/// redirect when the API is rate-limited (60 requests/hour without a token).
/// `Ok(None)` means the repo has no releases.
/// Run a request, retrying once after a connection problem (not after an HTTP status).
fn with_retry<T>(mut f: impl FnMut() -> Result<T, ureq::Error>) -> Result<T, ureq::Error> {
    match f() {
        Err(e) if !matches!(e, ureq::Error::StatusCode(_)) => {
            std::thread::sleep(Duration::from_millis(1500));
            f()
        }
        other => other,
    }
}

pub fn latest_release(owner: &str, repo: &str) -> Result<Option<Release>, String> {
    let url = format!("{}/repos/{owner}/{repo}/releases/latest", api_base());
    match with_retry(|| {
        agent()
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .call()
    }) {
        Ok(mut resp) => {
            let text = resp.body_mut().read_to_string().map_err(err)?;
            let rel: ApiRelease = serde_json::from_str(&text).map_err(err)?;
            Ok(Some(Release {
                tag: rel.tag_name,
                assets: rel
                    .assets
                    .into_iter()
                    .map(|a| (a.name, a.browser_download_url))
                    .collect(),
                guessed: false,
            }))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(ureq::Error::StatusCode(403 | 429)) => latest_tag_from_web(owner, repo).map(|t| {
            t.map(|tag| Release {
                tag,
                assets: Vec::new(),
                guessed: true,
            })
        }),
        Err(e) => Err(err(e)),
    }
}

fn latest_tag_from_web(owner: &str, repo: &str) -> Result<Option<String>, String> {
    let url = format!("{}/{owner}/{repo}/releases/latest", web_base());
    let resp = with_retry(|| agent().head(&url).call()).map_err(err)?;
    Ok(tag_from_release_url(&resp.get_uri().to_string()))
}

pub fn tag_from_release_url(url: &str) -> Option<String> {
    let rest = &url[url.find("/releases/tag/")? + "/releases/tag/".len()..];
    let tag: String = rest
        .chars()
        .take_while(|c| !matches!(c, '/' | '?' | '#'))
        .collect();
    (!tag.is_empty()).then_some(tag)
}

/// Pick the AppImage for this machine from a list of asset names.
pub fn pick_appimage<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    names.into_iter().find(|n| {
        let low = n.to_lowercase();
        n.ends_with(".AppImage") && arch_tokens().iter().any(|t| low.contains(t))
    })
}

/// Download `url` to `dest`, reporting (done, total) bytes. Returns the SHA-256 hex digest.
pub fn download(url: &str, dest: &Path, progress: &dyn Fn(u64, u64)) -> Result<String, String> {
    let mut resp = agent().get(url).call().map_err(err)?;
    let total = resp.body().content_length().unwrap_or(0);
    let mut reader = resp.body_mut().as_reader();
    let mut file = File::create(dest).map_err(err)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        let n = reader.read(&mut buf).map_err(err)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(err)?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if last.elapsed() > Duration::from_millis(100) {
            last = Instant::now();
            progress(done, total);
        }
    }
    file.flush().map_err(err)?;
    progress(done, if total == 0 { done } else { total });
    Ok(hex::encode(hasher.finalize()))
}

/// Download to a temporary file next to `dest`, check it against `SHA256SUMS.txt` (when
/// the release has one) and move it into place. Returns whether it was verified.
pub fn download_verified(
    url: &str,
    asset_name: &str,
    sums_url: Option<&str>,
    dest: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<bool, String> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(err)?;
    }
    let tmp = dest.with_extension("part");
    let digest = match download(url, &tmp, progress) {
        Ok(d) => d,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let want = sums_url.and_then(|u| expected_sha(u, asset_name));
    if let Some(want) = &want
        && !want.eq_ignore_ascii_case(&digest)
    {
        let _ = fs::remove_file(&tmp);
        return Err("Checksum mismatch; download discarded".into());
    }
    set_executable(&tmp)?;
    fs::rename(&tmp, dest).map_err(err)?;
    Ok(want.is_some())
}

pub fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(err)
}

/// The expected SHA-256 for `asset` from a `SHA256SUMS.txt` file, if listed.
pub fn expected_sha(sums_url: &str, asset: &str) -> Option<String> {
    let text = agent()
        .get(sums_url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    parse_sums(&text, asset)
}

pub fn parse_sums(text: &str, asset: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.last()?.trim_start_matches('*');
        (name == asset && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// `v1.2.3`, `artcraft-v0.32.0`, `1.2` → (1, 2, 3) / (0, 32, 0) / (1, 2, 0).
pub fn parse_version(s: &str) -> (u64, u64, u64) {
    let mut nums = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u64>().unwrap_or(0));
    (
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
    )
}

/// Display form of a tag: `artcraft-v0.32.0` → `0.32.0`.
pub fn pretty_version(tag: &str) -> String {
    match tag.find(|c: char| c.is_ascii_digit()) {
        Some(i) => tag[i..].to_string(),
        None => tag.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), (1, 2, 3));
        assert_eq!(parse_version("artcraft-v0.32.0"), (0, 32, 0));
        assert_eq!(parse_version("2.0"), (2, 0, 0));
        assert!(parse_version("v2.0.1") > parse_version("v2.0.0"));
        assert_eq!(pretty_version("artcraft-v0.32.0"), "0.32.0");
        assert_eq!(pretty_version("v0.5.0"), "0.5.0");
    }

    #[test]
    fn tags_from_urls() {
        assert_eq!(
            tag_from_release_url("https://github.com/a/b/releases/tag/v0.5.0").as_deref(),
            Some("v0.5.0")
        );
        assert_eq!(
            tag_from_release_url("https://github.com/a/b/releases"),
            None
        );
    }

    #[test]
    fn sums() {
        let h = "a".repeat(64);
        let text = format!(
            "{h}  photocraft-0.5.0-linux-x86_64.AppImage\n{}  other\n",
            "b".repeat(64)
        );
        assert_eq!(
            parse_sums(&text, "photocraft-0.5.0-linux-x86_64.AppImage"),
            Some(h)
        );
        assert_eq!(parse_sums(&text, "missing"), None);
    }

    #[test]
    fn appimage_choice() {
        let names = [
            "photocraft-0.5.0-linux-aarch64.AppImage",
            "photocraft-0.5.0-linux-x86_64.AppImage",
            "photocraft-0.5.0-linux-x86_64.AppImage.zsync",
            "photocraft-0.5.0-linux-x86_64.deb",
        ];
        let pick = pick_appimage(names.iter().copied()).unwrap();
        assert!(pick.ends_with(".AppImage") && pick.contains(arch_name()));
    }
}
