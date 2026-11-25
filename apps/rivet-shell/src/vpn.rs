//! VPNs, of three kinds behind one list: the connections NetworkManager keeps (WireGuard,
//! OpenVPN and the rest, through `nmcli`), Tailscale (`tailscale`) and ZeroTier's networks
//! (`zerotier-cli`). Each is read and switched with its own program, and a kind whose program
//! isn't installed is left out. Everything here runs a program and waits: call it from a thread.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    /// A connection NetworkManager keeps.
    #[default]
    Network,
    Tailscale,
    /// One ZeroTier network.
    ZeroTier,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Vpn {
    pub kind: Kind,
    /// NetworkManager's id for the connection, or the ZeroTier network's id (empty: Tailscale).
    pub id: String,
    pub name: String,
    pub on: bool,
    /// Whether it can be turned on and off from here.
    pub switch: bool,
    /// A quiet line under its name: what kind it is, the address it gave this computer.
    pub detail: String,
}

impl Vpn {
    /// What the settings file and a switch call it.
    pub fn key(&self) -> String {
        match self.kind {
            Kind::Network => format!("nm:{}", self.id),
            Kind::Tailscale => "tailscale".to_string(),
            Kind::ZeroTier => format!("zerotier:{}", self.id),
        }
    }
}

/// A computer on the tailnet that offers to carry this one's traffic.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Exit {
    pub name: String,
    pub address: String,
    pub in_use: bool,
}

/// Everything the VPN page shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Vpns {
    pub list: Vec<Vpn>,
    /// ZeroTier's program is installed.
    pub zerotier: bool,
    /// Tailscale is installed, and is not signed in.
    pub tailscale_signed_out: bool,
    pub exits: Vec<Exit>,
    /// ZeroTier is installed, and its networks can't be read until the person allows it.
    pub zerotier_locked: bool,
}

/// What switching Tailscale says when this user isn't allowed to.
pub const TAILSCALE_LOCKED: &str = "Tailscale needs your permission first";

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("{cmd}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { text.trim().to_string() } else { err })
    }
}

fn installed(cmd: &str) -> bool {
    crate::browser::on_path(cmd).is_some()
}

/// The first line of what a program said went wrong.
fn short(e: &str) -> String {
    e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("it didn't work").trim_start_matches("Error: ").trim_end_matches('.').to_string()
}

// ------------------------------------------------------------------ NetworkManager

/// The VPN connections in nmcli's terse `NAME,UUID,TYPE,ACTIVE,FILENAME` listing.
fn network_from(listing: &str) -> Vec<Vpn> {
    listing
        .lines()
        .filter_map(|line| {
            // The file's path comes last and may hold colons of its own; the name comes first
            // and has its colons escaped.
            let f = crate::radio::split_terse(line);
            if f.len() < 5 {
                return None;
            }
            let label = match f[2].as_str() {
                "wireguard" => "WireGuard",
                "vpn" => "VPN",
                _ => return None,
            };
            let on = f[3] == "yes";
            // Made by another program (wg-quick and the like): NetworkManager only watches it.
            let outside = f[4].starts_with("/run/");
            let detail = match (on, outside) {
                (true, true) => format!("Connected · {label} · turned on by another program"),
                (true, false) => format!("Connected · {label}"),
                _ => label.to_string(),
            };
            Some(Vpn { kind: Kind::Network, id: f[1].clone(), name: f[0].clone(), on, switch: !outside, detail })
        })
        .collect()
}

// ------------------------------------------------------------------ Tailscale

