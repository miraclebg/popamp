//! `popamp_http`: a plug-in reaches only the addresses its manifest lists
//! and the user left ticked, and every request goes in its journal. A port
//! that is not the usual one, and this machine or the user's network, must
//! be written out in the manifest and are off until the user ticks them.
//! Redirects are not followed: an address a server names is not one the
//! user was shown.

use crate::journal::Journal;
use crate::manifest::{check_host_pattern, host_matches};
use popamp_plugin_api::{HttpRequest, HttpResponse};
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// A longer answer is an error: it has to fit in the plug-in's memory.
const MAX_BODY: u64 = 8 << 20;
const METHODS: [&str; 6] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];

/// An address of a manifest's `net`: `host`, `*.domain` or either with `:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetEntry {
    host: String,
    port: Option<u16>,
}

impl NetEntry {
    pub fn parse(text: &str) -> Result<NetEntry, String> {
        let (host, port) = match text.rsplit_once(':') {
            Some((host, port)) => (host, Some(port.parse().ok().filter(|p| *p > 0).ok_or_else(|| format!("{text:?} has a bad port"))?)),
            None => (text, None),
        };
        check_host_pattern(host)?;
        Ok(NetEntry { host: host.to_string(), port })
    }

    /// This machine or the user's network, written out by name.
    fn is_local(&self) -> bool {
        let name = self.host.strip_prefix("*.").unwrap_or(&self.host);
        name == "localhost" || name.ends_with(".localhost") || name.parse().is_ok_and(|ip| !is_public(ip))
    }

    /// Why the user has to say yes first; `None` for an ordinary address.
    pub fn caution(&self) -> Option<&'static str> {
        match self.port {
            _ if self.is_local() => Some("this computer or your network"),
            Some(port) if port != 80 && port != 443 => Some("unusual port"),
            _ => None,
        }
    }

    fn takes(&self, to: &Target) -> bool {
        host_matches(std::slice::from_ref(&self.host), &to.host) && self.port.map_or(to.usual_port, |p| p == to.port)
    }
}

/// Where a URL leads.
struct Target {
    host: String,
    port: u16,
    usual_port: bool,
}

impl Target {
    fn of(url: &str) -> Result<Target, String> {
        let (scheme, rest) = url.split_once("://").ok_or(BAD_SCHEME)?;
        let usual = usual_port(scheme).ok_or(BAD_SCHEME)?;
        if url.chars().any(|c| c.is_control() || c.is_whitespace() || c == '\\') {
            return Err("has a character an address may not have".into());
        }
        let (host, port) = authority(rest, usual)?;
        Ok(Target { host, port, usual_port: port == usual })
    }

    fn shown(&self) -> String {
        if self.usual_port {
            format!("host {}", self.host)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

const BAD_SCHEME: &str = "must start with https:// or http://";

fn usual_port(scheme: &str) -> Option<u16> {
    match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    }
}

/// The lowercase host and the port a URL's text after `://` starts with.
fn authority(rest: &str, usual: u16) -> Result<(String, u16), String> {
    let text = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().map_err(|_| "has a bad port".to_string())?),
        None => (text.as_str(), usual),
    };
    match !host.is_empty() && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') {
        true => Ok((host.to_string(), port)),
        false => Err("must name a plain host".into()),
    }
}

/// Sends one request to a local address or a public one.
type Send = Box<dyn Fn(&HttpRequest, Duration, bool) -> Result<HttpResponse, String> + std::marker::Send + Sync>;

pub struct Network {
    entries: Vec<NetEntry>,
    journal: Arc<Journal>,
    send: Send,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Network({:?})", self.entries)
    }
}

impl Network {
    /// `addresses` are the manifest's that the user left open.
    pub fn new(addresses: &[String], proxy: &str, journal: Arc<Journal>) -> Network {
        let proxy = proxy.trim().to_string();
        Network { entries: entries(addresses), journal, send: Box::new(move |req, time, local| send(req, &proxy, time, local)) }
    }

    /// Answers a plug-in's request; what went wrong is in the answer.
    pub fn fetch(&self, req: HttpRequest, time: Duration) -> HttpResponse {
        let answered = self.allow(&req).and_then(|local| (self.send)(&req, time, local));
        answered.unwrap_or_else(|error| HttpResponse { error: Some(error), ..Default::default() })
    }

    /// Whether the address is a local one, if it is allowed at all.
    fn allow(&self, req: &HttpRequest) -> Result<bool, String> {
        let asked = format!("{} {} ({} bytes)", req.method, req.url, req.body.as_ref().map_or(0, String::len));
        let checked = check(req, &self.entries);
        match &checked {
            Ok(_) => self.journal.record("net", &asked, "allowed"),
            Err(why) => self.journal.record("net", &format!("{asked} ({why})"), "refused"),
        }
        checked
    }
}

