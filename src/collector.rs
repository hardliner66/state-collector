use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::OnceLock,
};

use anyhow::Context;
use chrono::Local;
use rune::{Any, ContextError, Module, Value, runtime::Bytes};
use serde::{Deserialize, Serialize};

use crate::{
    OUTDIR,
    constants::{LOG_PREFIX, MODULE_NAME, OS_RELEASE_PATH, UPTIME_PATH},
};

// ── Primitive shared types ─────────────────────────────────────────────────

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub enum Format {
    #[rune(constructor)]
    Json,
    #[rune(constructor)]
    PrettyJson,
    #[rune(constructor)]
    Rsn,
    #[rune(constructor)]
    RsnPretty,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct TextLines {
    pub lines: Vec<String>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct UptimeInfo {
    pub pretty: String,
    pub uptime_seconds: f64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
}

// ── Parsed structured types ────────────────────────────────────────────────

/// One row from `free -b` output (Mem or Swap).
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct MemoryRow {
    pub label: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub shared_bytes: Option<u64>,
    pub buff_cache_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
}

/// One row from `df` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct DfEntry {
    pub filesystem: String,
    pub size: String,
    pub used: String,
    pub avail: String,
    pub use_percent: u8,
    pub mountpoint: String,
}

/// One row from `du` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct DiskUsage {
    pub size: String,
    pub path: String,
}

/// One block device from `lsblk -P` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct BlockDevice {
    pub name: String,
    pub size: String,
    pub device_type: String,
    pub fstype: String,
    pub label: String,
    pub uuid: String,
    pub mountpoints: Vec<String>,
}

/// One device from `lspci` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct PciDevice {
    pub slot: String,
    pub class: String,
    pub description: String,
}

/// One device from `lsusb` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct UsbDevice {
    pub bus: u32,
    pub device: u32,
    pub vendor_id: String,
    pub product_id: String,
    pub description: String,
}

/// One process from `ps aux` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct Process {
    pub user: String,
    pub pid: u32,
    pub cpu_pct: f32,
    pub mem_pct: f32,
    pub vsz_kb: u64,
    pub rss_kb: u64,
    pub tty: String,
    pub stat: String,
    pub start: String,
    pub time: String,
    pub command: String,
}

/// One socket from `netstat -an` output (internet connections only).
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct SocketEntry {
    pub proto: String,
    pub recv_q: u64,
    pub send_q: u64,
    pub local_addr: String,
    pub foreign_addr: String,
    pub state: String,
}

/// One unit from `systemctl list-units` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct SystemdUnit {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

/// One entry from `mount` output.
#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct MountEntry {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: Vec<String>,
}

