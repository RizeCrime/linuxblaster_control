//! PipeWire system-output integration for the G6.
//!
//! The G6 exposes two useful outputs:
//! * its ALSA "pro" sink for direct stereo playback, and
//! * a virtual 7.1 sink backed by the vendored Creative MatrixEncoder
//!   LADSPA plugin (MIT, `vendor/g6ct`).
//!
//! The virtual sink receives the 7.1 PCM and downmixes it with a 1:1 code
//! port of the Sound BlasterX G6 downmixer APO (`CMatrixEncoderEfxMod` from
//! `KSUSBAPO64.dll`). The port runs in f64 instead of the original f32 —
//! the only difference.
//!
//! This module only shells out to `pactl` and `systemctl`; it never
//! processes audio. `build.rs` compiles the plugin into the binary; the
//! installer writes it to `~/.local/lib/linuxblaster/` and generates a
//! PipeWire filter-chain config for the G6 sink that is actually present.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::warn;

use crate::DEFAULT_BASE_PATH;

pub const VIRTUAL_SINK_NAME: &str = "g6_ct_downmix";
pub const VIRTUAL_SINK_DESCRIPTION: &str = "G6 Creative MatrixEncoder 7.1";
pub const PLUGIN_FILE: &str = "libg6ct_ladspa_f64.so";
pub const PLUGIN_FALLBACK_FILE: &str = "libg6ct_ladspa.so";

/// The f64 LADSPA plugin built from `vendor/g6ct` by `build.rs`. Carrying
/// the bytes in the binary means installation needs neither a source tree
/// nor a download.
static EMBEDDED_PLUGIN: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/libg6ct_ladspa_f64.so"));

pub fn embedded_plugin() -> &'static [u8] {
    EMBEDDED_PLUGIN
}

/// Files installed by the optional cold-boot helper. `packaging/coldboot/`
/// ships the same set for a terminal install (`install.sh`).
pub const COLD_BOOT_SCRIPT: &str = "/usr/local/libexec/g6-coldboot-prep.sh";
pub const COLD_BOOT_UNIT: &str =
    "/etc/systemd/system/g6-coldboot-prep.service";
pub const COLD_BOOT_RULE: &str = "/etc/udev/rules.d/71-g6-coldboot.rules";
pub const COLD_BOOT_CONF: &str = "/etc/modprobe.d/99-g6-coldboot.conf";

/// `(file name, contents, destination)`, embedded so the button works
/// without a source checkout or a package that ships the helper separately.
const COLD_BOOT_FILES: [(&str, &str, &str); 4] = [
    (
        "g6-coldboot-prep.sh",
        include_str!("../packaging/coldboot/g6-coldboot-prep.sh"),
        COLD_BOOT_SCRIPT,
    ),
    (
        "g6-coldboot-prep.service",
        include_str!("../packaging/coldboot/g6-coldboot-prep.service"),
        COLD_BOOT_UNIT,
    ),
    (
        "71-g6-coldboot.rules",
        include_str!("../packaging/coldboot/71-g6-coldboot.rules"),
        COLD_BOOT_RULE,
    ),
    (
        "99-g6-coldboot.conf",
        include_str!("../packaging/coldboot/99-g6-coldboot.conf"),
        COLD_BOOT_CONF,
    ),
];

/// What the system pane reports about the optional cold-boot helper.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColdBoot {
    pub installed: bool,
    /// The reset and the HID ping shell out to python3.
    pub python: bool,
}

impl ColdBoot {
    fn detect() -> Self {
        Self {
            installed: COLD_BOOT_FILES
                .iter()
                .all(|(_, _, destination)| Path::new(destination).exists()),
            python: Path::new("/usr/bin/python3").exists(),
        }
    }
}

