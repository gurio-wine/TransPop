//! Self-update from GitHub Releases.
//!
//! The release asset is trusted only when GitHub reports a SHA-256 digest for
//! it; the downloaded bytes are hashed locally and must match. Windows keeps a
//! running executable locked against writes but allows it to be renamed, so an
//! update renames the current executable aside, installs the verified copy at
//! the same path and starts it. The new process waits for this one to exit
//! before it takes the single-instance guard.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

// GitHub's web redirect carries the tag and the release page publishes the
// asset digest, so no API endpoint (and no API rate limit) is involved.
const REPO: &str = "https://github.com/gurio-wine/TransPop";
const RELEASES_LATEST: &str = "https://github.com/gurio-wine/TransPop/releases/latest";
const ASSET_NAME: &str = "transpop.exe";
const MAX_PAGE_BODY: u64 = 262_144;
// A release executable is about 2 MiB; refuse anything unreasonably larger.
const MAX_DOWNLOAD: u64 = 32 * 1024 * 1024;
const USER_AGENT: &str = concat!("TransPop/", env!("CARGO_PKG_VERSION"));
// GitHub reverse proxies, used only when github.com itself is unreachable.
// A digest coming from a mirror is trusted only when two independent mirrors
// publish the same one, so a single hostile proxy cannot push code.
const MIRRORS: [&str; 3] =
    ["https://ghfast.top", "https://gh.llkk.cc", "https://ghproxy.net"];

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub sha256: String,
    /// Where the digest came from ("github" or the mirror that agreed first).
    pub source: String,
    /// github.com itself, when it answered during the check.
    pub direct_url: Option<String>,
    /// Reverse proxies that published the same digest, in probe order.
    pub mirror_urls: Vec<String>,
}

fn transport_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format!("HTTP_{status}"),
        // Never log raw transport strings: some contain user-configured URLs.
        ureq::Error::Transport(error) => format!("TRANSPORT_{:?}", error.kind()),
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(timeout)
        .build()
}

/// The tag GitHub redirects /releases/latest to.
fn tag_from_url(url: &str) -> Option<String> {
    let tail = url.split("/releases/tag/").nth(1)?;
    let tag = tail.split(['?', '#']).next()?.trim_matches('/');
    (!tag.is_empty() && tag.starts_with('v')).then(|| tag.to_owned())
}

/// The release page lists the asset link and the digest GitHub computed for
/// it; both must be present or the download is not trusted.
fn digest_from_assets(html: &str, tag: &str) -> Option<String> {
    let link = html.find(&format!("/releases/download/{tag}/{ASSET_NAME}"))?;
    let start = link + html.get(link..)?.find("sha256:")? + "sha256:".len();
    let digest = html.get(start..start + 64)?;
    digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        .then(|| digest.to_ascii_lowercase())
}

/// Some mirrors answer with the page instead of a redirect, so the tag can
/// also come from the body. GitHub's URL templates embed the literal
/// "/releases/tag/*name", whose star is not a tag character.
fn tag_from_body(html: &str) -> Option<String> {
    // The page also embeds URL templates such as
    // "/:user_id/:repository/releases/tag/*name", so scan for the first match
    // that actually looks like a tag instead of trusting the first occurrence.
    for (index, _) in html.match_indices("/releases/tag/") {
        let tag: String = html.get(index + "/releases/tag/".len()..)?
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect();
        if tag.starts_with('v') && tag.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
            return Some(tag);
        }
    }
    None
}

/// The asset URL on github.com, or behind a reverse proxy that takes the full
/// GitHub URL as its path.
fn asset_url(base: Option<&str>, tag: &str) -> String {
    let url = format!("{REPO}/releases/download/{tag}/{ASSET_NAME}");
    match base {
        Some(mirror) => format!("{mirror}/{url}"),
        None => url,
    }
}

/// The newest published release that carries a verifiable transpop.exe.
/// Reads the newest tag and its published digest from github.com (None) or
/// through one reverse proxy; a mirror is only usable if it serves both pages.
fn check_via(base: Option<&str>) -> Result<(String, String), String> {
    let through = |url: &str| match base {
        Some(mirror) => format!("{mirror}/{url}"),
        None => url.to_string(),
    };
    let response = agent(Duration::from_secs(20))
        .get(&through(RELEASES_LATEST))
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(transport_error)?;
    let tag = match tag_from_url(response.get_url()) {
        Some(tag) => tag,
        None => {
            let mut body = String::new();
            response.into_reader().take(MAX_PAGE_BODY).read_to_string(&mut body)
                .map_err(|_| "response read error".to_string())?;
            tag_from_body(&body).ok_or_else(|| "release page has no tag".to_string())?
        }
    };
    let mut body = String::new();
    agent(Duration::from_secs(20))
        .get(&through(&format!("{REPO}/releases/expanded_assets/{tag}")))
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(transport_error)?
        .into_reader().take(MAX_PAGE_BODY).read_to_string(&mut body)
        .map_err(|_| "response read error".to_string())?;
    let sha256 = digest_from_assets(&body, &tag)
        .ok_or_else(|| "release asset has no trusted digest".to_string())?;
    Ok((tag, sha256))
}