fn entries(addresses: &[String]) -> Vec<NetEntry> {
    addresses.iter().filter_map(|a| NetEntry::parse(a).ok()).collect()
}

fn check(req: &HttpRequest, entries: &[NetEntry]) -> Result<bool, String> {
    if !METHODS.contains(&req.method.as_str()) {
        return Err(format!("method {:?} is not allowed", req.method));
    }
    if req.headers.keys().any(|k| k.eq_ignore_ascii_case("host")) {
        return Err("the Host header is the player's to set".into());
    }
    let to = Target::of(&req.url)?;
    entries.iter().find(|e| e.takes(&to)).map(NetEntry::is_local).ok_or_else(|| format!("{} is not allowed", to.shown()))
}

/// What is not the open internet: networks as address and prefix length.
const LOCAL_V4: [(u32, u32); 14] = [
    (0x0000_0000, 8),  // "this network"
    (0x0a00_0000, 8),  // private
    (0x6440_0000, 10), // carrier-grade NAT
    (0x7f00_0000, 8),  // this machine
    (0xa9fe_0000, 16), // link-local, cloud metadata
    (0xac10_0000, 12), // private
    (0xc000_0000, 24), // protocol assignments
    (0xc000_0200, 24), // documentation
    (0xc0a8_0000, 16), // private
    (0xc612_0000, 15), // benchmarking
    (0xc633_6400, 24), // documentation
    (0xcb00_7100, 24), // documentation
    (0xe000_0000, 4),  // multicast
    (0xf000_0000, 4),  // reserved, broadcast
];
const LOCAL_V6: [(u128, u32); 7] = [
    (0, 127),                                        // unspecified, this machine
    (0x0064_ff9b_0000_0000_0000_0000_0000_0000, 96), // an IPv4 address in disguise
    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // documentation
    (0xfc00_0000_0000_0000_0000_0000_0000_0000, 7),  // private
    (0xfe80_0000_0000_0000_0000_0000_0000_0000, 10), // link-local
    (0xfec0_0000_0000_0000_0000_0000_0000_0000, 10), // old private
    (0xff00_0000_0000_0000_0000_0000_0000_0000, 8),  // multicast
];

/// True for an address on the open internet: not this machine, not the
/// user's network.
fn is_public(ip: IpAddr) -> bool {
    let v4_is_public = |ip: Ipv4Addr| !LOCAL_V4.iter().any(|&(net, bits)| u32::from(ip) >> (32 - bits) == net >> (32 - bits));
    match ip {
        IpAddr::V4(v4) => v4_is_public(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4_is_public(v4),
            None => !LOCAL_V6.iter().any(|&(net, bits)| u128::from(v6) >> (128 - bits) == net >> (128 - bits)),
        },
    }
}

/// The public addresses of `netloc`; it connects only to those, so a name
/// cannot lead into the user's network, not even by changing its answer.
fn public_addresses(netloc: &str) -> io::Result<Vec<SocketAddr>> {
    let public: Vec<SocketAddr> = netloc.to_socket_addrs()?.filter(|a| is_public(a.ip())).collect();
    match public.is_empty() {
        true => Err(io::Error::other(format!("{netloc} is not on the open internet"))),
        false => Ok(public),
    }
}

/// A proxy resolves names itself, so all that can be done is to look first.
fn known_to_be_local(host: &str) -> bool {
    (host, 443).to_socket_addrs().is_ok_and(|mut a| a.any(|a| !is_public(a.ip())))
}

fn refuse_local(host: &str) -> Result<(), String> {
    match known_to_be_local(host) {
        true => Err(format!("{host} is not on the open internet")),
        false => Ok(()),
    }
}

fn agent(proxy: &str, time: Duration, local: bool) -> Result<ureq::Agent, String> {
    let builder = ureq::AgentBuilder::new().timeout(time).redirects(0).user_agent(concat!("Popamp/", env!("CARGO_PKG_VERSION")));
    Ok(match (local, proxy) {
        (true, _) => builder,
        (false, "") => builder.resolver(public_addresses),
        (false, _) => builder.proxy(ureq::Proxy::new(proxy).map_err(|e| format!("bad proxy: {e}"))?),
    }
    .build())
}

fn send(req: &HttpRequest, proxy: &str, time: Duration, local: bool) -> Result<HttpResponse, String> {
    if !local && !proxy.is_empty() {
        refuse_local(&Target::of(&req.url)?.host)?;
    }
    let mut request = agent(proxy, time, local)?.request(&req.method, &req.url);
    for (name, value) in &req.headers {
        request = request.set(name, value);
    }
    let sent = match &req.body {
        Some(body) => request.send_string(body),
        None => request.call(),
    };
    match sent {
        Ok(reply) | Err(ureq::Error::Status(_, reply)) => answer(reply),
        Err(e) => Err(e.to_string()),
    }
}