// ── Section structs ────────────────────────────────────────────────────────

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct HostSection {
    pub hostname: String,
    pub os_release: Vec<KeyValue>,
    pub uptime: UptimeInfo,
    pub hostnamectl: Vec<KeyValue>,
    pub timedatectl: Vec<KeyValue>,
    pub locale: Vec<KeyValue>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct ResourcesSection {
    pub uptime: UptimeInfo,
    pub memory: Vec<MemoryRow>,
    pub disk_root: Vec<DfEntry>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct HardwareSection {
    pub memory: Vec<MemoryRow>,
    pub block_devices: Vec<BlockDevice>,
    pub blkid: Vec<KeyValue>,
    pub pci_devices: Vec<PciDevice>,
    pub usb_devices: Vec<UsbDevice>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct SystemdStatusSection {
    pub services: Vec<SystemdUnit>,
    pub failed: TextLines,
    pub failed_units: TextLines,
    pub timers: TextLines,
    pub jobs: TextLines,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct NetworkSection {
    pub ip_addr: TextLines,
    pub ip_route: TextLines,
    pub ip_rule: TextLines,
    pub resolvectl_status: TextLines,
    pub resolv_conf: TextLines,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct FilesystemSection {
    pub mounts: Vec<MountEntry>,
    pub findmnt: TextLines,
    pub block_devices: Vec<BlockDevice>,
    pub df: Vec<DfEntry>,
    pub df_inodes: Vec<DfEntry>,
    pub du_log_tmp: Vec<DiskUsage>,
    pub du_media_card: Vec<DiskUsage>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct ProcessSection {
    pub processes: Vec<Process>,
}

#[derive(Any, Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub generated_at: String,
    pub collector_version: String,
    pub outdir: String,
    pub host: HostSection,
    pub resources: ResourcesSection,
    pub hardware: HardwareSection,
    pub services: Vec<SystemdUnit>,
    pub systemd_status: SystemdStatusSection,
    pub network: NetworkSection,
    pub wifi: TextLines,
    pub ports: Vec<SocketEntry>,
    pub filesystems: FilesystemSection,
    pub processes: ProcessSection,
}

// ── Low-level command runner ───────────────────────────────────────────────

fn run_command(cmd: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;
    if !stdout.trim().is_empty() {
        return Ok(stdout);
    }

    let stderr = String::from_utf8(output.stderr)?;
    if !stderr.trim().is_empty() {
        return Ok(stderr);
    }

    Ok(String::new())
}

// ── Text helpers ───────────────────────────────────────────────────────────

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn key_values_equals(text: &str) -> Vec<KeyValue> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some(KeyValue {
                key: key.trim().to_string(),
                value: value.trim().trim_matches('"').to_string(),
            })
        })
        .collect()
}

fn key_values_colon(text: &str) -> Vec<KeyValue> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (key, value) = line.split_once(':')?;
            Some(KeyValue {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

fn text_or_error(cmd: &str, args: &[&str]) -> TextLines {
    TextLines {
        lines: lines(&run_command(cmd, args).unwrap_or_else(|error| format!("[error: {error}]"))),
    }
}

fn kv_or_error_equals(cmd: &str, args: &[&str]) -> Vec<KeyValue> {
    match run_command(cmd, args) {
        Ok(text) => key_values_equals(&text),
        Err(error) => vec![KeyValue {
            key: "error".to_string(),
            value: format!("{error}"),
        }],
    }
}

fn kv_or_error_colon(cmd: &str, args: &[&str]) -> Vec<KeyValue> {
    match run_command(cmd, args) {
        Ok(text) => key_values_colon(&text),
        Err(error) => vec![KeyValue {
            key: "error".to_string(),
            value: format!("{error}"),
        }],
    }
}

// ── Parse helpers ──────────────────────────────────────────────────────────

/// Split `s` into at most `n` whitespace-delimited fields, where the final
/// field captures all remaining text (including spaces).
fn split_n_fields<'a>(s: &'a str, n: usize) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut rest = s.trim_start();
    for _ in 0..n.saturating_sub(1) {
        if rest.is_empty() {
            break;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        result.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    if !rest.is_empty() {
        result.push(rest);
    }
    result
}

/// Parse a single `lsblk -P` output line (KEY="value" KEY="value" …) into
/// a list of (key, value) pairs.
fn parse_lsblk_kv_line(line: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut rest = line.trim();
    while !rest.is_empty() {
        let eq = match rest.find('=') {
            Some(p) => p,
            None => break,
        };
        let key = rest[..eq].trim().to_string();
        rest = &rest[eq + 1..];
        let value = if rest.starts_with('"') {
            rest = &rest[1..];
            let end = rest.find('"').unwrap_or(rest.len());
            let val = rest[..end].to_string();
            rest = if end < rest.len() {
                &rest[end + 1..]
            } else {
                ""
            };
            val
        } else {
            let end = rest.find(' ').unwrap_or(rest.len());
            let val = rest[..end].to_string();
            rest = &rest[end..];
            val
        };
        rest = rest.trim_start();
        result.push((key, value));
    }
    result
}

/// Parse `free -b` output into `MemoryRow` entries.
fn parse_free() -> Vec<MemoryRow> {
    let text = match run_command("free", &["-b"]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let label = parts[0].trim_end_matches(':').to_string();
        let total_bytes: u64 = parts[1].parse().unwrap_or(0);
        let used_bytes: u64 = parts[2].parse().unwrap_or(0);
        let free_bytes: u64 = parts[3].parse().unwrap_or(0);
        if parts.len() >= 7 {
            rows.push(MemoryRow {
                label,
                total_bytes,
                used_bytes,
                free_bytes,
                shared_bytes: parts[4].parse().ok(),
                buff_cache_bytes: parts[5].parse().ok(),
                available_bytes: parts[6].parse().ok(),
            });
        } else {
            rows.push(MemoryRow {
                label,
                total_bytes,
                used_bytes,
                free_bytes,
                shared_bytes: None,
                buff_cache_bytes: None,
                available_bytes: None,
            });
        }
    }
    rows
}

/// Parse `df <args>` output into `DfEntry` entries (works for both `-h` and
/// `-ih` since the column layout is identical).
fn parse_df(args: &[&str]) -> Vec<DfEntry> {
    let text = match run_command("df", args) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts = split_n_fields(line, 6);
        if parts.len() < 6 {
            continue;
        }
        result.push(DfEntry {
            filesystem: parts[0].to_string(),
            size: parts[1].to_string(),
            used: parts[2].to_string(),
            avail: parts[3].to_string(),
            use_percent: parts[4].trim_end_matches('%').parse().unwrap_or(0),
            mountpoint: parts[5].to_string(),
        });
    }
    result
}

/// Parse `du -sh <paths>` output into `DiskUsage` entries.
fn parse_du(paths: &[&str]) -> Vec<DiskUsage> {
    let mut args = vec!["-sh"];
    args.extend_from_slice(paths);
    let text = match run_command("du", &args) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (size, path) = line.split_once('\t')?;
            Some(DiskUsage {
                size: size.trim().to_string(),
                path: path.trim().to_string(),
            })
        })
        .collect()
}

/// Parse `lsblk -P -o NAME,SIZE,TYPE,FSTYPE,LABEL,UUID,MOUNTPOINT` output
/// into `BlockDevice` entries.
fn parse_lsblk() -> Vec<BlockDevice> {
    let text = match run_command(
        "lsblk",
        &["-P", "-o", "NAME,SIZE,TYPE,FSTYPE,LABEL,UUID,MOUNTPOINT"],
    ) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let pairs = parse_lsblk_kv_line(line);
        let get = |key: &str| -> String {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let mp = get("MOUNTPOINT");
        let mountpoints: Vec<String> = if mp.is_empty() { vec![] } else { vec![mp] };
        result.push(BlockDevice {
            name: get("NAME"),
            size: get("SIZE"),
            device_type: get("TYPE"),
            fstype: get("FSTYPE"),
            label: get("LABEL"),
            uuid: get("UUID"),
            mountpoints,
        });
    }
    result
}

/// Parse `lspci` output into `PciDevice` entries.
fn parse_lspci() -> Vec<PciDevice> {
    let text = match run_command("lspci", &[]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (slot, rest) = line.split_once(' ')?;
            let (class, description) = rest.split_once(": ")?;
            Some(PciDevice {
                slot: slot.to_string(),
                class: class.trim().to_string(),
                description: description.trim().to_string(),
            })
        })
        .collect()
}