/// Finds the first tag/digest pair published by at least two mirrors.
fn pick_agreement(
    results: &[(&'static str, String, String)],
) -> Option<(String, String, Vec<&'static str>)> {
    for (tag, sha256) in results.iter().map(|(_, tag, sha256)| (tag, sha256)) {
        let agreeing: Vec<&'static str> = results.iter()
            .filter(|(_, t, s)| t == tag && s == sha256)
            .map(|(mirror, _, _)| *mirror)
            .collect();
        if agreeing.len() >= 2 {
            return Some((tag.clone(), sha256.clone(), agreeing));
        }
    }
    None
}

/// The newest published release that carries a verifiable transpop.exe.
///
/// github.com is asked first and its digest is the authority. When it cannot be
/// reached (common in mainland China) two independent reverse proxies must
/// publish the same tag and digest instead, so one hostile mirror cannot push
/// its own executable. That check doubles as a source list for the download.
pub fn latest() -> Result<Release, String> {
    // Escape hatch for networks where every github.com host is blocked: point
    // the updater at one trusted reverse proxy and skip the agreement check.
    if let Ok(override_url) = std::env::var("TRANSPOP_UPDATE_MIRROR") {
        let override_url = override_url.trim().trim_end_matches('/');
        if override_url.starts_with("https://") {
            let (tag, sha256) = check_via(Some(override_url))?;
            return Ok(Release {
                direct_url: None,
                mirror_urls: vec![asset_url(Some(override_url), &tag)],
                source: override_url.to_string(),
                tag,
                sha256,
            });
        }
    }
    if let Ok((tag, sha256)) = check_via(None) {
        return Ok(Release {
            mirror_urls: MIRRORS.iter().map(|mirror| asset_url(Some(mirror), &tag)).collect(),
            direct_url: Some(asset_url(None, &tag)),
            source: "github".to_string(),
            tag,
            sha256,
        });
    }
    let mut results: Vec<(&'static str, String, String)> = Vec::new();
    for mirror in MIRRORS {
        if let Ok((tag, sha256)) = check_via(Some(mirror)) {
            results.push((mirror, tag, sha256));
        }
    }
    match pick_agreement(&results) {
        Some((tag, sha256, mirrors)) => Ok(Release {
            direct_url: None,
            mirror_urls: mirrors.iter().map(|mirror| asset_url(Some(mirror), &tag)).collect(),
            source: mirrors[0].to_string(),
            tag,
            sha256,
        }),
        None => Err("github.com unreachable and no two mirrors agree on the release".to_string()),
    }
}


fn parse_version(text: &str) -> Option<[u64; 3]> {
    let core = text.trim().trim_start_matches('v')
        .split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut version = [0u64; 3];
    for (slot, part) in version.iter_mut().zip(parts.by_ref()) {
        *slot = part.parse().ok()?;
    }
    Some(version)
}

/// Remote tags are accepted only when they sort strictly above the local build.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_version(current), parse_version(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

/// Streams one URL into memory, reporting progress as it goes.
fn fetch_into(url: &str, bytes: &mut Vec<u8>, progress: &mut dyn FnMut(u32)) -> Result<(), String> {
    let response = agent(Duration::from_secs(120))
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(transport_error)?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    bytes.clear();
    let mut reader = response.into_reader();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|_| "download read error".to_string())?;
        if read == 0 { break; }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() as u64 > MAX_DOWNLOAD { return Err("download too large".to_string()); }
        let percent = if total == 0 { 0 } else {
            (bytes.len() as u64 * 100 / total).min(100) as u32
        };
        progress(percent);
    }
    Ok(())
}

/// Download the release executable and verify its digest, trying each source
/// that published this release in turn (github.com first, then the mirrors).
/// `progress` receives a percentage after every chunk.
pub fn download(
    release: &Release,
    dir: &Path,
    progress: &mut dyn FnMut(u32),
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("create update dir: {e}"))?;
    let safe_tag: String = release.tag.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let path = dir.join(format!("transpop-{safe_tag}.exe"));
    let mut sources: Vec<&str> = Vec::new();
    if let Some(direct) = release.direct_url.as_deref() { sources.push(direct); }
    sources.extend(release.mirror_urls.iter().map(String::as_str));
    let mut bytes = Vec::new();
    let mut last_error = "download failed".to_string();
    for url in sources {
        match fetch_into(url, &mut bytes, progress) {
            Ok(()) => {
                if sha256_hex(&bytes).eq_ignore_ascii_case(&release.sha256) {
                    fs::write(&path, &bytes).map_err(|e| format!("write update file: {e}"))?;
                    progress(100);
                    return Ok(path);
                }
                // A mirror may serve a stale file; the digest is the only trust
                // anchor, so move on to the next source instead of installing it.
                last_error = "UPDATE_DIGEST_MISMATCH".to_string();
            }
            Err(reason) => last_error = reason,
        }
        progress(0);
    }
    Err(last_error)
}

