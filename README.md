# state-collector

`state-collector` is a tool to collect system state, writes the collected files into a temporary directory, and then packages that directory into a compressed archive.

## Why Does This Exist?
That's a really good question.

Over the years of working with embedded devices and servers I realized that most places have a system in place that they can use to retrieve data about the system, like logs or which services are running. Depending on the specific use case, that can range from a bash script the admin uploads in case something crashes, up to fully automated log and metric collection services.

The state collectors is not meant to replace the fully automatic collectors in highly connected scenarios, but rather be a tool that helps working with devices that might not always be online or need more flexible ways to retrieve data.

The idea is to create a system that allows you to focus on what you're trying to do and not on how to actually do it. For this reason, the actual collection logic is defined externally in the form of a rune script. This lets you easily customize what data is collected.

_The script engine does not yet proved an extensive set of APIs to help with that!_


## Typical Use Cases

- Device or fleet troubleshooting: capture logs and system state from systems in the field.
- Support bundles: produce one archive to share with developers or support teams.
- Incident response snapshots: preserve command output and service status at a point in time.
- Custom diagnostics profiles: use different Rune scripts for different products or environments.

## How It Works

`state-collector`:

- Compiles and executes a Rune script.
- Expects the script to expose `pub async fn collect()`.
- Makes a module available to scripts (default module name: `sc`) with helper functions:
  - `sc::version()`
  - `sc::hostname()`
  - `sc::os_pretty_name()`
  - `sc::uptime()`
  - `sc::snapshot()`
  - `sc::log_units()`
  - `sc::resources_text()`
  - `sc::sysinfo_text()`
  - `sc::hardware_text()`
  - `sc::services_text()`
  - `sc::systemd_status_text()`
  - `sc::network_text()`
  - `sc::wifi_text()`
  - `sc::ports_text()`
  - `sc::filesystems_text()`
  - `sc::processes_text()`
  - `sc::summary_text()`
  - `sc::write(path, content)`
  - `sc::to_postcard_bytes(value)`
  - `sc::write_bytes(path, content)`
  - `sc::log(message)`
  - `sc::outdir()`
  - `sc::timestamp()`
- Some common summary values are read directly from host state rather than parsed from command output.
- Archives the staging directory as a gzip-compressed tar file (default extension: `.sc`).

`sc-unpack`:

- Opens one or more `.sc` archives.
- Unpacks each archive into a directory named after the archive stem (or under `--output` when provided).

## Build

```bash
cargo build
```

For optimized binaries:

```bash
cargo build --release
```

## Usage

### Collect with default script

Runs built-in `examples/basic.rn`:

```bash
cargo run --release --
```

### Collect with a specific Rune script

```bash
cargo run --release -- examples/json.rn
```

### Write to a custom output archive path

```bash
cargo run --release -- examples/basic.rn --output /tmp/snapshot.sc
```

Short flag works too:

```bash
cargo run --release -- examples/basic.rn -o /tmp/snapshot.sc
```

### Unpack archive(s)

```bash
cargo run --release --bin sc-unpack -- /tmp/snapshot.sc
```

Unpack multiple archives:

```bash
cargo run --release --bin sc-unpack -- tmp/basic.sc tmp/json.sc
```

Set a custom destination root:

```bash
cargo run --release --bin sc-unpack -- /tmp/snapshot.sc --output /tmp/unpacked
```

## Example Scripts

The repository includes:

- `examples/basic.rn`: writes text-based diagnostic files.
- `examples/json.rn`: writes structured JSON state to `state.json`.
- `examples/binary.rn`: writes postcard-encoded binary state to `state.bin`.

You can run all examples via:

```bash
./run_all.sh
```

That script writes archives into `tmp/` and unpacks each one for quick inspection.

## Writing Your Own Rune Script

Minimum shape:

```rune
pub async fn collect() {
    sc::log("collecting...");
    sc::write("hello.txt", "hello world")?;
}
```

Notes:

- The entrypoint must be `pub async fn collect()`.
- Relative write paths are rooted at the collector staging directory.
- Parent directories are created automatically when using `sc::write` and `sc::write_bytes`.

## Runtime Expectations

The bundled examples invoke system commands such as `systemctl`, `journalctl`, `ip`, `df`, and others. Depending on your target environment:

- Some commands may be missing.
- Some commands may require elevated privileges.
- Command output can differ across distros and OS versions.

The example scripts are written to tolerate failures for some optional probes and continue collecting.

## Configuration via Build-Time Environment Variables

The binary supports these optional compile-time environment variables:

- `STATE_COLLECTOR_MODULE_NAME` (default: `sc`)
- `STATE_COLLECTOR_LOG_PREFIX` (default: `[collector]`)
- `STATE_COLLECTOR_ARCHIVE_EXT` (default: `sc`)
- `STATE_COLLECTOR_ARCHIVE_PREFIX` (default: `system-info`)

If unset, defaults are used.

## Output Naming

If `--output` is not provided, output defaults to:

- Current directory
- Filename pattern: `<executable-stem>-<YYYYMMDD_HHMMSS>.<ext>`

Example:

- `state-collector-20260530_153000.sc`

## Project Layout

- `src/main.rs`: main collector CLI.
- `src/bin/sc-unpack.rs`: archive unpacker CLI.
- `examples/*.rn`: sample Rune collection scripts.
- `run_all.sh`: helper script that runs all examples and unpacks results.