/// Parse `lsusb` output into `UsbDevice` entries.
fn parse_lsusb() -> Vec<UsbDevice> {
    let text = match run_command("lsusb", &[]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // "Bus 002 Device 003: ID 046d:c07e Description..."
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                return None;
            }
            let bus: u32 = parts[1].parse().ok()?;
            let device: u32 = parts[3].trim_end_matches(':').parse().ok()?;
            let id_parts: Vec<&str> = parts[5].splitn(2, ':').collect();
            if id_parts.len() < 2 {
                return None;
            }
            let description = if parts.len() > 6 {
                parts[6..].join(" ")
            } else {
                String::new()
            };
            Some(UsbDevice {
                bus,
                device,
                vendor_id: id_parts[0].to_string(),
                product_id: id_parts[1].to_string(),
                description,
            })
        })
        .collect()
}

/// Parse `ps aux` output into `Process` entries.
fn parse_ps() -> Vec<Process> {
    let text = match run_command("ps", &["aux"]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts = split_n_fields(line, 11);
        if parts.len() < 11 {
            continue;
        }
        result.push(Process {
            user: parts[0].to_string(),
            pid: parts[1].parse().unwrap_or(0),
            cpu_pct: parts[2].parse().unwrap_or(0.0),
            mem_pct: parts[3].parse().unwrap_or(0.0),
            vsz_kb: parts[4].parse().unwrap_or(0),
            rss_kb: parts[5].parse().unwrap_or(0),
            tty: parts[6].to_string(),
            stat: parts[7].to_string(),
            start: parts[8].to_string(),
            time: parts[9].to_string(),
            command: parts[10].to_string(),
        });
    }
    result
}

/// Parse `netstat -an` internet-connection lines into `SocketEntry` entries.
fn parse_netstat() -> Vec<SocketEntry> {
    let text = match run_command("netstat", &["-an"]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    let mut in_internet = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Active Internet") {
            in_internet = true;
            continue;
        }
        if line.starts_with("Active UNIX") {
            in_internet = false;
            continue;
        }
        if !in_internet || line.is_empty() || line.starts_with("Proto") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        result.push(SocketEntry {
            proto: parts[0].to_string(),
            recv_q: parts[1].parse().unwrap_or(0),
            send_q: parts[2].parse().unwrap_or(0),
            local_addr: parts[3].to_string(),
            foreign_addr: parts[4].to_string(),
            state: parts.get(5).copied().unwrap_or("").to_string(),
        });
    }
    result
}

/// Parse `systemctl list-units --type=service --all --plain --no-legend` into
/// `SystemdUnit` entries.
fn parse_systemd_units() -> Vec<SystemdUnit> {
    let text = match run_command(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--all",
            "--plain",
            "--no-legend",
            "--no-pager",
        ],
    ) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip footer lines such as "123 loaded units listed."
        if line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let parts = split_n_fields(line, 5);
        if parts.len() < 4 {
            continue;
        }
        result.push(SystemdUnit {
            unit: parts[0].to_string(),
            load: parts[1].to_string(),
            active: parts[2].to_string(),
            sub: parts[3].to_string(),
            description: parts.get(4).unwrap_or(&"").to_string(),
        });
    }
    result
}

/// Parse `mount` output into `MountEntry` entries.
fn parse_mount() -> Vec<MountEntry> {
    let text = match run_command("mount", &[]) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // "<device> on <mountpoint> type <fstype> (<options>)"
            let (device, rest) = line.split_once(" on ")?;
            let (rest, options_raw) = rest.split_once(" (")?;
            let (mountpoint, fstype_part) = rest.split_once(" type ")?;
            let options = options_raw
                .trim_end_matches(')')
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            Some(MountEntry {
                device: device.trim().to_string(),
                mountpoint: mountpoint.trim().to_string(),
                fstype: fstype_part.trim().to_string(),
                options,
            })
        })
        .collect()
}

// ── Render helpers ─────────────────────────────────────────────────────────