fn backup_path(current: &Path) -> PathBuf {
    let mut name = current.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// Move the verified executable into place and start it. The caller exits.
pub fn apply(new_exe: &Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    let backup = backup_path(&current);
    // A leftover backup is unlocked once its process is gone.
    let _ = fs::remove_file(&backup);
    fs::rename(&current, &backup).map_err(|e| format!("rename current exe: {e}"))?;
    if let Err(error) = fs::copy(new_exe, &current) {
        let _ = fs::rename(&backup, &current);
        return Err(format!("install new exe: {error}"));
    }
    // The replacement must wait for this instance to release the
    // single-instance window and mutex.
    if let Err(error) = std::process::Command::new(&current).arg("--after-update").spawn() {
        let _ = fs::remove_file(&current);
        let _ = fs::rename(&backup, &current);
        return Err(format!("restart: {error}"));
    }
    Ok(())
}

/// The previous executable left by a self-update, if any.
pub fn stale_backup() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let backup = backup_path(&current);
    backup.exists().then_some(backup)
}

/// Drop the previous executable once the replacement proved healthy.
pub fn remove_backup(path: &Path) {
    let _ = fs::remove_file(path);
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 { message.push(0); }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            schedule[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = schedule[i - 15].rotate_right(7)
                ^ schedule[i - 15].rotate_right(18) ^ (schedule[i - 15] >> 3);
            let s1 = schedule[i - 2].rotate_right(17)
                ^ schedule[i - 2].rotate_right(19) ^ (schedule[i - 2] >> 10);
            schedule[i] = schedule[i - 16].wrapping_add(s0)
                .wrapping_add(schedule[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let t1 = h.wrapping_add(s1).wrapping_add(choice)
                .wrapping_add(SHA256_K[i]).wrapping_add(schedule[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            h = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        digest[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_published_vectors() {
        assert_eq!(sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn tag_and_digest_are_read_from_the_release_pages() {
        assert_eq!(tag_from_url("https://github.com/o/r/releases/tag/v1.2.3").as_deref(),
            Some("v1.2.3"));
        assert_eq!(tag_from_url("https://github.com/o/r/releases/tag/"), None);
        assert_eq!(tag_from_url("https://github.com/o/r/releases"), None);
        let digest = "a".repeat(64);
        let html = format!(
            "<a href=\"/o/r/releases/download/v1.2.3/transpop.exe\">x</a> sha256:{digest}",
        );
        assert_eq!(digest_from_assets(&html, "v1.2.3"), Some(digest.clone()));
        // Wrong tag or missing digest must not be trusted.
        assert_eq!(digest_from_assets(&html, "v9.9.9"), None);
        assert_eq!(digest_from_assets("sha256:nothex", "v1.2.3"), None);
    }

    #[test]
    fn a_lone_mirror_digest_is_never_trusted() {
        let results = vec![
            ("https://a", "v1.0.0".to_string(), "aa".to_string()),
            ("https://b", "v1.0.0".to_string(), "aa".to_string()),
            ("https://c", "v1.0.0".to_string(), "bb".to_string()),
        ];
        let (tag, sha256, mirrors) = pick_agreement(&results).expect("two mirrors agree");
        assert_eq!((tag.as_str(), sha256.as_str(), mirrors), ("v1.0.0", "aa", vec!["https://a", "https://b"]));
        assert!(pick_agreement(&results[..1]).is_none());
        assert!(pick_agreement(&[]).is_none());
    }

    #[test]
    fn mirror_pages_yield_a_tag_but_url_templates_do_not() {
        assert_eq!(tag_from_body("<a href=\"/o/r/releases/tag/v1.2.3\">x</a>").as_deref(),
            Some("v1.2.3"));
        assert_eq!(
            tag_from_body("content=\"/:user_id/:repository/releases/tag/*name\" \
                <a href=\"/o/r/releases/tag/v1.2.3\">x</a>").as_deref(),
            Some("v1.2.3"),
        );
        assert_eq!(tag_from_body("content=\"/:user_id/:repository/releases/tag/*name\""), None);
        assert_eq!(tag_from_body("/releases/tag/latest"), None);
        assert_eq!(
            asset_url(Some("https://ghfast.top"), "v1.2.3"),
            "https://ghfast.top/https://github.com/gurio-wine/TransPop/releases/download/v1.2.3/transpop.exe",
        );
    }

    #[test]
    fn only_strictly_newer_tags_are_accepted() {
        assert!(is_newer("0.6.8", "v0.6.9"));
        assert!(is_newer("0.6.9", "v0.7"));
        assert!(is_newer("0.6.9", "v0.7.0"));
        assert!(!is_newer("0.6.9", "v0.6.9"));
        assert!(!is_newer("0.6.9", "v0.6.8"));
        assert!(!is_newer("0.6.9", "latest"));
    }
}
