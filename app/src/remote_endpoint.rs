//! Remote Agent Watch 的可达地址发现与安装命令生成。
//!
//! 本模块把网卡枚举、地址分类、脱敏展示和 shell 命令拼装从 `agent_monitor` 中拆出。
//! 这样会话监听只关心事件流，远程安装入口则可以独立支持 LAN、tailnet/VPN 和后续手动地址。
//! 它与设置页 IPC 共享 `RemoteInstallInfo`，也被 Agent Watch 只读服务用来生成一键安装命令。

use serde::Serialize;
use std::net::{Ipv4Addr, UdpSocket};
use std::process::Command;

use crate::agent_monitor::{DEFAULT_AGENT_MONITOR_PORT, DEFAULT_AGENT_VIEW_PORT};

/// 设置页远程安装区域需要展示和复制的信息。
#[derive(Debug, Clone, Serialize)]
pub struct RemoteInstallInfo {
    pub local_ip: String,
    pub local_ips: Vec<String>,
    pub endpoints: Vec<RemoteEndpointInfo>,
    pub port: u16,
    pub view_port: u16,
    pub watch_url: String,
    pub watch_urls: Vec<String>,
    pub script_path: String,
    pub install_command: String,
}

/// 一个可能可被远程机器访问的 Windows 端地址。
#[derive(Debug, Clone, Serialize)]
pub struct RemoteEndpointInfo {
    pub ip: String,
    pub kind: String,
    pub label: String,
    pub display_label: String,
    pub watch_url: String,
    pub install_url: String,
    pub priority: u8,
    pub selected: bool,
}

#[derive(Debug, Clone)]
struct Ipv4Candidate {
    ip: Ipv4Addr,
    adapter_hint: Option<String>,
}

/// 生成设置页可直接使用的远程安装信息。
pub fn remote_install_info() -> Result<RemoteInstallInfo, String> {
    let endpoints = get_remote_endpoints()?;
    let local_ips = endpoints
        .iter()
        .map(|endpoint| endpoint.ip.clone())
        .collect::<Vec<_>>();
    let local_ip = local_ips
        .first()
        .cloned()
        .ok_or_else(|| "no remote IPv4 endpoint found".to_string())?;
    let script_path = format!("http://{local_ip}:{DEFAULT_AGENT_VIEW_PORT}/remote-install.sh");
    let install_command = build_remote_install_command(
        &local_ips,
        DEFAULT_AGENT_VIEW_PORT,
        DEFAULT_AGENT_MONITOR_PORT,
    );
    let watch_urls = local_ips
        .iter()
        .map(|ip| format!("http://{ip}:{DEFAULT_AGENT_VIEW_PORT}/watch"))
        .collect::<Vec<_>>();
    Ok(RemoteInstallInfo {
        watch_url: format!("http://{local_ip}:{DEFAULT_AGENT_VIEW_PORT}/watch"),
        watch_urls,
        local_ip,
        local_ips,
        endpoints,
        port: DEFAULT_AGENT_MONITOR_PORT,
        view_port: DEFAULT_AGENT_VIEW_PORT,
        script_path,
        install_command,
    })
}

fn get_remote_endpoints() -> Result<Vec<RemoteEndpointInfo>, String> {
    let mut candidates = local_ipv4_candidates();
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let std::net::IpAddr::V4(ipv4) = addr.ip() {
                    candidates.push(Ipv4Candidate {
                        ip: ipv4,
                        adapter_hint: Some("default route".to_string()),
                    });
                }
            }
        }
    }
    let endpoints = remote_endpoints_from_candidates(candidates);
    if endpoints.is_empty() {
        Err("no remote IPv4 endpoint found".to_string())
    } else {
        Ok(endpoints)
    }
}

fn build_remote_install_command(ips: &[String], view_port: u16, monitor_port: u16) -> String {
    if let Some(ip) = ips.first().filter(|_| ips.len() == 1) {
        return format!(
            "curl -fsSL http://{ip}:{view_port}/remote-install.sh | bash -s -- --host {ip} --port {monitor_port}"
        );
    }
    let hosts = ips.join(" ");
    format!(
        "BITCAT_SCRIPT=; for h in {hosts}; do BITCAT_SCRIPT=$(curl -fsSL \"http://$h:{view_port}/remote-install.sh\") && printf '%s\\n' \"$BITCAT_SCRIPT\" | bash -s -- --hosts \"{hosts}\" --port {monitor_port} && break; done"
    )
}

