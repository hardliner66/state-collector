use std::process::{Command, Stdio};

use anyhow::Context;
use chrono::Local;
use rune::Any;
use serde::Serialize;

#[derive(Any, Serialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Any, Serialize)]
pub struct TextLines {
    pub lines: Vec<String>,
}

#[derive(Any, Serialize)]
pub struct UptimeInfo {
    pub pretty: String,
    pub uptime_seconds: f64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
}

#[derive(Any, Serialize)]
pub struct HostSection {
    pub hostname: String,
    pub os_release: Vec<KeyValue>,
    pub uptime: UptimeInfo,
    pub hostnamectl: Vec<KeyValue>,
    pub timedatectl: Vec<KeyValue>,
    pub locale: Vec<KeyValue>,
}

#[derive(Any, Serialize)]
pub struct ResourcesSection {
    pub uptime: UptimeInfo,
    pub memory_free_h: TextLines,
    pub disk_root_df_h: TextLines,
}

#[derive(Any, Serialize)]
pub struct HardwareSection {
    pub memory_free_h: TextLines,
    pub lsblk_f: TextLines,
    pub blkid: Vec<KeyValue>,
    pub lspci: TextLines,
    pub lsusb: TextLines,
}

#[derive(Any, Serialize)]
pub struct SystemdStatusSection {
    pub services: TextLines,
    pub failed: TextLines,
    pub failed_units: TextLines,
    pub timers: TextLines,
    pub jobs: TextLines,
}

#[derive(Any, Serialize)]
pub struct NetworkSection {
    pub ip_addr: TextLines,
    pub ip_route: TextLines,
    pub ip_rule: TextLines,
    pub resolvectl_status: TextLines,
    pub resolv_conf: TextLines,
}

#[derive(Any, Serialize)]
pub struct FilesystemSection {
    pub mount: TextLines,
    pub findmnt: TextLines,
    pub lsblk: TextLines,
    pub df_h: TextLines,
    pub df_ih: TextLines,
    pub du_log_tmp: TextLines,
    pub du_media_card: TextLines,
}

#[derive(Any, Serialize)]
pub struct ProcessSection {
    pub ps_aux: TextLines,
}

#[derive(Any, Serialize)]
pub struct Snapshot {
    pub generated_at: String,
    pub collector_version: String,
    pub outdir: String,
    pub host: HostSection,
    pub resources: ResourcesSection,
    pub hardware: HardwareSection,
    pub services: TextLines,
    pub systemd_status: SystemdStatusSection,
    pub network: NetworkSection,
    pub wifi: TextLines,
    pub ports: TextLines,
    pub filesystems: FilesystemSection,
    pub processes: ProcessSection,
}

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

