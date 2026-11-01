//! LAN send: Rewinds on the same network find each other over zeroconf and
//! send clips straight over TCP. No cloud, no account.
//!
//! Protocol, one clip per connection:
//! - sender: `REWIND1 {"id","name","file","size"}\n`
//! - receiver: `OK\n`, or `NO <reason>\n` (a first-time sender is asked about first)
//! - sender: `size` raw bytes, written in 20 MB chunks
//! - receiver: `DONE\n` once the file is on disk, or `ERR <reason>\n`

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use crossbeam_channel::{bounded, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::library;
use crate::settings::{Settings, TrustedPeer};

pub const SERVICE: &str = "_rewind._tcp.local";
pub const CHUNK: usize = 20 * 1024 * 1024;
const MAGIC: &str = "REWIND1 ";
/// Refuse anything bigger; a long 4K clip is a few GB.
const MAX_FILE: u64 = 16 * 1024 * 1024 * 1024;
/// How long a receiver has to answer "Accept clip from …?".
const ASK_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub id: String,
    pub name: String,
    pub file: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub addr: String,
}

/// File name a received clip may be saved under: the last path component,
/// only video/image types, no characters Windows rejects.
pub fn safe_file_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next()?.trim();
    let clean: String = base.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).collect();
    let clean = clean.trim_start_matches('.').trim().to_string();
    let ext = Path::new(&clean).extension()?.to_str()?.to_ascii_lowercase();
    let stem = Path::new(&clean).file_stem()?.to_str()?.trim();
    (["mp4", "gif", "png"].contains(&ext.as_str()) && !stem.is_empty() && clean.len() <= 200).then_some(clean)
}

fn read_line(r: &mut impl BufRead, max: usize) -> Result<String> {
    let mut line = Vec::new();
    r.take(max as u64).read_until(b'\n', &mut line)?;
    ensure!(line.ends_with(b"\n"), "connection closed");
    Ok(String::from_utf8_lossy(&line).trim_end().to_string())
}

/// Copy exactly `size` bytes in [`CHUNK`]s, reporting the running total.
fn copy_chunks(from: &mut impl Read, to: &mut impl Write, size: u64, progress: &mut dyn FnMut(u64)) -> Result<()> {
    let mut buf = vec![0u8; CHUNK.min(size.max(1) as usize)];
    let mut done = 0u64;
    while done < size {
        let want = buf.len().min((size - done) as usize);
        from.read_exact(&mut buf[..want]).context("connection lost")?;
        to.write_all(&buf[..want])?;
        done += want as u64;
        progress(done);
    }
    to.flush()?;
    Ok(())
}

/// Send one file over an open connection. `progress(sent, total)`.
pub fn send_over(conn: TcpStream, me: (&str, &str), path: &Path, progress: &mut dyn FnMut(u64, u64)) -> Result<()> {
    let size = std::fs::metadata(path)?.len();
    let hello = Hello {
        id: me.0.into(),
        name: me.1.into(),
        file: path.file_name().unwrap_or_default().to_string_lossy().into(),
        size,
    };
    let mut w = conn.try_clone()?;
    let mut r = BufReader::new(conn);
    w.write_all(format!("{MAGIC}{}\n", serde_json::to_string(&hello)?).as_bytes())?;
    // The receiver may be deciding whether to accept.
    r.get_ref().set_read_timeout(Some(Duration::from_secs(ASK_SECS + 15)))?;
    let answer = read_line(&mut r, 512)?;
    if let Some(why) = answer.strip_prefix("NO") {
        bail!("{}", why.trim().to_string().chars().take(200).collect::<String>());
    }
    ensure!(answer == "OK", "unexpected answer from the other PC");
    r.get_ref().set_read_timeout(Some(Duration::from_secs(120)))?;
    let mut file = std::fs::File::open(path)?;
    copy_chunks(&mut file, &mut w, size, &mut |n| progress(n, size))?;
    let done = read_line(&mut r, 512)?;
    if let Some(why) = done.strip_prefix("ERR") {
        bail!("the other PC couldn't save it: {}", why.trim());
    }
    ensure!(done == "DONE", "unexpected answer from the other PC");
    Ok(())
}