/// Installs the cold-boot helper with a graphical password prompt.
///
/// The four files are written to a private temporary directory and then
/// copied into place by one `pkexec` call, so there is a single prompt and
/// no half-installed state on cancel.
pub fn install_cold_boot_helper() -> Result<String, String> {
    let directory = std::env::temp_dir().join(format!(
        "linuxblaster-coldboot-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).map_err(|error| {
        format!("Couldn't create {}: {}", directory.display(), error)
    })?;
    // Only this user may write here; root can still read it for the install.
    // Without this, another local user could swap a file between the write
    // and the pkexec copy.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| {
                format!(
                    "Couldn't restrict {}: {}",
                    directory.display(),
                    error
                )
            })?;
    }

    for (name, contents, _) in COLD_BOOT_FILES {
        let path = directory.join(name);
        fs::write(&path, contents).map_err(|error| {
            format!("Couldn't write {}: {}", path.display(), error)
        })?;
    }

    // One shell, one prompt: install all four files, then reload udev and
    // systemd. "set -e" stops before the reloads if a copy fails.
    let script = r#"
set -e
install -Dm755 "$1/g6-coldboot-prep.sh" "/usr/local/libexec/g6-coldboot-prep.sh"
install -Dm644 "$1/g6-coldboot-prep.service" "/etc/systemd/system/g6-coldboot-prep.service"
install -Dm644 "$1/71-g6-coldboot.rules" "/etc/udev/rules.d/71-g6-coldboot.rules"
install -Dm644 "$1/99-g6-coldboot.conf" "/etc/modprobe.d/99-g6-coldboot.conf"
udevadm control --reload-rules
systemctl daemon-reload
"#;

    let status = std::process::Command::new("pkexec")
        .args(["sh", "-c", script, "sh"])
        .arg(&directory)
        .status();
    let _ = fs::remove_dir_all(&directory);

    match status {
        Ok(status) if status.success() => Ok(
            "Cold-boot helper installed. It runs on the next cold boot, when \
             the G6 appears on USB."
                .to_owned(),
        ),
        // pkexec: 126 = dialog dismissed, 127 = not authorized
        Ok(status) if status.code() == Some(126) => {
            Err("The password prompt was cancelled.".to_owned())
        }
        Ok(status) if status.code() == Some(127) => {
            Err("The password prompt was not authorized.".to_owned())
        }
        Ok(status) => Err(format!("Installing the helper failed ({}).", status)),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(
            "pkexec isn't installed, so the password prompt can't be shown. \
             Run packaging/coldboot/install.sh as root instead."
                .to_owned(),
        ),
        Err(error) => Err(format!("Couldn't start pkexec: {}", error)),
    }
}

/// User settings owned by the System pane. Kept separately from the G6
/// device snapshot so the device JSON stays compatible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Let the active PipeWire sink be the single user volume control.
    pub sync_volume: bool,
    /// Cap the G6 hardware Speaker volume at -2.00 dB (raw 124/128).
    pub sinad_fix: bool,
}

