//! Minimal HTTPS POST over WinHTTP, so Rewind needs no HTTP or TLS crate.
//! Proxies follow the system settings.

use anyhow::{bail, Result};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn open(h: *mut core::ffi::c_void, what: &str) -> Result<Handle> {
    if h.is_null() {
        bail!("{what}: {}", windows::core::Error::from_thread());
    }
    Ok(Handle(h))
}

/// POST `body` to `https://<host><path>`. Returns the status code and the
/// response text.
pub fn post(host: &str, path: &str, content_type: &str, body: &[u8]) -> Result<(u16, String)> {
    unsafe {
        let session = open(WinHttpOpen(w!("Rewind"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "open HTTP session")?;
        // Resolve, connect, send, receive (ms). Sending 25 MB on a slow uplink takes a while.
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 300_000, 60_000)?;
        let connect = open(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0), "connect")?;
        let request = open(
            WinHttpOpenRequest(connect.0, w!("POST"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "open request",
        )?;
        let headers: Vec<u16> = format!("Content-Type: {content_type}\r\n").encode_utf16().collect();
        let len = u32::try_from(body.len())?;
        WinHttpSendRequest(request.0, Some(&headers), Some(body.as_ptr().cast()), len, len, 0)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )?;
        let mut reply = Vec::new();
        let mut chunk = [0u8; 8192];
        while reply.len() < 64 * 1024 {
            let mut read = 0u32;
            WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut read)?;
            if read == 0 {
                break;
            }
            reply.extend_from_slice(&chunk[..read as usize]);
        }
        Ok((status as u16, String::from_utf8_lossy(&reply).into_owned()))
    }
}