/// Receive one file into `dir`. `decide` sees the sender's hello and says
/// whether to take it. Returns the saved path and the hello.
pub fn receive_over(conn: TcpStream, dir: &Path, decide: &mut dyn FnMut(&Hello) -> Result<(), String>, progress: &mut dyn FnMut(&Hello, u64)) -> Result<(PathBuf, Hello)> {
    conn.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut w = conn.try_clone()?;
    let mut r = BufReader::new(conn);
    let line = read_line(&mut r, 4096)?;
    let hello: Hello = serde_json::from_str(line.strip_prefix(MAGIC).ok_or_else(|| anyhow!("not a Rewind"))?)?;
    let refuse = |w: &mut TcpStream, why: &str| -> Result<(PathBuf, Hello)> {
        let _ = w.write_all(format!("NO {why}\n").as_bytes());
        bail!("refused {}: {why}", hello.file)
    };
    let Some(name) = safe_file_name(&hello.file) else { return refuse(&mut w, "Only clips and screenshots can be sent") };
    if hello.size == 0 || hello.size > MAX_FILE {
        return refuse(&mut w, "That file is too large");
    }
    if let Err(why) = decide(&hello) {
        return refuse(&mut w, &why);
    }
    std::fs::create_dir_all(dir)?;
    let p = Path::new(&name);
    let dest = library::unique_path(dir, &p.file_stem().unwrap_or_default().to_string_lossy(), &p.extension().unwrap_or_default().to_string_lossy());
    let part = dest.with_extension("part");
    w.write_all(b"OK\n")?;
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&part)?;
        copy_chunks(&mut r, &mut file, hello.size, &mut |n| progress(&hello, n))?;
        drop(file);
        std::fs::rename(&part, &dest)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            w.write_all(b"DONE\n")?;
            Ok((dest, hello))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            let _ = w.write_all(format!("ERR {e:#}\n").as_bytes());
            Err(e)
        }
    }
}

/// Instance label for zeroconf: no dots (they'd split the name), at most 63 bytes.
fn instance_label(name: &str) -> String {
    let mut s: String = name.chars().map(|c| if c == '.' || c.is_control() { ' ' } else { c }).collect();
    while s.len() > 63 {
        s.pop();
    }
    if s.trim().is_empty() { "Rewind".into() } else { s.trim().to_string() }
}