impl Settings {
    /// `~/.local/share/linuxblaster/system.json`
    pub fn path() -> PathBuf {
        DEFAULT_BASE_PATH.join("system.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|error| {
                warn!("Ignoring unreadable system settings: {}", error);
                Self::default()
            }),
            Err(error) if error.kind() == ErrorKind::NotFound => Self::default(),
            Err(error) => {
                warn!("Couldn't read {}: {}", path.display(), error);
                Self::default()
            }
        }
    }

    /// Saves atomically, so a crash cannot leave a truncated settings file.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("Couldn't create {}: {}", parent.display(), error)
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Couldn't encode settings: {}", error))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json).map_err(|error| {
            format!("Couldn't write {}: {}", temporary.display(), error)
        })?;
        fs::rename(&temporary, &path).map_err(|error| {
            format!("Couldn't replace {}: {}", path.display(), error)
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sink {
    pub index: u32,
    pub name: String,
    pub driver: String,
    pub spec: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkInput {
    pub id: u32,
    pub sink: u32,
    pub node_name: Option<String>,
    pub node_group: Option<String>,
}

impl SinkInput {
    /// The virtual sink's own filter-chain output is a sink input on the
    /// G6 Pro sink. Moving it would create a loop, so stream migration
    /// must leave it where PipeWire created it.
    fn is_filter_chain_output(&self) -> bool {
        self.node_name
            .as_deref()
            .is_some_and(|name| {
                name == format!("{}.output", VIRTUAL_SINK_NAME)
            })
            || self
                .node_group
                .as_deref()
                .is_some_and(|group| group.starts_with("filter-chain-"))
    }
}

/// A snapshot of the system output state for the UI. Refreshing it runs
/// `pactl`; callers should cache it rather than calling this per frame.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub default_sink: Option<String>,
    pub g6_sink: Option<String>,
    pub virtual_installed: bool,
    pub config_present: bool,
    pub plugin: Option<PathBuf>,
    pub virtual_volume: Option<f32>,
    pub g6_volume: Option<f32>,
    pub active_volume: Option<f32>,
    pub cold_boot: ColdBoot,
    pub error: Option<String>,
}

impl Status {
    /// The default sink, but only when it is one of the two outputs this
    /// app manages. Another default (HDMI, onboard analog) returns `None`.
    pub fn active_sink(&self) -> Option<&str> {
        let name = self.default_sink.as_deref()?;
        if name == VIRTUAL_SINK_NAME
            || Some(name) == self.g6_sink.as_deref()
        {
            Some(name)
        } else {
            None
        }
    }

    pub fn refresh() -> Self {
        let plugin = find_plugin();
        let config_present = config_path().is_file();
        let cold_boot = ColdBoot::detect();

        let sinks = match list_sinks() {
            Ok(sinks) => sinks,
            Err(error) => {
                return Self {
                    config_present,
                    plugin,
                    cold_boot,
                    error: Some(error),
                    ..Self::default()
                };
            }
        };

        let (default_sink, mut error) = match default_sink() {
            Ok(default) => (default, None),
            Err(error) => (None, Some(error)),
        };

        let g6_sink =
            find_g6_sink(&sinks).map(|sink| sink.name.clone());
        let virtual_installed =
            sinks.iter().any(|sink| sink.name == VIRTUAL_SINK_NAME);
        let virtual_volume = if virtual_installed {
            sink_volume(VIRTUAL_SINK_NAME).ok()
        } else {
            None
        };
        let g6_volume = g6_sink
            .as_deref()
            .and_then(|name| sink_volume(name).ok());

        let active_volume = match default_sink.as_deref() {
            Some(VIRTUAL_SINK_NAME) => virtual_volume,
            Some(name) if Some(name) == g6_sink.as_deref() => g6_volume,
            _ => None,
        };

        if error.is_none() {
            // A missing G6 is not an error by itself; the pane explains it.
            if g6_sink.is_none() && virtual_volume.is_some() {
                error = Some(
                    "The G6 Pro sink is missing; the virtual sink has no target."
                        .to_owned(),
                );
            }
        }

        Self {
            default_sink,
            g6_sink,
            virtual_installed,
            config_present,
            plugin,
            virtual_volume,
            g6_volume,
            active_volume,
            cold_boot,
            error,
        }
    }
}

/// The per-user PipeWire drop-in that creates the virtual sink.
pub fn config_path() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from(".config"));
    config_home.join("pipewire/pipewire.conf.d/99-g6-ct-downmix.conf")
}

/// Stable location for the locally built/copied plugin.
pub fn plugin_install_path() -> PathBuf {
    home_dir()
        .join(".local/lib/linuxblaster")
        .join(PLUGIN_FILE)
}

