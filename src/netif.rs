//! Network adapter discovery for binding and the kill switch.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Clone, Debug, PartialEq)]
pub struct Adapter {
    pub name: String,
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub looks_like_vpn: bool,
}

impl Adapter {
    pub fn label(&self) -> String {
        let ip = self
            .ipv4
            .map(|ip| ip.to_string())
            .or_else(|| self.ipv6.map(|ip| ip.to_string()))
            .unwrap_or_else(|| "no address".into());
        if self.looks_like_vpn {
            format!("{} · {} (VPN)", self.name, ip)
        } else {
            format!("{} · {}", self.name, ip)
        }
    }
}

const VPN_HINTS: &[&str] = &[
    "vpn", "wireguard", "wg", "tun", "tap", "proton", "nord", "mullvad", "express", "surfshark",
    "pia", "private internet", "windscribe", "ivpn", "openvpn", "wintun", "cyberghost", "airvpn",
];

fn is_vpn_name(name: &str) -> bool {
    let n = name.to_lowercase();
    VPN_HINTS.iter().any(|h| {
        if h.len() <= 3 {
            // Short hints must be a word on their own ("wg0", "tun0", "TAP-Windows").
            n.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| w.starts_with(h) && w[h.len()..].chars().all(|c| c.is_ascii_digit()))
        } else {
            n.contains(h)
        }
    })
}

fn usable_v4(ip: &Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
}

fn usable_v6(ip: &Ipv6Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && (ip.segments()[0] & 0xffc0) != 0xfe80
}

/// Every adapter that currently has a usable address.
pub fn list() -> Vec<Adapter> {
    let mut out: Vec<Adapter> = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        let entry = match out.iter_mut().position(|a| a.name == iface.name) {
            Some(i) => &mut out[i],
            None => {
                out.push(Adapter {
                    looks_like_vpn: is_vpn_name(&iface.name),
                    name: iface.name.clone(),
                    ipv4: None,
                    ipv6: None,
                });
                out.last_mut().unwrap()
            }
        };
        match iface.ip() {
            IpAddr::V4(ip) if usable_v4(&ip) => {
                if entry.ipv4.is_none() {
                    entry.ipv4 = Some(ip)
                }
            }
            IpAddr::V6(ip) if usable_v6(&ip) => {
                if entry.ipv6.is_none() {
                    entry.ipv6 = Some(ip)
                }
            }
            _ => {}
        }
    }
    out.retain(|a| a.ipv4.is_some() || a.ipv6.is_some());
    out.sort_by(|a, b| b.looks_like_vpn.cmp(&a.looks_like_vpn).then(a.name.cmp(&b.name)));
    out
}

pub fn find(name: &str) -> Option<Adapter> {
    list().into_iter().find(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::is_vpn_name;

    #[test]
    fn vpn_names() {
        assert!(is_vpn_name("ProtonVPN"));
        assert!(is_vpn_name("wg0"));
        assert!(is_vpn_name("tun0"));
        assert!(is_vpn_name("TAP-Windows Adapter V9"));
        assert!(is_vpn_name("Mullvad"));
        assert!(!is_vpn_name("Ethernet"));
        assert!(!is_vpn_name("Wi-Fi"));
        assert!(!is_vpn_name("eth0"));
        assert!(!is_vpn_name("Tapestry LAN"));
    }
}