fn format_uptime(seconds: f64) -> String {
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

fn hostname() -> anyhow::Result<String> {
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

fn snapshot_inner() -> anyhow::Result<Snapshot> {
    Ok(Snapshot {
        generated_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        collector_version: env!("CARGO_PKG_VERSION").to_string(),
        outdir: std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .into_owned(),
        host: HostSection {
            hostname: hostname()?,
            os_release: key_values_equals(&std::fs::read_to_string("/etc/os-release")?),
            uptime: uptime_info()?,
            hostnamectl: kv_or_error_colon("hostnamectl", &[]),
            timedatectl: kv_or_error_colon("timedatectl", &[]),
            locale: kv_or_error_equals("locale", &[]),
        },
        resources: ResourcesSection {
            uptime: uptime_info()?,
            memory_free_h: text_or_error("free", &["-h"]),
            disk_root_df_h: text_or_error("df", &["-h", "/"]),
        },
        hardware: HardwareSection {
            memory_free_h: text_or_error("free", &["-h"]),
            lsblk_f: text_or_error("lsblk", &["-f"]),
            blkid: kv_or_error_equals("blkid", &[]),
            lspci: text_or_error("lspci", &[]),
            lsusb: text_or_error("lsusb", &[]),
        },
        services: text_or_error(
            "systemctl",
            &["list-units", "--type=service", "--all", "--no-pager"],
        ),
        systemd_status: SystemdStatusSection {
            services: text_or_error(
                "systemctl",
                &["list-units", "--type=service", "--all", "--no-pager"],
            ),
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
        ports: text_or_error("netstat", &["-an"]),
        filesystems: FilesystemSection {
            mount: text_or_error("mount", &[]),
            findmnt: text_or_error("findmnt", &[]),
            lsblk: text_or_error("lsblk", &[]),
            df_h: text_or_error("df", &["-h"]),
            df_ih: text_or_error("df", &["-ih"]),
            du_log_tmp: text_or_error("du", &["-sh", "/var/log", "/tmp"]),
            du_media_card: text_or_error("du", &["-h", "/media/card"]),
        },
        processes: ProcessSection {
            ps_aux: text_or_error("ps", &["aux"]),
        },
    })
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
    snapshot_inner()
}

#[rune::function]
pub fn snapshot_json() -> Result<String, anyhow::Error> {
    let snap = snapshot_inner()?;
    Ok(serde_json::to_string_pretty(&snap)?)
}

#[rune::function]
pub fn snapshot_bytes() -> Result<rune::runtime::Bytes, anyhow::Error> {
    let snap = snapshot_inner()?;
    let bytes = postcard::to_allocvec(&snap)?;
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
    let output = run_command(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--all",
            "--plain",
            "--no-legend",
            "--no-pager",
        ],
    )?;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let unit = line.split_whitespace().next().unwrap_or("");
        if !unit.is_empty() && !units.iter().any(|existing| existing == unit) {
            units.push(unit.to_string());
        }
    }

    Ok(units)
}

#[rune::function]
pub fn resources_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    Ok(format!(
        "{}\n{}\n{}",
        render_lines(
            "Resources: uptime",
            &[snapshot.resources.uptime.pretty.clone()]
        ),
        render_lines(
            "Resources: memory_free_h",
            &snapshot.resources.memory_free_h.lines
        ),
        render_lines(
            "Resources: disk_root_df_h",
            &snapshot.resources.disk_root_df_h.lines
        ),
    ))
}

#[rune::function]
pub fn sysinfo_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
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
    let snapshot = snapshot_inner()?;
    let mut output = String::new();
    output.push_str(&render_lines(
        "free -h",
        &snapshot.hardware.memory_free_h.lines,
    ));
    output.push_str(&render_lines("lsblk -f", &snapshot.hardware.lsblk_f.lines));
    output.push_str(&render_kvs("blkid", &snapshot.hardware.blkid));
    output.push_str(&render_lines("lspci", &snapshot.hardware.lspci.lines));
    output.push_str(&render_lines("lsusb", &snapshot.hardware.lsusb.lines));
    Ok(output)
}

#[rune::function]
pub fn services_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    Ok(render_lines(
        "systemctl list-units --type=service --all --no-pager",
        &snapshot.services.lines,
    ))
}

#[rune::function]
pub fn systemd_status_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
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
    let snapshot = snapshot_inner()?;
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
    let snapshot = snapshot_inner()?;
    Ok(render_lines("nmcli dev wifi", &snapshot.wifi.lines))
}

#[rune::function]
pub fn ports_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    Ok(render_lines("netstat -an", &snapshot.ports.lines))
}

#[rune::function]
pub fn filesystems_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    let mut output = String::new();
    output.push_str(&render_lines("mount", &snapshot.filesystems.mount.lines));
    output.push_str(&render_lines(
        "findmnt",
        &snapshot.filesystems.findmnt.lines,
    ));
    output.push_str(&render_lines("lsblk", &snapshot.filesystems.lsblk.lines));
    output.push_str(&render_lines("df -h", &snapshot.filesystems.df_h.lines));
    output.push_str(&render_lines("df -ih", &snapshot.filesystems.df_ih.lines));
    output.push_str(&render_lines(
        "du -sh /var/log /tmp",
        &snapshot.filesystems.du_log_tmp.lines,
    ));
    output.push_str(&render_lines(
        "du -h /media/card",
        &snapshot.filesystems.du_media_card.lines,
    ));
    Ok(output)
}

#[rune::function]
pub fn processes_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    Ok(render_lines("ps aux", &snapshot.processes.ps_aux.lines))
}

#[rune::function]
pub fn summary_text() -> Result<String, anyhow::Error> {
    let snapshot = snapshot_inner()?;
    Ok(summary_text_inner(&snapshot))
}