/// The plugin currently available on this machine. The installed copy
/// comes first; the old g6-re paths stay as a fallback for existing setups.
pub fn find_plugin() -> Option<PathBuf> {
    let home = home_dir();
    let candidates = [
        plugin_install_path(),
        home.join("g6-re/port").join(PLUGIN_FILE),
        home.join("g6/port").join(PLUGIN_FILE),
        home.join(".local/lib/linuxblaster").join(PLUGIN_FALLBACK_FILE),
        home.join("g6-re/port").join(PLUGIN_FALLBACK_FILE),
        home.join("g6/port").join(PLUGIN_FALLBACK_FILE),
        PathBuf::from("/usr/local/lib/linuxblaster").join(PLUGIN_FILE),
        PathBuf::from("/usr/lib/linuxblaster").join(PLUGIN_FILE),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

pub fn is_g6_pro_sink(name: &str) -> bool {
    name.starts_with("alsa_output.usb-")
        && name.contains("Sound_BlasterX_G6")
        && name.contains("pro-output")
}

pub fn find_g6_sink(sinks: &[Sink]) -> Option<&Sink> {
    sinks.iter().find(|sink| is_g6_pro_sink(&sink.name))
}

/// Lists PipeWire/Pulse sinks. Parsing `list short sinks` keeps this
/// independent of the JSON format version.
pub fn list_sinks() -> Result<Vec<Sink>, String> {
    let output = pactl(&["list", "short", "sinks"])?;
    Ok(parse_short_sinks(&output))
}

fn parse_short_sinks(output: &str) -> Vec<Sink> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim_end().split('\t');
            let index = fields.next()?.trim().parse().ok()?;
            let name = fields.next()?.trim().to_owned();
            if name.is_empty() {
                return None;
            }
            Some(Sink {
                index,
                name,
                driver: fields.next().unwrap_or("").to_owned(),
                spec: fields.next().unwrap_or("").to_owned(),
                state: fields.next().unwrap_or("").to_owned(),
            })
        })
        .collect()
}

pub fn default_sink() -> Result<Option<String>, String> {
    let output = pactl(&["get-default-sink"])?;
    let name = output.trim();
    if name.is_empty() || name == "unknown" {
        Ok(None)
    } else {
        Ok(Some(name.to_owned()))
    }
}

pub fn set_default_sink(name: &str) -> Result<(), String> {
    pactl(&["set-default-sink", name]).map(|_| ())
}

pub fn sink_volume(name: &str) -> Result<f32, String> {
    let output = pactl(&["get-sink-volume", name])?;
    parse_sink_volume_percent(&output)
        .ok_or_else(|| format!("Couldn't parse the volume of {}", name))
}

fn parse_sink_volume_percent(output: &str) -> Option<f32> {
    for token in output.split_whitespace() {
        if let Some(number) = token.strip_suffix('%')
            && let Ok(value) = number.parse::<f32>()
        {
            return Some(value);
        }
    }
    None
}

pub fn set_sink_volume(name: &str, percent: f32) -> Result<(), String> {
    let percent = percent.clamp(0.0, 100.0);
    let value = format!("{:.0}%", percent);
    pactl(&["set-sink-volume", name, &value]).map(|_| ())
}

fn parse_sink_inputs(output: &str) -> Vec<SinkInput> {
    let mut inputs = Vec::new();
    let mut current: Option<SinkInput> = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Sink Input #") {
            if let Some(input) = current.take() {
                inputs.push(input);
            }
            if let Ok(id) = rest.trim().parse() {
                current = Some(SinkInput {
                    id,
                    sink: 0,
                    node_name: None,
                    node_group: None,
                });
            }
            continue;
        }

        let Some(input) = current.as_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("Sink:") {
            if let Ok(sink) = rest.trim().parse() {
                input.sink = sink;
            }
        } else if let Some(rest) = line.strip_prefix("node.name =") {
            input.node_name = parse_property_value(rest);
        } else if let Some(rest) = line.strip_prefix("node.group =") {
            input.node_group = parse_property_value(rest);
        }
    }

    if let Some(input) = current {
        inputs.push(input);
    }
    inputs
}

fn parse_property_value(rest: &str) -> Option<String> {
    let value = rest.trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_owned())
}