fn local_ipv4_candidates() -> Vec<Ipv4Candidate> {
    let mut out = Vec::new();
    for command in local_ip_commands() {
        if let Ok(output) = Command::new(command.0).args(command.1).output() {
            out.extend(parse_ipv4_candidates(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }
    out
}

#[cfg(windows)]
fn local_ip_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("ipconfig", vec![])]
}

#[cfg(not(windows))]
fn local_ip_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("hostname", vec!["-I"]),
        ("ip", vec!["-4", "addr", "show"]),
        ("ifconfig", vec![]),
    ]
}

#[cfg(test)]
fn parse_ipv4_addrs(text: &str) -> Vec<Ipv4Addr> {
    parse_ipv4_candidates(text)
        .into_iter()
        .map(|candidate| candidate.ip)
        .collect()
}

fn parse_ipv4_candidates(text: &str) -> Vec<Ipv4Candidate> {
    let mut section_hint: Option<String> = None;
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let starts_with_space = line.chars().next().is_some_and(char::is_whitespace);
        if !starts_with_space && trimmed.ends_with(':') {
            section_hint = Some(trimmed.trim_end_matches(':').to_string());
        }
        out.extend(
            extract_ipv4_addrs(trimmed)
                .into_iter()
                .map(|ip| Ipv4Candidate {
                    ip,
                    adapter_hint: section_hint.clone(),
                }),
        );
    }
    out
}

fn extract_ipv4_addrs(text: &str) -> Vec<Ipv4Addr> {
    text.split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .filter(|part| part.matches('.').count() == 3)
        .filter_map(|part| part.parse::<Ipv4Addr>().ok())
        .collect()
}

fn remote_endpoints_from_candidates(candidates: Vec<Ipv4Candidate>) -> Vec<RemoteEndpointInfo> {
    let mut endpoints = candidates
        .into_iter()
        .filter(|candidate| {
            !candidate.ip.is_loopback()
                && !candidate.ip.is_unspecified()
                && !candidate.ip.is_multicast()
        })
        .map(remote_endpoint_info)
        .collect::<Vec<_>>();
    endpoints.sort_by_key(|endpoint| {
        (
            endpoint.priority,
            endpoint.ip.ends_with(".1"),
            endpoint.ip.clone(),
        )
    });
    endpoints.dedup_by(|left, right| left.ip == right.ip);
    if let Some(first) = endpoints.first_mut() {
        first.selected = true;
    }
    endpoints
}

fn remote_endpoint_info(candidate: Ipv4Candidate) -> RemoteEndpointInfo {
    let ip = candidate.ip;
    let kind = ipv4_kind(ip, candidate.adapter_hint.as_deref());
    let priority = ipv4_score(ip, candidate.adapter_hint.as_deref());
    let redacted_ip = redact_ipv4(ip);
    let label = match kind {
        "lan" => format!("LAN {ip}"),
        "tailscale" => format!("Tailscale {ip}"),
        "tailnet" => format!("Tailnet/CGNAT {ip}"),
        "virtual" => format!("Virtual {ip}"),
        "link_local" => format!("Link-local {ip}"),
        _ => format!("Public {ip}"),
    };
    let display_label = match kind {
        "lan" => format!("LAN {redacted_ip}"),
        "tailscale" => format!("Tailscale {redacted_ip}"),
        "tailnet" => format!("Tailnet/CGNAT {redacted_ip}"),
        "virtual" => format!("Virtual {redacted_ip}"),
        "link_local" => format!("Link-local {redacted_ip}"),
        _ => format!("Public {redacted_ip}"),
    };
    RemoteEndpointInfo {
        ip: ip.to_string(),
        kind: kind.to_string(),
        label,
        display_label,
        watch_url: format!("http://{ip}:{DEFAULT_AGENT_VIEW_PORT}/watch"),
        install_url: format!("http://{ip}:{DEFAULT_AGENT_VIEW_PORT}/remote-install.sh"),
        priority,
        selected: false,
    }
}

#[cfg(test)]
fn sorted_remote_ipv4(candidates: Vec<Ipv4Addr>) -> Vec<Ipv4Addr> {
    remote_endpoints_from_candidates(
        candidates
            .into_iter()
            .map(|ip| Ipv4Candidate {
                ip,
                adapter_hint: None,
            })
            .collect(),
    )
    .into_iter()
    .filter_map(|endpoint| endpoint.ip.parse().ok())
    .collect()
}