fn render_lines(title: &str, lines: &[String]) -> String {
    let mut output = String::new();
    output.push_str(title);
    output.push('\n');
    output.push_str(&"-".repeat(title.len()));
    output.push('\n');
    for line in lines {
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn render_kvs(title: &str, entries: &[KeyValue]) -> String {
    let mut output = String::new();
    output.push_str(title);
    output.push('\n');
    output.push_str(&"-".repeat(title.len()));
    output.push('\n');
    for entry in entries {
        output.push_str(&entry.key);
        output.push_str(": ");
        output.push_str(&entry.value);
        output.push('\n');
    }
    output
}

fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    const KIB: u64 = 1 << 10;
    if bytes >= GIB {
        format!("{:.1}G", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1}M", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1}K", bytes as f64 / KIB as f64)
    } else {
        format!("{}B", bytes)
    }
}

fn render_memory_rows(title: &str, rows: &[MemoryRow]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:6}  {:>10}  {:>10}  {:>10}  {:>10}  {:>12}  {:>12}\n",
        "", "total", "used", "free", "shared", "buff/cache", "available"
    ));
    for row in rows {
        out.push_str(&format!(
            "{:6}  {:>10}  {:>10}  {:>10}  {:>10}  {:>12}  {:>12}\n",
            row.label,
            format_bytes(row.total_bytes),
            format_bytes(row.used_bytes),
            format_bytes(row.free_bytes),
            row.shared_bytes.map(format_bytes).unwrap_or_default(),
            row.buff_cache_bytes.map(format_bytes).unwrap_or_default(),
            row.available_bytes.map(format_bytes).unwrap_or_default(),
        ));
    }
    out
}

fn render_df_entries(title: &str, entries: &[DfEntry]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<30}  {:>8}  {:>8}  {:>8}  {:>4}  {}\n",
        "Filesystem", "Size", "Used", "Avail", "Use%", "Mounted on"
    ));
    for e in entries {
        out.push_str(&format!(
            "{:<30}  {:>8}  {:>8}  {:>8}  {:>3}%  {}\n",
            e.filesystem, e.size, e.used, e.avail, e.use_percent, e.mountpoint
        ));
    }
    out
}

fn render_disk_usage(title: &str, entries: &[DiskUsage]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    for e in entries {
        out.push_str(&format!("{:>8}  {}\n", e.size, e.path));
    }
    out
}

fn render_block_devices(title: &str, devices: &[BlockDevice]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<14}  {:>8}  {:>6}  {:>8}  {:<20}  {:<36}  {}\n",
        "Name", "Size", "Type", "FSType", "Label", "UUID", "Mountpoints"
    ));
    for d in devices {
        out.push_str(&format!(
            "{:<14}  {:>8}  {:>6}  {:>8}  {:<20}  {:<36}  {}\n",
            d.name,
            d.size,
            d.device_type,
            d.fstype,
            d.label,
            d.uuid,
            d.mountpoints.join(", ")
        ));
    }
    out
}

fn render_pci_devices(title: &str, devices: &[PciDevice]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    for d in devices {
        out.push_str(&format!("{}  {}:  {}\n", d.slot, d.class, d.description));
    }
    out
}

fn render_usb_devices(title: &str, devices: &[UsbDevice]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:>3}  {:>6}  {:>6}:{:<6}  {}\n",
        "Bus", "Device", "Vendor", "Product", "Description"
    ));
    for d in devices {
        out.push_str(&format!(
            "{:>3}  {:>6}  {}:{}  {}\n",
            d.bus, d.device, d.vendor_id, d.product_id, d.description
        ));
    }
    out
}

fn render_processes(title: &str, processes: &[Process]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<12}  {:>7}  {:>5}  {:>5}  {:>8}  {:>8}  {:<8}  {:<6}  {:<8}  {:<8}  {}\n",
        "USER", "PID", "%CPU", "%MEM", "VSZ", "RSS", "TTY", "STAT", "START", "TIME", "COMMAND"
    ));
    for p in processes {
        out.push_str(&format!(
            "{:<12}  {:>7}  {:>5.1}  {:>5.1}  {:>8}  {:>8}  {:<8}  {:<6}  {:<8}  {:<8}  {}\n",
            p.user,
            p.pid,
            p.cpu_pct,
            p.mem_pct,
            p.vsz_kb,
            p.rss_kb,
            p.tty,
            p.stat,
            p.start,
            p.time,
            p.command
        ));
    }
    out
}

fn render_socket_entries(title: &str, entries: &[SocketEntry]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<6}  {:>7}  {:>7}  {:<28}  {:<28}  {}\n",
        "Proto", "Recv-Q", "Send-Q", "Local Address", "Foreign Address", "State"
    ));
    for e in entries {
        out.push_str(&format!(
            "{:<6}  {:>7}  {:>7}  {:<28}  {:<28}  {}\n",
            e.proto, e.recv_q, e.send_q, e.local_addr, e.foreign_addr, e.state
        ));
    }
    out
}

fn render_systemd_units(title: &str, units: &[SystemdUnit]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<40}  {:<8}  {:<8}  {:<10}  {}\n",
        "UNIT", "LOAD", "ACTIVE", "SUB", "DESCRIPTION"
    ));
    for u in units {
        out.push_str(&format!(
            "{:<40}  {:<8}  {:<8}  {:<10}  {}\n",
            u.unit, u.load, u.active, u.sub, u.description
        ));
    }
    out
}