/// Moves every application stream on `source` to `target`. The filter
/// chain's own output is skipped; moving it would feed the virtual sink
/// into itself.
pub fn move_streams(source: &str, target: &str) -> Result<usize, String> {
    let sinks = list_sinks()?;
    let Some(source_sink) =
        sinks.iter().find(|sink| sink.name == source)
    else {
        return Ok(0);
    };

    let output = pactl(&["list", "sink-inputs"])?;
    let mut moved = 0;
    for input in parse_sink_inputs(&output) {
        if input.sink != source_sink.index
            || input.is_filter_chain_output()
        {
            continue;
        }
        pactl(&["move-sink-input", &input.id.to_string(), target])?;
        moved += 1;
    }
    Ok(moved)
}

/// Switches the default output between the G6 Pro sink and the virtual
/// sink. With `preserve_volume`, the new active sink inherits the old
/// active volume and the other gain stage is pinned to 100% so only one
/// stage attenuates.
pub fn switch_output(use_virtual: bool, preserve_volume: bool) -> Result<(), String> {
    let sinks = list_sinks()?;
    let g6 = find_g6_sink(&sinks)
        .ok_or_else(|| {
            "The G6 Pro sink is not available. Is the G6 connected?".to_owned()
        })?
        .name
        .clone();

    if use_virtual
        && !sinks.iter().any(|sink| sink.name == VIRTUAL_SINK_NAME)
    {
        return Err(
            "The virtual 7.1 sink is not installed. Run the installer first."
                .to_owned(),
        );
    }

    let target = if use_virtual { VIRTUAL_SINK_NAME } else { &g6 };
    let current = default_sink()?;
    if current.as_deref() == Some(target) {
        return Ok(());
    }

    if preserve_volume {
        let source = if use_virtual { g6.as_str() } else { VIRTUAL_SINK_NAME };
        let source_was_active = current.as_deref() == Some(source);
        if source_was_active
            && let Ok(volume) = sink_volume(source)
        {
            set_sink_volume(target, volume)?;
        }
    }

    set_default_sink(target)?;
    for source in [g6.as_str(), VIRTUAL_SINK_NAME] {
        if source != target
            && let Err(error) = move_streams(source, target)
        {
            warn!("Couldn't move every stream from {}: {}", source, error);
        }
    }

    if preserve_volume {
        if use_virtual {
            // The virtual sink's filter-chain output goes into the G6 Pro
            // sink; that stage must not attenuate as well. Do this after
            // moving streams so direct playback does not jump first.
            set_sink_volume(&g6, 100.0)?;
        } else {
            set_sink_volume(VIRTUAL_SINK_NAME, 100.0).ok();
        }
    }
    Ok(())
}

/// Writes the bundled plugin to its stable path, writes the PipeWire
/// drop-in for the current G6 sink, and reloads PipeWire.
///
/// The plugin is part of this MIT-licensed project and is compiled into
/// the binary, so installation needs neither a source tree nor a download.
pub fn install_virtual_sink() -> Result<String, String> {
    let plugin = ensure_plugin()?;

    let sinks = list_sinks()?;
    let g6 = find_g6_sink(&sinks)
        .ok_or_else(|| {
            "The G6 Pro sink is not available. Connect the G6 first."
                .to_owned()
        })?
        .name
        .clone();

    let old_default = default_sink().ok().flatten();
    let was_virtual = old_default.as_deref() == Some(VIRTUAL_SINK_NAME);
    let virtual_volume = if was_virtual {
        sink_volume(VIRTUAL_SINK_NAME).ok()
    } else {
        None
    };
    let g6_volume = sink_volume(&g6).ok();

    let config = config_contents(&plugin, &g6);
    write_atomic(&config_path(), &config)?;

    restart_pipewire()?;
    wait_for_sink(VIRTUAL_SINK_NAME, Duration::from_secs(10))?;
    // The G6 re-enumerates after the restart and can take longer than the
    // filter-chain node. Wait for it before restoring the previous default:
    // otherwise the restore is skipped and PipeWire re-applies its stored
    // default, which is usually the virtual sink.
    let _ = wait_for_sink(&g6, Duration::from_secs(5));

    // Keep the user's previous default. A full PipeWire restart can
    // otherwise leave the virtual sink or the G6 sink selected by itself.
    let after = list_sinks()?;
    match old_default.as_deref() {
        Some(VIRTUAL_SINK_NAME) => {
            set_default_sink(VIRTUAL_SINK_NAME)?;
            if let Some(volume) = virtual_volume {
                let _ = set_sink_volume(VIRTUAL_SINK_NAME, volume);
            }
            // The filter-chain output goes into the G6 Pro sink.
            let _ = set_sink_volume(&g6, 100.0);
        }
        Some(name) if after.iter().any(|sink| sink.name == name) => {
            let _ = set_default_sink(name);
            if let Some(volume) = g6_volume {
                let _ = set_sink_volume(&g6, volume);
            }
        }
        _ => {
            if let Some(volume) = g6_volume {
                let _ = set_sink_volume(&g6, volume);
            }
        }
    }

    Ok(format!(
        "Installed the virtual 7.1 sink.\nPlugin: {}\nConfig: {}\nTarget: {}",
        plugin.display(),
        config_path().display(),
        g6
    ))
}