fn answer(reply: ureq::Response) -> Result<HttpResponse, String> {
    let status = reply.status();
    let headers: BTreeMap<String, String> = reply
        .headers_names()
        .into_iter()
        .filter_map(|n| reply.header(&n).map(str::to_string).map(|v| (n.to_ascii_lowercase(), v)))
        .collect();
    let mut body = Vec::new();
    reply.into_reader().take(MAX_BODY + 1).read_to_end(&mut body).map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_BODY {
        return Err(format!("the answer is longer than {MAX_BODY} bytes"));
    }
    Ok(HttpResponse { error: None, status, headers, body: String::from_utf8_lossy(&body).into_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    fn get(url: &str) -> HttpRequest {
        HttpRequest { method: "GET".into(), url: url.into(), headers: BTreeMap::new(), body: None }
    }

    fn reply(status: u16, location: Option<&str>) -> HttpResponse {
        let headers = location.into_iter().map(|l| ("location".to_string(), l.to_string())).collect();
        HttpResponse { status, headers, body: "text".into(), error: None }
    }

    /// What was sent, each with whether it was sent as local.
    type Sent = Arc<Mutex<Vec<(HttpRequest, bool)>>>;

    /// A network that answers from `replies` in order and keeps what was sent.
    fn fake(addresses: &[&str], replies: Vec<HttpResponse>) -> (Network, Sent) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (log, left) = (sent.clone(), Mutex::new(replies.into_iter()));
        let send: Send = Box::new(move |req, _, local| {
            log.lock().unwrap().push((req.clone(), local));
            left.lock().unwrap().next().ok_or("no more".to_string())
        });
        let addresses: Vec<String> = addresses.iter().map(|a| a.to_string()).collect();
        (Network { entries: entries(&addresses), journal: Arc::default(), send }, sent)
    }

    const TIME: Duration = Duration::from_secs(5);

    #[test]
    fn a_listed_host_is_reached_and_an_unlisted_one_is_not() {
        let (net, sent) = fake(&["a.example", "*.b.example"], vec![reply(200, None), reply(200, None), reply(200, None)]);
        assert_eq!(net.fetch(get("https://a.example/x"), TIME).body, "text");
        assert_eq!(net.fetch(get("http://a.example/x"), TIME).status, 200);
        assert_eq!(net.fetch(get("https://cdn.b.example/"), TIME).status, 200);
        let refused = net.fetch(get("https://c.example/"), TIME);
        assert_eq!(refused.error.as_deref(), Some("host c.example is not allowed"));
        assert_eq!(sent.lock().unwrap().len(), 3, "nothing was sent to c.example");
    }

    #[test]
    fn an_unusual_port_must_be_listed_and_only_that_port_opens() {
        let (net, sent) = fake(&["a.example", "b.example:8000"], vec![reply(200, None), reply(200, None)]);
        assert_eq!(net.fetch(get("https://a.example:8080/"), TIME).error.as_deref(), Some("a.example:8080 is not allowed"));
        assert_eq!(net.fetch(get("http://b.example:8000/"), TIME).status, 200);
        assert_eq!(net.fetch(get("https://b.example:8000/"), TIME).status, 200);
        assert_eq!(net.fetch(get("https://b.example:8001/"), TIME).error.as_deref(), Some("b.example:8001 is not allowed"));
        assert_eq!(net.fetch(get("https://b.example/"), TIME).error.as_deref(), Some("host b.example is not allowed"));
        assert_eq!(sent.lock().unwrap().len(), 2);
        assert!(
            net.fetch(get("https://a.example:443/"), TIME).error.as_deref() == Some("no more"),
            "the usual port written out is the same"
        );
    }

    #[test]
    fn what_could_hide_the_real_destination_is_refused() {
        let (net, sent) = fake(&["a.example"], Vec::new());
        for url in [
            "ftp://a.example/",
            "https://a.example@evil.example/",
            "https://a.example:x/",
            "https://a.example\\@evil.example/",
            "a.example",
        ] {
            assert!(net.fetch(get(url), TIME).error.is_some(), "{url}");
        }
        let mut host = get("https://a.example/");
        host.headers.insert("HOST".into(), "evil.example".into());
        assert_eq!(net.fetch(host, TIME).error.as_deref(), Some("the Host header is the player's to set"));
        let mut method = get("https://a.example/");
        method.method = "CONNECT".into();
        assert!(net.fetch(method, TIME).error.unwrap().contains("CONNECT"));
        assert!(sent.lock().unwrap().is_empty());
    }

    #[test]
    fn only_an_address_written_as_local_is_sent_as_local() {
        let (net, sent) = fake(&["a.example:8000", "localhost:8000", "192.168.1.5:8000"], vec![reply(200, None); 3]);
        for url in ["http://a.example:8000/", "http://localhost:8000/", "http://192.168.1.5:8000/"] {
            assert_eq!(net.fetch(get(url), TIME).status, 200, "{url}");
        }
        let local: Vec<bool> = sent.lock().unwrap().iter().map(|(_, local)| *local).collect();
        assert_eq!(local, [false, true, true]);
    }

    #[test]
    fn the_user_decides_on_unusual_ports_and_local_addresses() {
        let caution = |a: &str| NetEntry::parse(a).unwrap().caution();
        for plain in ["a.example", "*.a.example", "a.example:80", "a.example:443", "8.8.8.8"] {
            assert_eq!(caution(plain), None, "{plain}");
        }
        assert_eq!(caution("a.example:8000"), Some("unusual port"));
        for local in ["localhost", "localhost:8000", "127.0.0.1:8000", "192.168.1.5", "10.0.0.2:80"] {
            assert_eq!(caution(local), Some("this computer or your network"), "{local}");
        }
        for bad in ["*", "a.example:", "a.example:0", "a.example:70000", "a.example:x", "http://a.example", ""] {
            assert!(NetEntry::parse(bad).is_err(), "{bad}");
        }
    }

    /// A server on this machine that answers `head` once and returns what
    /// it was asked.
    fn server(head: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let serve = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut asked = vec![0; 2048];
            let n = stream.read(&mut asked).unwrap();
            stream.write_all(head.as_bytes()).unwrap();
            String::from_utf8_lossy(&asked[..n]).into_owned()
        });
        (port, serve)
    }

    #[test]
    fn a_server_on_this_computer_is_reached_once_its_address_is_listed() {
        let (port, serve) = server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-Hi: yes\r\n\r\nok");
        let net = Network::new(&[format!("127.0.0.1:{port}")], "", Arc::default());
        let answer = net.fetch(get(&format!("http://127.0.0.1:{port}/status")), TIME);
        assert_eq!((answer.error.clone(), answer.status, answer.body.as_str()), (None, 200, "ok"), "{answer:?}");
        assert_eq!(answer.headers.get("x-hi").map(String::as_str), Some("yes"));
        assert!(serve.join().unwrap().starts_with("GET /status HTTP/1.1\r\n"));
    }

    #[test]
    fn a_redirect_comes_back_unfollowed() {
        let (port, serve) = server("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\n\r\n");
        let net = Network::new(&[format!("127.0.0.1:{port}")], "", Arc::default());
        let answer = net.fetch(get(&format!("http://127.0.0.1:{port}/")), TIME);
        assert_eq!((answer.error.clone(), answer.status), (None, 302), "{answer:?}");
        assert_eq!(answer.headers.get("location").map(String::as_str), Some("http://127.0.0.1:1/secret"));
        serve.join().unwrap();
    }

    #[test]
    fn a_public_name_cannot_lead_to_this_computer() {
        assert!(send(&get("http://localhost:1/"), "", TIME, false).unwrap_err().contains("open internet"));
    }

    #[test]
    fn each_request_is_in_the_journal_allowed_or_refused() {
        let dir = std::env::temp_dir().join(format!("popamp-net-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut net, _) = fake(&["a.example"], vec![reply(200, None)]);
        net.journal = Arc::new(Journal::new(&dir, "p"));
        net.fetch(HttpRequest { method: "POST".into(), body: Some("12345".into()), ..get("https://a.example/x") }, TIME);
        net.fetch(get("https://b.example/y"), TIME);
        let text = std::fs::read_to_string(Journal::path_in(&dir, "p")).unwrap();
        assert!(text.contains(" net allowed: POST https://a.example/x (5 bytes)\n"), "{text}");
        assert!(text.contains(" net refused: GET https://b.example/y (0 bytes) (host b.example is not allowed)\n"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_the_open_internet_is_public() {
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "198.19.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "203.0.113.9",
            "240.0.0.1",
            "2001:db8::1",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.0.1",
            "64:ff9b::7f00:1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn a_name_for_this_machine_gets_no_connection_unless_written_as_local() {
        assert!(public_addresses("localhost:443").is_err());
        assert!(public_addresses("127.0.0.1:443").is_err());
        assert!(public_addresses("192.168.0.1:443").is_err());
        assert!(known_to_be_local("localhost"));
    }

    #[test]
    #[ignore = "needs the internet"]
    fn a_real_page_comes_back() {
        let net = Network::new(&["example.com".to_string()], "", Arc::default());
        let answer = net.fetch(get("https://example.com/"), Duration::from_secs(20));
        assert_eq!((answer.error, answer.status), (None, 200));
        assert!(answer.body.contains("Example Domain"));
    }
}