/// Tailscale from `tailscale status --json`: the one entry, whether it is signed out, and the
/// exit nodes on offer.
fn tailscale_from(json: &str) -> Option<(Vpn, bool, Vec<Exit>)> {
    let v: Value = serde_json::from_str(json).ok()?;
    let state = v.get("BackendState")?.as_str()?;
    let on = state == "Running";
    let signed_out = matches!(state, "NeedsLogin" | "NoState" | "NeedsMachineAuth");
    let text = |v: &Value, path: &[&str]| path.iter().try_fold(v, |v, k| v.get(k)).and_then(Value::as_str).unwrap_or_default().to_string();
    let first_ip = |v: &Value| v.get("TailscaleIPs").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut parts = Vec::new();
    if signed_out {
        parts.push("Not signed in".to_string());
    } else if on {
        parts.push("Connected".to_string());
        parts.extend([text(&v, &["CurrentTailnet", "Name"]), first_ip(&v)].into_iter().filter(|p| !p.is_empty()));
    } else {
        parts.extend(Some(text(&v, &["CurrentTailnet", "Name"])).filter(|p| !p.is_empty()));
    }
    let mut exits: Vec<Exit> = v
        .get("Peer")
        .and_then(Value::as_object)
        .map(|peers| peers.values().filter(|p| p.get("ExitNodeOption").and_then(Value::as_bool) == Some(true)).map(|p| Exit { name: text(p, &["HostName"]), address: first_ip(p), in_use: p.get("ExitNode").and_then(Value::as_bool) == Some(true) }).filter(|e| !e.address.is_empty()).collect())
        .unwrap_or_default();
    exits.sort_by_key(|e| e.name.to_lowercase());
    Some((Vpn { kind: Kind::Tailscale, id: String::new(), name: "Tailscale".to_string(), on, switch: !signed_out, detail: parts.join(" · ") }, signed_out, exits))
}

/// Send this computer's traffic through `address` (empty: through none).
pub fn set_exit(address: &str) -> Result<(), String> {
    run("timeout", &["15", "tailscale", "set", &format!("--exit-node={address}")]).map(|_| ()).map_err(|e| tailscale_error(&e))
}

fn tailscale_error(e: &str) -> String {
    if e.contains("Access denied") || e.contains("--operator") { TAILSCALE_LOCKED.to_string() } else { short(e) }
}

/// The one-time step for Tailscale: let this user switch it without a password from then on.
/// Asks for the password.
pub fn allow_tailscale() -> Result<(), String> {
    let user = std::env::var("USER").map_err(|_| "this user's name isn't known".to_string())?;
    as_root(&["tailscale", "set", &format!("--operator={user}")])
}

// ------------------------------------------------------------------ ZeroTier

/// ZeroTier's networks from `zerotier-cli -j listnetworks`.
fn zerotier_from(json: &str) -> Vec<Vpn> {
    let Ok(Value::Array(nets)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    nets.iter()
        .filter_map(|n| {
            let text = |k: &str| n.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let id = text("nwid");
            if id.is_empty() {
                return None;
            }
            let address = n.get("assignedAddresses").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).map(|a| a.split('/').next().unwrap_or(a).to_string()).unwrap_or_default();
            let status = text("status");
            let mut parts = vec![match status.as_str() {
                "OK" => "Connected".to_string(),
                "ACCESS_DENIED" => "Waiting to be let in by the network's owner".to_string(),
                "NOT_FOUND" => "No network has this ID".to_string(),
                _ => "Connecting…".to_string(),
            }];
            parts.extend(Some(address).filter(|a| !a.is_empty() && status == "OK"));
            let name = text("name");
            Some(Vpn { kind: Kind::ZeroTier, name: if name.is_empty() { id.clone() } else { name }, id, on: true, switch: true, detail: parts.join(" · ") })
        })
        .collect()
}

/// Where ZeroTier's program looks for this user's own copy of its key.
fn zerotier_token() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(".zeroTierOneAuthToken"))
}

/// The one-time step for ZeroTier: give this user a copy of its key, which its program then
/// reads by itself. Asks for the password.
pub fn allow_zerotier() -> Result<(), String> {
    let file = zerotier_token().ok_or("this user's home folder isn't known")?;
    let file = file.to_string_lossy();
    // The path goes into a shell's single quotes, which only another quote would end.
    if file.contains('\'') {
        return Err("the home folder's name has a quote in it".to_string());
    }
    let uid = unsafe { libc::getuid() };
    as_root(&["sh", "-c", &format!("cat /var/lib/zerotier-one/authtoken.secret > '{file}' && chown {uid} '{file}' && chmod 600 '{file}'")])
}