fn render_mount_entries(title: &str, entries: &[MountEntry]) -> String {
    let mut out = format!("{title}\n{}\n", "-".repeat(title.len()));
    out.push_str(&format!(
        "{:<30}  {:<25}  {:<12}  {}\n",
        "Device", "Mountpoint", "FSType", "Options"
    ));
    for e in entries {
        out.push_str(&format!(
            "{:<30}  {:<25}  {:<12}  {}\n",
            e.device,
            e.mountpoint,
            e.fstype,
            e.options.join(",")
        ));
    }
    out
}

// ── Uptime / hostname ──────────────────────────────────────────────────────

pub(crate) fn format_uptime(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).floor() as u64;
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!(
            "up {days} day{}, {hours:02}:{minutes:02}:{seconds:02}",
            if days == 1 { "" } else { "s" }
        )
    } else {
        format!("up {hours:02}:{minutes:02}:{seconds:02}")
    }
}

fn uptime_info() -> anyhow::Result<UptimeInfo> {
    let uptime_seconds = std::fs::read_to_string("/proc/uptime")?
        .split_whitespace()
        .next()
        .context("missing uptime seconds")?
        .parse::<f64>()?;

    let load_values = std::fs::read_to_string("/proc/loadavg")?;
    let mut parts = load_values.split_whitespace();
    let load_1 = parts
        .next()
        .context("missing load average 1")?
        .parse::<f64>()?;
    let load_5 = parts
        .next()
        .context("missing load average 5")?
        .parse::<f64>()?;
    let load_15 = parts
        .next()
        .context("missing load average 15")?
        .parse::<f64>()?;

    Ok(UptimeInfo {
        pretty: format_uptime(uptime_seconds),
        uptime_seconds,
        load_1,
        load_5,
        load_15,
    })
}

fn hostname_inner() -> anyhow::Result<String> {
    let mut buffer = vec![0u8; 256];
    let result =
        unsafe { libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len()) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    let nul = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    buffer.truncate(nul);
    Ok(String::from_utf8(buffer)?)
}

#[rune::function]
fn hostname() -> anyhow::Result<String> {
    hostname_inner()
}

// ── Snapshot builder ───────────────────────────────────────────────────────

fn snapshot_inner() -> anyhow::Result<Snapshot> {
    let memory = parse_free();
    let systemd_services = parse_systemd_units();
    let block_devices = parse_lsblk();

    Ok(Snapshot {
        generated_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        collector_version: env!("CARGO_PKG_VERSION").to_string(),
        outdir: std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .into_owned(),
        host: HostSection {
            hostname: hostname_inner()?,
            os_release: key_values_equals(&std::fs::read_to_string("/etc/os-release")?),
            uptime: uptime_info()?,
            hostnamectl: kv_or_error_colon("hostnamectl", &[]),
            timedatectl: kv_or_error_colon("timedatectl", &[]),
            locale: kv_or_error_equals("locale", &[]),
        },
        resources: ResourcesSection {
            uptime: uptime_info()?,
            memory: memory.clone(),
            disk_root: parse_df(&["-h", "/"]),
        },
        hardware: HardwareSection {
            memory: memory.clone(),
            block_devices: block_devices.clone(),
            blkid: kv_or_error_equals("blkid", &[]),
            pci_devices: parse_lspci(),
            usb_devices: parse_lsusb(),
        },
        services: systemd_services.clone(),
        systemd_status: SystemdStatusSection {
            services: systemd_services,
            failed: text_or_error("systemctl", &["--failed", "--no-legend", "--no-pager"]),
            failed_units: text_or_error("systemctl", &["list-units", "--state=failed"]),
            timers: text_or_error("systemctl", &["list-timers", "--all"]),
            jobs: text_or_error("systemctl", &["list-jobs"]),
        },
        network: NetworkSection {
            ip_addr: text_or_error("ip", &["addr"]),
            ip_route: text_or_error("ip", &["route"]),
            ip_rule: text_or_error("ip", &["rule"]),
            resolvectl_status: text_or_error("resolvectl", &["status"]),
            resolv_conf: text_or_error("cat", &["/etc/resolv.conf"]),
        },
        wifi: text_or_error("nmcli", &["dev", "wifi"]),
        ports: parse_netstat(),
        filesystems: FilesystemSection {
            mounts: parse_mount(),
            findmnt: text_or_error("findmnt", &[]),
            block_devices,
            df: parse_df(&["-h"]),
            df_inodes: parse_df(&["-ih"]),
            du_log_tmp: parse_du(&["/var/log", "/tmp"]),
            du_media_card: parse_du(&["/media/card"]),
        },
        processes: ProcessSection {
            processes: parse_ps(),
        },
    })
}

static SNAPSHOT_CACHE: OnceLock<Snapshot> = OnceLock::new();

