pub const MODULE_NAME: &str = match option_env!("STATE_COLLECTOR_MODULE_NAME") {
    Some(name) => name,
    None => "sc",
};

pub const LOG_PREFIX: &str = match option_env!("STATE_COLLECTOR_LOG_PREFIX") {
    Some(name) => name,
    None => "[collector]",
};

pub const ARCHIVE_EXT: &str = match option_env!("STATE_COLLECTOR_ARCHIVE_EXT") {
    Some(ext) => ext,
    None => "sc",
};

pub const ARCHIVE_PREFIX: &str = match option_env!("STATE_COLLECTOR_ARCHIVE_PREFIX") {
    Some(prefix) => prefix,
    None => "system-info",
};

pub const DEFAULT_SCRIPT_BASIC: &str = include_str!("../default-scripts/basic.rn");
pub const DEFAULT_SCRIPT_JSON: &str = include_str!("../default-scripts/json.rn");
pub const DEFAULT_SCRIPT_BINARY: &str = include_str!("../default-scripts/binary.rn");

pub const OS_RELEASE_PATH: &str = "/etc/os-release";
pub const UPTIME_PATH: &str = "/proc/uptime";
