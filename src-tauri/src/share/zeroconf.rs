//! Zeroconf (mDNS / DNS-SD) through Windows' own responder (`DnsServiceRegister`
//! / `DnsServiceBrowse`, Windows 10 1809+), so nothing else binds port 5353.
//!
//! The Windows callbacks run on system threads and may still fire after a
//! cancel, so everything they touch is global and owned here.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::NetworkManagement::Dns::*;

const DNS_REQUEST_PENDING: u32 = 9506;
/// Re-ask the network this often, so peers that left without a goodbye age out.
const REBROWSE: Duration = Duration::from_secs(15);

/// A service instance found on the network.
#[derive(Debug, Clone)]
pub struct Found {
    pub addr: Ipv4Addr,
    pub port: u16,
    pub txt: HashMap<String, String>,
    pub seen: Instant,
}

static FOUND: Mutex<Option<HashMap<String, Found>>> = Mutex::new(None);
/// Instance names the browse callback saw, for the browse thread to resolve.
static TO_RESOLVE: Mutex<Option<Sender<String>>> = Mutex::new(None);
/// Query names and cancel handles of resolves in flight; Windows reads them
/// until the resolve completes. Replaced, not freed, when the next one starts.
type InFlight = HashMap<String, (Box<[u16]>, Box<Cancel>)>;
static RESOLVING: Mutex<Option<InFlight>> = Mutex::new(None);

/// A cancel handle. Windows writes it once; nothing here reads it across threads.
struct Cancel(DNS_SERVICE_CANCEL);
unsafe impl Send for Cancel {}

fn wide(s: &str) -> Box<[u16]> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn text(p: PWSTR) -> String {
    if p.is_null() { String::new() } else { p.to_string().unwrap_or_default() }
}

unsafe extern "system" fn registered(_status: u32, _ctx: *const core::ffi::c_void, inst: *const DNS_SERVICE_INSTANCE) {
    if !inst.is_null() {
        DnsServiceFreeInstance(inst);
    }
}

/// An advertised service. Dropping it sends the goodbye.
pub struct Registration {
    req: DNS_SERVICE_REGISTER_REQUEST,
}

// The request only points at the instance Windows allocated for it.
unsafe impl Send for Registration {}

impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            DnsServiceDeRegister(&self.req, None);
        }
        // The instance stays allocated: deregistering finishes asynchronously
        // and may still read it. It's a few bytes per toggle.
    }
}

/// Advertise `<instance>.<service>` on `port` with TXT `props`.
pub fn register(instance: &str, service: &str, port: u16, props: &[(&str, &str)]) -> Result<Registration> {
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "rewind".into());
    let keys: Vec<HSTRING> = props.iter().map(|(k, _)| HSTRING::from(*k)).collect();
    let values: Vec<HSTRING> = props.iter().map(|(_, v)| HSTRING::from(*v)).collect();
    let kp: Vec<PCWSTR> = keys.iter().map(|k| PCWSTR(k.as_ptr())).collect();
    let vp: Vec<PCWSTR> = values.iter().map(|v| PCWSTR(v.as_ptr())).collect();
    unsafe {
        let inst = DnsServiceConstructInstance(
            &HSTRING::from(format!("{instance}.{service}")),
            &HSTRING::from(format!("{host}.local")),
            None,
            None,
            port,
            0,
            0,
            props.len() as u32,
            kp.as_ptr(),
            vp.as_ptr(),
        );
        if inst.is_null() {
            bail!("couldn't build the zeroconf record");
        }
        let req = DNS_SERVICE_REGISTER_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1.0,
            InterfaceIndex: 0,
            pServiceInstance: inst,
            pRegisterCompletionCallback: Some(registered),
            pQueryContext: std::ptr::null_mut(),
            hCredentials: Default::default(),
            unicastEnabled: false.into(),
        };
        let r = DnsServiceRegister(&req, None);
        if r != DNS_REQUEST_PENDING {
            DnsServiceFreeInstance(inst);
            bail!("zeroconf register failed ({r}). Needs Windows 10 1809 or later.");
        }
        Ok(Registration { req })
    }
}

unsafe extern "system" fn browsed(status: u32, _ctx: *const core::ffi::c_void, rec: *const DNS_RECORDW) {
    if status == 0 {
        let mut r = rec;
        while !r.is_null() {
            if (*r).wType == DNS_TYPE_PTR.0 {
                let name = text((*r).Data.PTR.pNameHost);
                if (*r).dwTtl == 0 {
                    // Goodbye packet: the peer is going away.
                    if let Some(m) = FOUND.lock().as_mut() {
                        m.remove(&name);
                    }
                } else if let Some(tx) = TO_RESOLVE.lock().as_ref() {
                    let _ = tx.send(name);
                }
            }
            r = (*r).pNext;
        }
    }
    if !rec.is_null() {
        DnsFree(Some(rec.cast()), DnsFreeRecordList);
    }
}