fn cached_snapshot() -> anyhow::Result<Snapshot> {
    if let Some(snap) = SNAPSHOT_CACHE.get() {
        return Ok(snap.clone());
    }
    let snap = snapshot_inner()?;
    Ok(SNAPSHOT_CACHE.get_or_init(|| snap.clone()).clone())
}

fn summary_text_inner(snapshot: &Snapshot) -> String {
    let mut output = String::new();
    output.push_str("System State Dump\n");
    output.push_str(&format!("Collected: {}\n", snapshot.generated_at));
    output.push_str(&format!(
        "Collector version: {}\n",
        snapshot.collector_version
    ));
    output.push_str(&format!("Hostname: {}\n", snapshot.host.hostname));
    if let Some(pretty_name) = snapshot
        .host
        .os_release
        .iter()
        .find(|entry| entry.key == "PRETTY_NAME")
        .map(|entry| entry.value.clone())
    {
        output.push_str(&format!("OS: {}\n", pretty_name));
    }
    output.push_str(&format!("Uptime: {}\n", snapshot.host.uptime.pretty));
    output.push_str(&format!(
        "Load: {:.2}, {:.2}, {:.2}\n",
        snapshot.host.uptime.load_1, snapshot.host.uptime.load_5, snapshot.host.uptime.load_15
    ));
    output
}

#[rune::function]
pub fn snapshot() -> Result<Snapshot, anyhow::Error> {
    cached_snapshot()
}

#[rune::function]
pub fn serialize_snapshot(value: Value, format: Format) -> Result<String, anyhow::Error> {
    let snapshot: Snapshot = rune::from_value(value)?;
    match format {
        Format::Json => Ok(serde_json::to_string(&snapshot)?),
        Format::PrettyJson => Ok(serde_json::to_string_pretty(&snapshot)?),
        Format::Rsn => Ok(rsn::to_string(&snapshot)?),
        Format::RsnPretty => Ok(rsn::to_string_pretty(&snapshot)?),
    }
}

#[rune::function]
pub fn snapshot_to_bytes(value: Value) -> Result<rune::runtime::Bytes, anyhow::Error> {
    let snapshot: Snapshot = rune::from_value(value)?;
    let bytes = postcard::to_allocvec(&snapshot)?;
    let mut rune_vec = rune::alloc::Vec::new();
    for byte in bytes {
        rune_vec
            .try_push(byte)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(rune::runtime::Bytes::from_vec(rune_vec))
}

#[rune::function]
pub fn log_units() -> Result<Vec<String>, anyhow::Error> {
    let mut units = vec![
        "NetworkManager.service".to_string(),
        "sshd@.service".to_string(),
    ];
    for su in &cached_snapshot()?.services {
        if !units.iter().any(|existing| existing == &su.unit) {
            units.push(su.unit.clone());
        }
    }
    Ok(units)
}

#[rune::function]
pub fn resources_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_lines(
        "Resources: uptime",
        &[snapshot.resources.uptime.pretty.clone()],
    ));
    output.push('\n');
    output.push_str(&render_memory_rows(
        "Resources: memory",
        &snapshot.resources.memory,
    ));
    output.push('\n');
    output.push_str(&render_df_entries(
        "Resources: disk (root)",
        &snapshot.resources.disk_root,
    ));
    Ok(output)
}

#[rune::function]
pub fn sysinfo_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_kvs("OS release", &snapshot.host.os_release));
    output.push_str(&format!("Hostname: {}\n", snapshot.host.hostname));
    output.push_str(&format!("Uptime: {}\n\n", snapshot.host.uptime.pretty));
    output.push_str(&render_kvs("hostnamectl", &snapshot.host.hostnamectl));
    output.push_str(&render_kvs("timedatectl", &snapshot.host.timedatectl));
    output.push_str(&render_kvs("locale", &snapshot.host.locale));
    Ok(output)
}

#[rune::function]
pub fn hardware_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_memory_rows("Memory", &snapshot.hardware.memory));
    output.push('\n');
    output.push_str(&render_block_devices(
        "Block devices",
        &snapshot.hardware.block_devices,
    ));
    output.push('\n');
    output.push_str(&render_kvs("blkid", &snapshot.hardware.blkid));
    output.push('\n');
    output.push_str(&render_pci_devices(
        "PCI devices",
        &snapshot.hardware.pci_devices,
    ));
    output.push('\n');
    output.push_str(&render_usb_devices(
        "USB devices",
        &snapshot.hardware.usb_devices,
    ));
    Ok(output)
}

#[rune::function]
pub fn services_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    Ok(render_systemd_units(
        "systemctl list-units --type=service --all",
        &snapshot.services,
    ))
}

#[rune::function]
pub fn systemd_status_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_lines(
        "systemctl --failed",
        &snapshot.systemd_status.failed.lines,
    ));
    output.push_str(&render_lines(
        "systemctl list-units --state=failed",
        &snapshot.systemd_status.failed_units.lines,
    ));
    output.push_str(&render_lines(
        "systemctl list-timers --all",
        &snapshot.systemd_status.timers.lines,
    ));
    output.push_str(&render_lines(
        "systemctl list-jobs",
        &snapshot.systemd_status.jobs.lines,
    ));
    Ok(output)
}