pub fn device_name(s: &Settings) -> String {
    let n = s.sharing.device_name.trim();
    if n.is_empty() { std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Rewind PC".into()) } else { n.to_string() }
}

pub type Emit = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

struct Running {
    port: u16,
    name: String,
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    _reg: super::zeroconf::Registration,
    #[cfg(windows)]
    _browser: super::zeroconf::Browser,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Inner {
    engine: Engine,
    emit: Emit,
    running: Mutex<Option<Running>>,
    /// Offers waiting for "Accept clip from …?", by offer id.
    pending: Mutex<HashMap<u64, Sender<bool>>>,
    next_offer: AtomicU64,
}

#[derive(Clone)]
pub struct Lan {
    inner: Arc<Inner>,
}

impl Lan {
    pub fn new(engine: Engine, emit: Emit) -> Self {
        let lan = Lan { inner: Arc::new(Inner { engine, emit, running: Mutex::new(None), pending: Mutex::new(HashMap::new()), next_offer: AtomicU64::new(1) }) };
        lan.apply(&lan.inner.engine.settings());
        lan
    }

    /// Listening and announcing right now.
    pub fn listening(&self) -> bool {
        self.inner.running.lock().is_some()
    }

    fn notice(&self, level: &str, message: &str) {
        (self.inner.emit)("notice", serde_json::json!({ "level": level, "message": message }));
    }

    /// Start, stop or restart listening and announcing to match the settings.
    pub fn apply(&self, s: &Settings) {
        let mut running = self.inner.running.lock();
        let name = device_name(s);
        let want = s.sharing.lan_enabled.then_some((s.sharing.lan_port, name));
        if want.as_ref() == running.as_ref().map(|r| (r.port, r.name.clone())).as_ref() {
            return;
        }
        *running = None;
        let Some((port, name)) = want else { return };
        match self.start(port, &name, &s.sharing.device_id) {
            Ok(r) => {
                tracing::info!("LAN send listening on {port} as {name}");
                *running = Some(r);
            }
            Err(e) => self.notice("error", &format!("LAN send couldn't start: {e:#}")),
        }
    }

    fn start(&self, port: u16, name: &str, id: &str) -> Result<Running> {
        let listener = TcpListener::bind(("0.0.0.0", port)).with_context(|| format!("port {port} is in use. Pick another in Settings > Sharing."))?;
        listener.set_nonblocking(true)?;
        #[cfg(windows)]
        let reg = super::zeroconf::register(&instance_label(name), SERVICE, port, &[("id", id), ("name", name), ("v", "1")])?;
        #[cfg(not(windows))]
        let _ = id;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let lan = self.clone();
        std::thread::Builder::new().name("rewind-lan".into()).spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((conn, from)) => {
                        let lan = lan.clone();
                        std::thread::spawn(move || lan.handle(conn, from));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })?;
        Ok(Running {
            port,
            name: name.to_string(),
            stop,
            #[cfg(windows)]
            _reg: reg,
            #[cfg(windows)]
            _browser: super::zeroconf::browse(SERVICE),
        })
    }

    fn handle(&self, conn: TcpStream, from: SocketAddr) {
        let _ = conn.set_nonblocking(false);
        let dir = self.inner.engine.clips_dir_path();
        let mut decide = |h: &Hello| self.decide(h);
        let mut last = 0u64;
        let mut progress = |h: &Hello, n: u64| {
            // About once per chunk, so the UI isn't flooded.
            if n == h.size || n - last >= CHUNK as u64 {
                last = n;
                (self.inner.emit)("lan-progress", serde_json::json!({ "direction": "in", "peer": h.name, "file": h.file, "done": n, "total": h.size }));
            }
        };
        match receive_over(conn, &dir, &mut decide, &mut progress) {
            Ok((path, hello)) => {
                #[cfg(windows)]
                if path.extension().is_some_and(|e| e == "mp4") {
                    crate::platform::com_init();
                    if let Ok((w, h, px)) = crate::export::mf::frame_at(&path, 1.0, 640) {
                        let _ = std::fs::create_dir_all(library::thumbs_dir(&dir));
                        let _ = crate::platform::write_image(&library::thumb_path(&path), w, h, &px, crate::platform::ImageFormat::Jpeg);
                    }
                }
                (self.inner.emit)("library-changed", serde_json::Value::Null);
                self.notice("success", &format!("Received \"{}\" from {}", path.file_stem().unwrap_or_default().to_string_lossy(), hello.name));
            }
            Err(e) => tracing::info!("LAN receive from {from}: {e:#}"),
        }
    }

    /// Trusted senders go straight through; anyone else is asked about first.
    fn decide(&self, h: &Hello) -> Result<(), String> {
        if self.inner.engine.settings().sharing.trusted_peers.iter().any(|p| p.id == h.id) {
            self.notice("info", &format!("Receiving \"{}\" from {}…", h.file, h.name));
            return Ok(());
        }
        let offer = self.inner.next_offer.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = bounded(1);
        self.inner.pending.lock().insert(offer, tx);
        (self.inner.emit)("lan-offer", serde_json::json!({ "offer": offer, "name": h.name, "file": h.file, "size": h.size }));
        let accepted = rx.recv_timeout(Duration::from_secs(ASK_SECS)).unwrap_or(false);
        self.inner.pending.lock().remove(&offer);
        if !accepted {
            return Err("They declined".into());
        }
        let mut s = self.inner.engine.settings();
        s.sharing.trusted_peers.retain(|p| p.id != h.id);
        s.sharing.trusted_peers.push(TrustedPeer { id: h.id.clone(), name: h.name.clone() });
        match self.inner.engine.set_settings(s.clone()) {
            Ok(()) => (self.inner.emit)("settings-changed", serde_json::to_value(&s).unwrap_or_default()),
            Err(e) => tracing::warn!("save trusted peer: {e:#}"),
        }
        Ok(())
    }

    pub fn answer(&self, offer: u64, accept: bool) {
        if let Some(tx) = self.inner.pending.lock().remove(&offer) {
            let _ = tx.send(accept);
        }
    }

    /// Other Rewinds announcing themselves, while LAN send is on here.
    #[cfg(windows)]
    pub fn peers(&self) -> Vec<Peer> {
        if self.inner.running.lock().is_none() {
            return Vec::new();
        }
        let me = self.inner.engine.settings().sharing.device_id;
        let mut peers: Vec<Peer> = super::zeroconf::found()
            .into_iter()
            .filter_map(|(instance, f)| {
                let id = f.txt.get("id")?.clone();
                let name = f.txt.get("name").cloned().unwrap_or_else(|| instance.split('.').next().unwrap_or_default().to_string());
                (id != me).then(|| Peer { id, name, addr: SocketAddr::from((f.addr, f.port)).to_string() })
            })
            .collect();
        peers.sort_by_key(|p| p.name.to_lowercase());
        peers.dedup_by(|a, b| a.id == b.id);
        peers
    }

    #[cfg(not(windows))]
    pub fn peers(&self) -> Vec<Peer> {
        Vec::new()
    }

    /// Send a clip to the peer with `peer_id`; blocks until it's saved there.
    pub fn send(&self, path: &Path, peer_id: &str) -> Result<()> {
        let peer = self.peers().into_iter().find(|p| p.id == peer_id).ok_or_else(|| anyhow!("That PC isn't on the network any more"))?;
        let s = self.inner.engine.settings();
        let addr: SocketAddr = peer.addr.parse()?;
        let conn = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).with_context(|| format!("couldn't reach {}", peer.name))?;
        let file = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let emit = self.inner.emit.clone();
        let mut last = 0u64;
        send_over(conn, (&s.sharing.device_id, &device_name(&s)), path, &mut |n, total| {
            if n == total || n - last >= CHUNK as u64 {
                last = n;
                emit("lan-progress", serde_json::json!({ "direction": "out", "peer": peer.name, "file": file, "path": path, "done": n, "total": total }));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_sanitized() {
        assert_eq!(safe_file_name("Valorant 2026.mp4").as_deref(), Some("Valorant 2026.mp4"));
        assert_eq!(safe_file_name("..\\..\\Windows\\evil.mp4").as_deref(), Some("evil.mp4"));
        assert_eq!(safe_file_name("a:b?.gif").as_deref(), Some("ab.gif"));
        assert_eq!(safe_file_name("run.exe"), None);
        assert_eq!(safe_file_name(".mp4"), None);
        assert_eq!(instance_label("my.pc"), "my pc");
    }

    fn pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (b, _) = l.accept().unwrap();
        (a, b)
    }

    #[test]
    fn send_and_receive_loopback() {
        let root = std::env::temp_dir().join(format!("rewind-lan-{}", std::process::id()));
        let out = root.join("in");
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("clip.mp4");
        let data: Vec<u8> = (0..(CHUNK + 1234)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&src, &data).unwrap();

        let (a, b) = pair();
        let dir = out.clone();
        let rx = std::thread::spawn(move || receive_over(b, &dir, &mut |h| if h.name == "Desk" { Ok(()) } else { Err("no".into()) }, &mut |_, _| {}));
        let mut seen = Vec::new();
        send_over(a, ("id1", "Desk"), &src, &mut |n, t| seen.push((n, t))).unwrap();
        let (path, hello) = rx.join().unwrap().unwrap();
        assert_eq!(hello.size, data.len() as u64);
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen.last().unwrap().0, data.len() as u64);

        // Declined: nothing written, the sender hears why.
        let (a, b) = pair();
        let dir = out.clone();
        let rx = std::thread::spawn(move || receive_over(b, &dir, &mut |_| Err("They declined".into()), &mut |_, _| {}));
        let e = send_over(a, ("id2", "Laptop"), &src, &mut |_, _| {}).unwrap_err();
        assert_eq!(e.to_string(), "They declined");
        assert!(rx.join().unwrap().is_err());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }
}