fn ensure_plugin() -> Result<PathBuf, String> {
    let destination = plugin_install_path();
    let current = fs::read(&destination)
        .map(|existing| existing == EMBEDDED_PLUGIN)
        .unwrap_or(false);
    if current {
        return Ok(destination);
    }
    write_atomic_bytes(&destination, EMBEDDED_PLUGIN)?;
    Ok(destination)
}

/// The generated PipeWire filter-chain drop-in. Kept as a pure function
/// so the text contract is unit-testable without touching the system.
pub fn config_contents(plugin: &Path, target: &str) -> String {
    let plugin = escape_pipewire_string(&plugin.display().to_string());
    let target = escape_pipewire_string(target);
    format!(
        r#"# Sound BlasterX G6 - Creative "MatrixEncoder" 7.1 -> binaural stereo.
#
# Generated by Linuxblaster Control's System pane.
# The LADSPA plugin is part of this project (MIT) and is bundled with
# the app; the installer writes it to ~/.local/lib/linuxblaster/.
#
# 7.1 PCM arrives at this sink and is downmixed by a 1:1 code port of the
# Sound BlasterX G6 downmixer APO (CMatrixEncoderEfxMod, KSUSBAPO64.dll).
# The port runs in f64 instead of the original f32 - the only difference.
#
#   frontL = FL + 0.707*FC + 0.707*LFE
#   frontR = FR + 0.707*FC + 0.707*LFE
#   surL   = 0.786*BL + 0.619*BR + 0.875*SL + 0.485*SR
#   surR   = 0.619*BL + 0.786*BR + 0.485*SL + 0.875*SR
context.modules = [
    {{ name = libpipewire-module-filter-chain
        flags = [ nofail ]
        args = {{
            node.description = "{description}"
            media.name       = "{description}"
            filter.graph = {{
                nodes = [
                    {{ type = ladspa
                      name = enc
                      plugin = "{plugin}"
                      label = "g6ct_matrix_encoder"
                      control = {{ "Channel Mask" = 1599 }} }}
                ]
                inputs  = [ "enc:FL" "enc:FR" "enc:FC" "enc:LFE" "enc:BL" "enc:BR" "enc:SL" "enc:SR" ]
                outputs = [ "enc:OutL" "enc:OutR" ]
            }}
            capture.props = {{
                node.name        = "{sink}"
                node.description = "{description}"
                media.class      = Audio/Sink
                audio.channels   = 8
                audio.position   = [ FL FR FC LFE RL RR SL SR ]
            }}
            playback.props = {{
                node.name        = "{sink}.output"
                node.description = "{description} output"
                node.passive     = true
                audio.channels   = 2
                audio.position   = [ FL FR ]
                target.object    = "{target}"
            }}
        }}
    }}
]
"#,
        description = VIRTUAL_SINK_DESCRIPTION,
        sink = VIRTUAL_SINK_NAME,
        plugin = plugin,
        target = target,
    )
}

fn escape_pipewire_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    write_atomic_bytes(path, contents.as_bytes())
}