/// Whether `text` could be a ZeroTier network's ID: sixteen hexadecimal digits.
pub fn zerotier_id(text: &str) -> bool {
    text.len() == 16 && text.chars().all(|c| c.is_ascii_hexdigit())
}

// ------------------------------------------------------------------ all of them

fn as_root(command: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("pkexec").args(command).stdin(std::process::Stdio::null()).output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "pkexec isn't installed".to_string() } else { e.to_string() })?;
    match out.status.code() {
        Some(0) => Ok(()),
        // pkexec's own: the password window was closed, or the password was refused.
        Some(126) | Some(127) => Err("the password wasn't given".to_string()),
        _ => Err(short(&String::from_utf8_lossy(&out.stderr))),
    }
}

/// Whether any of the three could be here at all (which is when the VPN page shows).
pub fn any_installed() -> bool {
    ["nmcli", "tailscale", "zerotier-cli"].into_iter().any(installed)
}

/// Read all three.
pub fn read() -> Vpns {
    let mut all = Vpns::default();
    if installed("nmcli") {
        all.list = network_from(&run("nmcli", &["-t", "-f", "NAME,UUID,TYPE,ACTIVE,FILENAME", "connection", "show"]).unwrap_or_default());
    }
    if installed("tailscale")
        && let Some((vpn, signed_out, exits)) = run("timeout", &["4", "tailscale", "status", "--json"]).ok().and_then(|j| tailscale_from(&j))
    {
        all.list.push(vpn);
        all.tailscale_signed_out = signed_out;
        all.exits = exits;
    }
    if installed("zerotier-cli") {
        all.zerotier = true;
        match run("timeout", &["4", "zerotier-cli", "-j", "listnetworks"]) {
            Ok(json) => all.list.extend(zerotier_from(&json)),
            Err(e) => all.zerotier_locked = e.contains("authtoken"),
        }
    }
    all
}

/// Turn the VPN called `key` on or off. A ZeroTier network is joined and left.
pub fn set(key: &str, on: bool) -> Result<(), String> {
    if key == "tailscale" {
        return run("timeout", &["20", "tailscale", if on { "up" } else { "down" }]).map(|_| ()).map_err(|e| tailscale_error(&e));
    }
    if let Some(id) = key.strip_prefix("zerotier:") {
        return run("timeout", &["10", "zerotier-cli", if on { "join" } else { "leave" }, id]).map(|_| ()).map_err(|e| short(&e));
    }
    if let Some(id) = key.strip_prefix("nm:") {
        return run("nmcli", &["connection", if on { "up" } else { "down" }, "uuid", id]).map(|_| ()).map_err(|e| short(&e));
    }
    Err("no such VPN".to_string())
}

/// The VPN quick settings switches: the one chosen in Settings, or the first that can be
/// switched. Its key, its name and whether it is on; a chosen one that isn't listed (a ZeroTier
/// network that was left) is off, with no name.
pub fn chosen(list: &[Vpn], key: &str) -> Option<(String, String, bool)> {
    if let Some(v) = list.iter().find(|v| v.key() == key && v.switch) {
        return Some((v.key(), v.name.clone(), v.on));
    }
    if key.starts_with("zerotier:") {
        return Some((key.to_string(), String::new(), false));
    }
    list.iter().find(|v| v.switch).map(|v| (v.key(), v.name.clone(), v.on))
}