fn redact_ipv4(ip: Ipv4Addr) -> String {
    let [a, b, _, d] = ip.octets();
    format!("{a}.{b}.*.{d}")
}

fn ipv4_score(ip: Ipv4Addr, adapter_hint: Option<&str>) -> u8 {
    match ipv4_kind(ip, adapter_hint) {
        "lan" => {
            let [a, b, _, _] = ip.octets();
            return match (a, b) {
                (10, _) => 0,
                (192, 168) => 1,
                (172, 16..=31) => 2,
                _ => 3,
            };
        }
        "tailscale" => return 4,
        "tailnet" => return 5,
        "public" => return 10,
        "virtual" => return 20,
        "link_local" => return 30,
        _ => {}
    }
    40
}

fn ipv4_kind(ip: Ipv4Addr, adapter_hint: Option<&str>) -> &'static str {
    let hint = adapter_hint.unwrap_or_default().to_ascii_lowercase();
    let [a, b, _, _] = ip.octets();
    if hint.contains("tailscale") {
        return "tailscale";
    }
    match (a, b) {
        (10, _) | (192, 168) | (172, 16..=31) => "lan",
        (100, 64..=127) => "tailnet",
        (198, 18 | 19) => "virtual",
        (169, 254) => "link_local",
        _ if ip.is_private() => "lan",
        _ => "public",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_private_lan_before_benchmark_network() {
        let selected = sorted_remote_ipv4(vec![
            "198.18.0.1".parse().unwrap(),
            "172.28.96.110".parse().unwrap(),
            "10.0.0.20".parse().unwrap(),
            "10.0.0.1".parse().unwrap(),
        ])
        .into_iter()
        .next();
        assert_eq!(selected, Some("10.0.0.20".parse().unwrap()));
    }

    #[test]
    fn includes_tailnet_after_lan_before_public() {
        let sorted = sorted_remote_ipv4(vec![
            "8.8.8.8".parse().unwrap(),
            "100.64.0.10".parse().unwrap(),
            "192.168.0.20".parse().unwrap(),
        ]);
        assert_eq!(sorted[0], "192.168.0.20".parse::<Ipv4Addr>().unwrap());
        assert_eq!(sorted[1], "100.64.0.10".parse::<Ipv4Addr>().unwrap());
        assert_eq!(ipv4_kind(sorted[1], None), "tailnet");
    }

    #[test]
    fn uses_adapter_hint_to_label_tailscale() {
        let endpoints = remote_endpoints_from_candidates(vec![Ipv4Candidate {
            ip: "100.64.0.10".parse().unwrap(),
            adapter_hint: Some("Unknown adapter Tailscale".into()),
        }]);
        assert_eq!(endpoints[0].kind, "tailscale");
        assert_eq!(endpoints[0].display_label, "Tailscale 100.64.*.10");
    }

    #[test]
    fn parses_windows_ipconfig_ipv4_lines() {
        let parsed = parse_ipv4_addrs(
            r#"
   IPv4 Address. . . . . . . . . . . : 198.18.0.1
                                       198.18.0.2
   IPv4 Address. . . . . . . . . . . : 172.28.96.110
   IPv4 Address. . . . . . . . . . . : 10.0.0.20
                                       10.0.0.1
"#,
        );
        assert!(parsed.contains(&"198.18.0.1".parse().unwrap()));
        assert!(parsed.contains(&"10.0.0.20".parse().unwrap()));
    }

    #[test]
    fn builds_remote_install_command_that_downloads_script() {
        let command =
            build_remote_install_command(&["10.0.0.20".into(), "192.168.0.20".into()], 5344, 5342);
        assert!(command.contains("for h in 10.0.0.20 192.168.0.20"));
        assert!(command.contains("http://$h:5344/remote-install.sh"));
        assert!(command.contains("--hosts \"10.0.0.20 192.168.0.20\" --port 5342"));
    }

    #[test]
    fn single_host_command_is_short() {
        let command = build_remote_install_command(&["192.168.0.20".into()], 5344, 5342);
        assert_eq!(
            command,
            "curl -fsSL http://192.168.0.20:5344/remote-install.sh | bash -s -- --host 192.168.0.20 --port 5342"
        );
    }
}