#[rune::function]
pub fn network_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_lines("ip addr", &snapshot.network.ip_addr.lines));
    output.push_str(&render_lines("ip route", &snapshot.network.ip_route.lines));
    output.push_str(&render_lines("ip rule", &snapshot.network.ip_rule.lines));
    output.push_str(&render_lines(
        "resolvectl status",
        &snapshot.network.resolvectl_status.lines,
    ));
    output.push_str(&render_lines(
        "/etc/resolv.conf",
        &snapshot.network.resolv_conf.lines,
    ));
    Ok(output)
}

#[rune::function]
pub fn wifi_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    Ok(render_lines("nmcli dev wifi", &snapshot.wifi.lines))
}

#[rune::function]
pub fn ports_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    Ok(render_socket_entries("netstat -an", &snapshot.ports))
}

#[rune::function]
pub fn filesystems_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    let mut output = String::new();
    output.push_str(&render_mount_entries("mount", &snapshot.filesystems.mounts));
    output.push('\n');
    output.push_str(&render_lines(
        "findmnt",
        &snapshot.filesystems.findmnt.lines,
    ));
    output.push('\n');
    output.push_str(&render_block_devices(
        "lsblk",
        &snapshot.filesystems.block_devices,
    ));
    output.push('\n');
    output.push_str(&render_df_entries("df -h", &snapshot.filesystems.df));
    output.push('\n');
    output.push_str(&render_df_entries(
        "df -ih (inodes)",
        &snapshot.filesystems.df_inodes,
    ));
    output.push('\n');
    output.push_str(&render_disk_usage(
        "du -sh /var/log /tmp",
        &snapshot.filesystems.du_log_tmp,
    ));
    output.push('\n');
    output.push_str(&render_disk_usage(
        "du -h /media/card",
        &snapshot.filesystems.du_media_card,
    ));
    Ok(output)
}

#[rune::function]
pub fn processes_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    Ok(render_processes("ps aux", &snapshot.processes.processes))
}

#[rune::function]
pub fn summary_text() -> Result<String, anyhow::Error> {
    let snapshot = cached_snapshot()?;
    Ok(summary_text_inner(&snapshot))
}

// ── Rune utility functions ─────────────────────────────────────────────────

#[rune::function]
pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[rune::function]
fn os_pretty_name() -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(OS_RELEASE_PATH)?;

    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Ok(value.trim_matches('"').to_string());
        }
    }

    Ok(String::new())
}

#[rune::function]
fn uptime() -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(UPTIME_PATH)?;
    let seconds = contents
        .split_whitespace()
        .next()
        .context("missing uptime seconds")?
        .parse::<f64>()?;

    Ok(format_uptime(seconds))
}

/// Write `content` to `path` relative to the output directory.
/// Parent directories are created automatically. Rejects absolute or escaping paths.
#[rune::function]
fn write(path: String, content: String) -> Result<(), anyhow::Error> {
    let outdir = OUTDIR.get().context("output directory not initialized")?;
    let full = outdir.join(&path);
    let full_canon = full
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, anyhow::Error>(full.clone()))?;
    let outdir_canon = outdir
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, anyhow::Error>(outdir.clone()))?;
    if !full_canon.starts_with(&outdir_canon) {
        return Err(anyhow::anyhow!(
            "write path escapes output directory: {path}"
        ));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, content)?;
    Ok(())
}

/// Serialize a Rune value to postcard binary encoding.
#[rune::function]
fn to_postcard_bytes(value: rune::runtime::Value) -> Result<Bytes, anyhow::Error> {
    let raw = postcard::to_allocvec(&value)?;
    Ok(Bytes::from_slice(raw).map_err(|e| anyhow::anyhow!("{e}"))?)
}

/// Write binary `content` to `path` relative to the output directory.
/// Parent directories are created automatically. Rejects absolute or escaping paths.
#[rune::function]
fn write_bytes(path: String, content: Bytes) -> Result<(), anyhow::Error> {
    let outdir = OUTDIR.get().context("output directory not initialized")?;
    let full = outdir.join(&path);
    let full_canon = full
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, anyhow::Error>(full.clone()))?;
    let outdir_canon = outdir
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, anyhow::Error>(outdir.clone()))?;
    if !full_canon.starts_with(&outdir_canon) {
        return Err(anyhow::anyhow!(
            "write path escapes output directory: {path}"
        ));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, content.as_slice())?;
    Ok(())
}

/// Print a progress message to stderr.
#[rune::function]
fn log(msg: String) {
    eprintln!("{LOG_PREFIX} {msg}");
}

