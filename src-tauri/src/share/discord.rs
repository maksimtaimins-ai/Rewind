//! Post a clip to a Discord channel webhook: one HTTPS multipart POST, no bot
//! and no token. The body is built here; the request goes through WinHTTP.

use std::path::Path;

use anyhow::{bail, ensure, Context, Result};

use crate::settings::DiscordWebhook;

/// Hosts Discord serves webhooks from. Anything else is refused, so a typo'd
/// or pasted URL can't send a clip somewhere unexpected.
const HOSTS: [&str; 4] = ["discord.com", "discordapp.com", "canary.discord.com", "ptb.discord.com"];

/// `(host, path)` of a webhook URL, or why it isn't one.
pub fn parse_url(url: &str) -> Result<(String, String)> {
    let rest = url.trim().strip_prefix("https://").context("A webhook URL starts with https://")?;
    let (host, path) = rest.split_once('/').context("That isn't a webhook URL")?;
    let host = host.to_ascii_lowercase();
    ensure!(HOSTS.contains(&host.as_str()), "Only discord.com webhook URLs are allowed");
    ensure!(path.starts_with("api/webhooks/") || path.contains("/webhooks/"), "That isn't a webhook URL");
    ensure!(!path.contains(char::is_whitespace), "That isn't a webhook URL");
    Ok((host, format!("/{path}")))
}

/// Largest file a webhook accepts. Decimal MB: Discord's cap is counted that
/// way, and it keeps a clip that's just under 10 MiB from being refused.
pub fn limit_bytes(w: &DiscordWebhook) -> u64 {
    w.limit_mb as u64 * 1_000_000
}

/// `multipart/form-data` body with the clip as `files[0]`.
pub fn multipart(boundary: &str, file_name: &str, bytes: &[u8]) -> Vec<u8> {
    // Quotes and line breaks would end the header early.
    let name: String = file_name.chars().map(|c| if c == '"' || c.is_control() { '_' } else { c }).collect();
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{{\"content\":\"\"}}\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{name}\"\r\nContent-Type: {}\r\n\r\n",
            content_type(file_name)
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn content_type(file_name: &str) -> &'static str {
    match Path::new(file_name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("gif") => "image/gif",
        Some("png") => "image/png",
        _ => "video/mp4",
    }
}

/// Upload `path` to the webhook. Fails without sending when the file is over
/// the webhook's cap.
#[cfg(windows)]
pub fn post(w: &DiscordWebhook, path: &Path) -> Result<()> {
    let (host, url_path) = parse_url(&w.url)?;
    let size = std::fs::metadata(path).with_context(|| format!("read {}", path.display()))?.len();
    if size > limit_bytes(w) {
        bail!("The clip is {:.1} MB, over {}'s {} MB limit", size as f64 / 1e6, w.name, w.limit_mb);
    }
    let bytes = std::fs::read(path)?;
    let boundary = format!("rewind{:016x}", crate::platform::qpc_now());
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let body = multipart(&boundary, &file_name, &bytes);
    let (status, reply) = super::http::post(&host, &url_path, &format!("multipart/form-data; boundary={boundary}"), &body)?;
    match status {
        200..=299 => Ok(()),
        401 | 404 => bail!("Discord says webhook \"{}\" no longer exists. Check its URL in Settings > Sharing.", w.name),
        413 => bail!("Discord refused the file as too large. Set \"{}\" to 10 MB, or export with a Discord preset.", w.name),
        429 => bail!("Discord is rate limiting this webhook. Try again in a minute."),
        _ => bail!("Discord answered {status}: {}", reply.chars().take(200).collect::<String>()),
    }
}

#[cfg(not(windows))]
pub fn post(_w: &DiscordWebhook, _path: &Path) -> Result<()> {
    bail!("Posting to Discord needs Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_checked() {
        let (h, p) = parse_url("https://discord.com/api/webhooks/123/abc").unwrap();
        assert_eq!((h.as_str(), p.as_str()), ("discord.com", "/api/webhooks/123/abc"));
        assert!(parse_url("https://ptb.discord.com/api/v10/webhooks/1/x").is_ok());
        assert!(parse_url("http://discord.com/api/webhooks/1/x").is_err());
        assert!(parse_url("https://discord.com.evil.io/api/webhooks/1/x").is_err());
        assert!(parse_url("https://discord.com/channels/1/2").is_err());
    }

    #[test]
    fn multipart_shape() {
        let body = multipart("XYZ", "a\"b.mp4", b"DATA");
        let text = String::from_utf8(body).unwrap();
        assert!(text.starts_with("--XYZ\r\n"));
        assert!(text.contains("name=\"files[0]\"; filename=\"a_b.mp4\"\r\nContent-Type: video/mp4\r\n\r\nDATA\r\n--XYZ--\r\n"));
        assert!(text.contains("name=\"payload_json\""));
    }

    #[test]
    fn limit_is_decimal() {
        let w = DiscordWebhook { name: "x".into(), url: String::new(), limit_mb: 10 };
        assert_eq!(limit_bytes(&w), 10_000_000);
    }
}
