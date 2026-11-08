//! Host naming conventions; collection and USB protocol stay platform independent.
use sysinfo::NetworkData;

pub fn default_interface(os: &str, name: &str, loopback: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    if loopback || matches!(lower.as_str(), "lo" | "lo0") || lower.contains("loopback") {
        return false;
    }
    match os {
        "macos" => name.starts_with("en"),
        "linux" => ["en", "eth", "wl", "ww", "usb"]
            .iter()
            .any(|p| name.starts_with(p)),
        // Windows adapter names may be localized or user-renamed. No English-only whitelist.
        "windows" => true,
        _ => true,
    }
}

pub fn selected_interface(name: &str, data: &NetworkData, selected: Option<&str>) -> bool {
    if let Some(selected) = selected {
        return name == selected;
    }
    let ips = data.ip_networks();
    let loopback = !ips.is_empty() && ips.iter().all(|ip| ip.addr.is_loopback());
    default_interface(std::env::consts::OS, name, loopback)
}