/// Return the absolute path of the output directory.
#[rune::function]
pub(crate) fn outdir() -> String {
    OUTDIR
        .get()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Return the current local time as `YYYY-MM-DD HH:MM:SS`.
#[rune::function]
fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn module() -> Result<Module, ContextError> {
    let mut m = Module::with_item([MODULE_NAME])?;
    m.ty::<Format>()?;
    m.ty::<KeyValue>()?;
    m.ty::<TextLines>()?;
    m.ty::<UptimeInfo>()?;
    m.ty::<MemoryRow>()?;
    m.ty::<DfEntry>()?;
    m.ty::<DiskUsage>()?;
    m.ty::<BlockDevice>()?;
    m.ty::<PciDevice>()?;
    m.ty::<UsbDevice>()?;
    m.ty::<Process>()?;
    m.ty::<SocketEntry>()?;
    m.ty::<SystemdUnit>()?;
    m.ty::<MountEntry>()?;
    m.ty::<HostSection>()?;
    m.ty::<ResourcesSection>()?;
    m.ty::<HardwareSection>()?;
    m.ty::<SystemdStatusSection>()?;
    m.ty::<NetworkSection>()?;
    m.ty::<FilesystemSection>()?;
    m.ty::<ProcessSection>()?;
    m.ty::<Snapshot>()?;
    m.function_meta(version)?;
    m.function_meta(hostname)?;
    m.function_meta(os_pretty_name)?;
    m.function_meta(uptime)?;
    m.function_meta(snapshot)?;
    m.function_meta(serialize_snapshot)?;
    m.function_meta(snapshot_to_bytes)?;
    m.function_meta(log_units)?;
    m.function_meta(resources_text)?;
    m.function_meta(sysinfo_text)?;
    m.function_meta(hardware_text)?;
    m.function_meta(services_text)?;
    m.function_meta(systemd_status_text)?;
    m.function_meta(network_text)?;
    m.function_meta(wifi_text)?;
    m.function_meta(ports_text)?;
    m.function_meta(filesystems_text)?;
    m.function_meta(processes_text)?;
    m.function_meta(summary_text)?;
    m.function_meta(write)?;
    m.function_meta(to_postcard_bytes)?;
    m.function_meta(write_bytes)?;
    m.function_meta(log)?;
    m.function_meta(outdir)?;
    m.function_meta(timestamp)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_hours_only() {
        assert_eq!(format_uptime(3661.0), "up 01:01:01");
    }

    #[test]
    fn format_uptime_single_day() {
        assert_eq!(format_uptime(90061.0), "up 1 day, 01:01:01");
    }

    #[test]
    fn format_uptime_multiple_days() {
        assert_eq!(format_uptime(176461.0), "up 2 days, 01:01:01");
    }

    #[test]
    fn split_n_fields_captures_remainder_in_last() {
        assert_eq!(split_n_fields("a b c d e", 3), vec!["a", "b", "c d e"]);
    }

    #[test]
    fn split_n_fields_fewer_fields_than_n() {
        assert_eq!(split_n_fields("a b", 5), vec!["a", "b"]);
    }

    #[test]
    fn split_n_fields_preserves_spaces_in_command() {
        let line = "user 123 0.5 1.0 1234 567 ? S 10:00 0:01 /usr/bin/cmd arg1 arg2";
        let fields = split_n_fields(line, 11);
        assert_eq!(fields.len(), 11);
        assert_eq!(fields[10], "/usr/bin/cmd arg1 arg2");
    }

    #[test]
    fn parse_lsblk_kv_line_extracts_all_fields() {
        let line = r#"NAME="sda" SIZE="500G" TYPE="disk" FSTYPE="" LABEL="" UUID="" MOUNTPOINT="""#;
        let pairs = parse_lsblk_kv_line(line);
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("NAME"), Some("sda"));
        assert_eq!(get("SIZE"), Some("500G"));
        assert_eq!(get("TYPE"), Some("disk"));
        assert_eq!(get("FSTYPE"), Some(""));
        assert_eq!(get("UUID"), Some(""));
    }

    #[test]
    fn key_values_equals_strips_quotes() {
        let text = "PRETTY_NAME=\"Ubuntu 22.04\"\nVERSION_ID=22.04\n";
        let kvs = key_values_equals(text);
        assert_eq!(kvs.len(), 2);
        assert_eq!(kvs[0].key, "PRETTY_NAME");
        assert_eq!(kvs[0].value, "Ubuntu 22.04");
        assert_eq!(kvs[1].key, "VERSION_ID");
        assert_eq!(kvs[1].value, "22.04");
    }

    #[test]
    fn key_values_equals_skips_empty_lines() {
        let text = "\nKEY=value\n\n";
        let kvs = key_values_equals(text);
        assert_eq!(kvs.len(), 1);
        assert_eq!(kvs[0].key, "KEY");
        assert_eq!(kvs[0].value, "value");
    }

    #[test]
    fn key_values_colon_trims_whitespace() {
        let text = " Operating System: Ubuntu 22.04\n Kernel: Linux 5.15\n";
        let kvs = key_values_colon(text);
        assert_eq!(kvs.len(), 2);
        assert_eq!(kvs[0].key, "Operating System");
        assert_eq!(kvs[0].value, "Ubuntu 22.04");
        assert_eq!(kvs[1].key, "Kernel");
        assert_eq!(kvs[1].value, "Linux 5.15");
    }

    #[test]
    fn key_values_colon_skips_lines_without_colon() {
        let text = "no colon here\nkey: value\n";
        let kvs = key_values_colon(text);
        assert_eq!(kvs.len(), 1);
        assert_eq!(kvs[0].key, "key");
    }
}