fn write_atomic_bytes(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("Couldn't create {}: {}", parent.display(), error)
        })?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents).map_err(|error| {
        format!("Couldn't write {}: {}", temporary.display(), error)
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        format!("Couldn't replace {}: {}", path.display(), error)
    })?;
    Ok(())
}

fn restart_pipewire() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["--user", "restart", "pipewire.service"])
        .output()
        .map_err(|error| {
            format!(
                "Couldn't run systemctl --user restart pipewire.service: {}",
                error
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "PipeWire could not be restarted: {}",
            stderr.trim()
        ));
    }
    Ok(())
}

fn wait_for_sink(name: &str, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if list_sinks()
            .map(|sinks| sinks.iter().any(|sink| sink.name == name))
            .unwrap_or(false)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "PipeWire restarted, but the {} sink did not appear. Check the \
         PipeWire journal and the plugin path in {}.",
        name,
        config_path().display()
    ))
}

fn pactl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| format!("Couldn't run pactl: {}", error))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            format!("pactl {} failed ({})", args.join(" "), output.status)
        } else {
            format!("pactl {} failed: {}", args.join(" "), stderr)
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT_SINKS: &str = "36\tg6_ct_downmix\tPipeWire\tfloat32le 8ch 48000Hz\tSUSPENDED\n\
72\talsa_output.pci-0000_06_00.1.hdmi-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n\
73\talsa_output.usb-Creative_Technology_Ltd_Sound_BlasterX_G6_6D00664763X-00.pro-output-0\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n";

    #[test]
    fn parses_short_sink_list_and_finds_the_g6() {
        let sinks = parse_short_sinks(SHORT_SINKS);
        assert_eq!(sinks.len(), 3);
        assert_eq!(sinks[0].name, VIRTUAL_SINK_NAME);
        assert_eq!(sinks[2].index, 73);

        let g6 = find_g6_sink(&sinks).expect("G6 sink");
        assert_eq!(g6.index, 73);
        assert!(is_g6_pro_sink(&g6.name));
        assert!(!is_g6_pro_sink(VIRTUAL_SINK_NAME));
        assert!(!is_g6_pro_sink(
            "alsa_output.pci-0000_06_00.1.hdmi-stereo"
        ));
    }

    #[test]
    fn parses_volume_from_pactl_text() {
        assert_eq!(
            parse_sink_volume_percent(
                "Volume: front-left: 65536 / 100% / 0.00 dB, \
                 front-right: 65536 / 100% / 0.00 dB"
            ),
            Some(100.0)
        );
        assert_eq!(
            parse_sink_volume_percent(
                "Volume: aux0: 63569 /  97% / -0.79 dB, \
                 aux1: 63569 /  97% / -0.79 dB"
            ),
            Some(97.0)
        );
        assert_eq!(parse_sink_volume_percent("Mute: no"), None);
    }

    #[test]
    fn parses_sink_inputs_and_recognizes_the_filter_chain_output() {
        let output = "Sink Input #37\n\
            \tDriver: PipeWire\n\
            \tSink: 73\n\
            \tProperties:\n\
            \t\tnode.name = \"g6_ct_downmix.output\"\n\
            \t\tnode.group = \"filter-chain-1441-29\"\n\
            \n\
            Sink Input #301\n\
            \tDriver: PipeWire\n\
            \tSink: 36\n\
            \tProperties:\n\
            \t\tnode.name = \"mpv\"\n";
        let inputs = parse_sink_inputs(output);
        assert_eq!(inputs.len(), 2);
        assert_eq!(
            inputs[0],
            SinkInput {
                id: 37,
                sink: 73,
                node_name: Some("g6_ct_downmix.output".to_owned()),
                node_group: Some("filter-chain-1441-29".to_owned()),
            }
        );
        assert!(inputs[0].is_filter_chain_output());
        assert_eq!(inputs[1].id, 301);
        assert_eq!(inputs[1].sink, 36);
        assert_eq!(inputs[1].node_name.as_deref(), Some("mpv"));
        assert!(!inputs[1].is_filter_chain_output());
    }

    #[test]
    fn generated_config_targets_the_detected_g6() {
        let config = config_contents(
            Path::new("/home/user/.local/lib/linuxblaster/libg6ct_ladspa_f64.so"),
            "alsa_output.usb-Creative_Technology_Ltd_Sound_BlasterX_G6_6D00664763X-00.pro-output-0",
        );
        assert!(config.contains(
            "plugin = \"/home/user/.local/lib/linuxblaster/libg6ct_ladspa_f64.so\""
        ));
        assert!(config.contains(
            "target.object    = \"alsa_output.usb-Creative_Technology_Ltd_Sound_BlasterX_G6_6D00664763X-00.pro-output-0\""
        ));
        assert!(config.contains("node.name        = \"g6_ct_downmix\""));
        assert!(config.contains("audio.channels   = 8"));
        assert!(config.contains("MIT"));
    }

    #[test]
    fn embedded_plugin_is_a_shared_object() {
        let plugin = embedded_plugin();
        assert!(plugin.len() > 1024, "plugin is suspiciously small");
        assert_eq!(&plugin[..4], b"\x7fELF", "not an ELF file");
        assert!(
            plugin
                .windows(b"ladspa_descriptor".len())
                .any(|window| window == b"ladspa_descriptor"),
            "the LADSPA entry point is missing"
        );
    }

    #[test]
    fn generated_config_escapes_pipewire_strings() {
        let config = config_contents(
            Path::new("/tmp/a\"b\\c.so"),
            "target\"name",
        );
        assert!(config.contains("/tmp/a\\\"b\\\\c.so"));
        assert!(config.contains("target\\\"name"));
    }

    #[test]
    #[ignore = "needs the live PipeWire graph"]
    fn live_graph_recognizes_the_filter_chain_output() {
        let output = pactl(&["list", "sink-inputs"]).unwrap();
        let inputs = parse_sink_inputs(&output);
        assert!(
            inputs.iter().any(SinkInput::is_filter_chain_output),
            "no filter-chain output in {inputs:#?}"
        );
    }

    #[test]
    fn embedded_cold_boot_helper_is_internally_consistent() {
        for (name, contents, destination) in COLD_BOOT_FILES {
            assert!(!contents.is_empty(), "{name} is empty");
            assert!(destination.starts_with('/'), "{name}: {destination}");
        }

        let script = COLD_BOOT_FILES
            .iter()
            .find(|(name, _, _)| *name == "g6-coldboot-prep.sh")
            .map(|(_, contents, _)| *contents)
            .expect("script");
        let unit = COLD_BOOT_FILES
            .iter()
            .find(|(name, _, _)| *name == "g6-coldboot-prep.service")
            .map(|(_, contents, _)| *contents)
            .expect("unit");
        let rule = COLD_BOOT_FILES
            .iter()
            .find(|(name, _, _)| *name == "71-g6-coldboot.rules")
            .map(|(_, contents, _)| *contents)
            .expect("rule");
        let conf = COLD_BOOT_FILES
            .iter()
            .find(|(name, _, _)| *name == "99-g6-coldboot.conf")
            .map(|(_, contents, _)| *contents)
            .expect("conf");

        // The unit has to point at the path the installer copies the script to.
        assert!(unit.contains(&format!("ExecStart={COLD_BOOT_SCRIPT}")));
        // The rule has to start the unit.
        assert!(rule.contains("g6-coldboot-prep.service"));
        assert!(rule.contains("ATTR{idVendor}==\"041e\""));
        // The script must undo what the drop-in holds back.
        assert!(conf.contains("blacklist snd-usb-audio"));
        assert!(script.contains("modprobe snd-usb-audio"));
    }

    #[test]
    fn settings_defaults_fill_missing_fields() {
        let settings: Settings =
            serde_json::from_str(r#"{"sync_volume":true}"#).unwrap();
        assert!(settings.sync_volume);
        assert!(!settings.sinad_fix);

        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
    }
}