/// Made-up VPNs, for screenshots.
pub fn demo() -> Vpns {
    let vpn = |kind, id: &str, name: &str, on, detail: &str| Vpn { kind, id: id.into(), name: name.into(), on, switch: true, detail: detail.into() };
    Vpns {
        list: vec![vpn(Kind::Network, "1", "Office", false, "WireGuard"), vpn(Kind::Network, "2", "Travel", false, "VPN"), vpn(Kind::Tailscale, "", "Tailscale", true, "Connected · example.com · 100.64.0.12"), vpn(Kind::ZeroTier, "8056c2e21c000001", "home-lab", true, "Connected · 10.147.20.5")],
        zerotier: true,
        exits: vec![Exit { name: "home-server".into(), address: "100.64.0.2".into(), in_use: false }, Exit { name: "office-gateway".into(), address: "100.64.0.3".into(), in_use: false }],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_connections() {
        let l = network_from("Home:1:802-11-wireless:yes:/etc/NetworkManager/system-connections/Home.nmconnection\nOffice\\: east:2:wireguard:no:/etc/NetworkManager/system-connections/Office.nmconnection\nwg0:3:wireguard:yes:/run/NetworkManager/system-connections/wg0.nmconnection\nTravel:4:vpn:yes:/etc/NetworkManager/system-connections/Travel.nmconnection\ntailscale0:5:tun:yes:/run/NetworkManager/system-connections/tailscale0.nmconnection");
        assert_eq!(l.iter().map(|v| (v.name.as_str(), v.on, v.switch)).collect::<Vec<_>>(), [("Office: east", false, true), ("wg0", true, false), ("Travel", true, true)]);
        assert_eq!(l[0].key(), "nm:2");
        assert_eq!(l[2].detail, "Connected · VPN");
    }

    #[test]
    fn tailscale_status() {
        let (v, out, exits) = tailscale_from(r#"{"BackendState":"Running","TailscaleIPs":["100.64.0.12","fd7a::1"],"CurrentTailnet":{"Name":"example.com"},
            "Peer":{"a":{"HostName":"zed","TailscaleIPs":["100.64.0.3"],"ExitNodeOption":true,"ExitNode":true},"b":{"HostName":"laptop","TailscaleIPs":["100.64.0.4"],"ExitNodeOption":false},"c":{"HostName":"Home","TailscaleIPs":["100.64.0.2"],"ExitNodeOption":true,"ExitNode":false}}}"#)
        .unwrap();
        assert!(v.on && v.switch && !out);
        assert_eq!(v.detail, "Connected · example.com · 100.64.0.12");
        assert_eq!(exits.iter().map(|e| (e.name.as_str(), e.in_use)).collect::<Vec<_>>(), [("Home", false), ("zed", true)]);
        let (v, out, _) = tailscale_from(r#"{"BackendState":"NeedsLogin"}"#).unwrap();
        assert!(!v.on && !v.switch && out);
        let (v, ..) = tailscale_from(r#"{"BackendState":"Stopped","CurrentTailnet":{"Name":"example.com"}}"#).unwrap();
        assert!(!v.on && v.switch);
        assert_eq!(v.detail, "example.com");
    }

    #[test]
    fn zerotier_networks() {
        let l = zerotier_from(r#"[{"nwid":"8056c2e21c000001","name":"home-lab","status":"OK","assignedAddresses":["10.147.20.5/24"]},{"nwid":"1d71939404000002","name":"","status":"ACCESS_DENIED","assignedAddresses":[]}]"#);
        assert_eq!(l[0].detail, "Connected · 10.147.20.5");
        assert_eq!(l[0].key(), "zerotier:8056c2e21c000001");
        assert_eq!((l[1].name.as_str(), l[1].detail.as_str()), ("1d71939404000002", "Waiting to be let in by the network's owner"));
        assert!(zerotier_id("8056c2e21c000001") && !zerotier_id("8056c2e21c00000") && !zerotier_id("8056c2e21c00000z"));
    }

    #[test]
    fn the_one_quick_settings_switches() {
        let list = demo().list;
        assert_eq!(chosen(&list, ""), Some(("nm:1".into(), "Office".into(), false)));
        assert_eq!(chosen(&list, "tailscale"), Some(("tailscale".into(), "Tailscale".into(), true)));
        // A ZeroTier network that was left is no longer listed, and can still be joined again.
        assert_eq!(chosen(&list, "zerotier:1d71939404000002"), Some(("zerotier:1d71939404000002".into(), String::new(), false)));
        assert_eq!(chosen(&list, "nm:gone").map(|c| c.0), Some("nm:1".into()));
        assert_eq!(chosen(&[], ""), None);
    }

    #[test]
    #[ignore = "reads this computer's VPNs"]
    fn vpns_here() {
        eprintln!("{:#?}", read());
    }
}