unsafe extern "system" fn resolved(status: u32, _ctx: *const core::ffi::c_void, inst: *const DNS_SERVICE_INSTANCE) {
    if inst.is_null() {
        return;
    }
    let i = &*inst;
    if status == 0 && !i.ip4Address.is_null() {
        let name = text(i.pszInstanceName);
        let mut txt = HashMap::new();
        for k in 0..i.dwPropertyCount as usize {
            txt.insert(text(*i.keys.add(k)), text(*i.values.add(k)));
        }
        let found = Found { addr: Ipv4Addr::from((*i.ip4Address).to_ne_bytes()), port: i.wPort, txt, seen: Instant::now() };
        FOUND.lock().get_or_insert_with(HashMap::new).insert(name, found);
    }
    DnsServiceFreeInstance(inst);
}

fn resolve(name: &str) {
    let query = wide(name);
    let mut cancel = Box::new(Cancel(DNS_SERVICE_CANCEL::default()));
    let req = DNS_SERVICE_RESOLVE_REQUEST {
        Version: DNS_QUERY_REQUEST_VERSION1.0,
        InterfaceIndex: 0,
        QueryName: PWSTR(query.as_ptr() as *mut u16),
        pResolveCompletionCallback: Some(resolved),
        pQueryContext: std::ptr::null_mut(),
    };
    let r = unsafe { DnsServiceResolve(&req, &mut cancel.0) };
    if r as u32 == DNS_REQUEST_PENDING {
        RESOLVING.lock().get_or_insert_with(HashMap::new).insert(name.to_string(), (query, cancel));
    }
}

/// Browses for a service type until dropped.
pub struct Browser {
    stop: Arc<AtomicBool>,
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        *TO_RESOLVE.lock() = None;
        *FOUND.lock() = None;
    }
}

pub fn browse(service: &str) -> Browser {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx): (Sender<String>, Receiver<String>) = unbounded();
    *TO_RESOLVE.lock() = Some(tx);
    let query = wide(service);
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("rewind-zeroconf".into())
        .spawn(move || {
            let mut cancel = Box::new(DNS_SERVICE_CANCEL::default());
            let mut running = false;
            let mut last = Instant::now() - REBROWSE;
            let mut asked: HashMap<String, Instant> = HashMap::new();
            while !flag.load(Ordering::Relaxed) {
                if last.elapsed() >= REBROWSE {
                    last = Instant::now();
                    unsafe {
                        if running {
                            DnsServiceBrowseCancel(&*cancel);
                        }
                        let req = DNS_SERVICE_BROWSE_REQUEST {
                            Version: DNS_QUERY_REQUEST_VERSION1.0,
                            InterfaceIndex: 0,
                            QueryName: PCWSTR(query.as_ptr()),
                            Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 { pBrowseCallback: Some(browsed) },
                            pQueryContext: std::ptr::null_mut(),
                        };
                        running = DnsServiceBrowse(&req, &mut *cancel) as u32 == DNS_REQUEST_PENDING;
                    }
                    if let Some(m) = FOUND.lock().as_mut() {
                        m.retain(|_, f| f.seen.elapsed() < REBROWSE * 4);
                    }
                }
                if let Ok(name) = rx.recv_timeout(Duration::from_millis(250)) {
                    // Answers repeat; resolving each once per round is enough.
                    if asked.get(&name).is_none_or(|t| t.elapsed() >= REBROWSE) {
                        asked.insert(name.clone(), Instant::now());
                        resolve(&name);
                    }
                }
            }
            if running {
                unsafe {
                    DnsServiceBrowseCancel(&*cancel);
                }
            }
            // `cancel` and `query` are leaked on purpose: a late callback may still read them.
            std::mem::forget(cancel);
            std::mem::forget(query);
        })
        .expect("spawn zeroconf thread");
    Browser { stop }
}

/// Instances seen recently, by instance name.
pub fn found() -> Vec<(String, Found)> {
    FOUND.lock().as_ref().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Advertise and find ourselves through the Windows responder.
    /// `cargo test -- --ignored zeroconf_sees_itself`
    #[test]
    #[ignore]
    fn zeroconf_sees_itself() {
        let _reg = register("rewind-test", "_rewindtest._tcp.local", 7999, &[("id", "abc")]).unwrap();
        let _b = browse("_rewindtest._tcp.local");
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            if let Some((_, f)) = found().into_iter().find(|(_, f)| f.txt.get("id").map(String::as_str) == Some("abc")) {
                assert_eq!(f.port, 7999);
                println!("found at {}:{}", f.addr, f.port);
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("not found");
    }
}
