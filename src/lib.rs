#![allow(unused)]

use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[cfg(test)]
mod tests;

pub mod features;
pub mod mixer;
pub mod system;
pub use features::{Feature, FeatureId, MIC_EQ_PRESETS, ValueKind};

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

/// The G6's hardware `Speaker` control has a raw range of 0..128.
/// Raw 124 is exactly -2.00 dB, which is the level where the G6's
/// full-scale SINAD degradation is avoided.
pub const SINAD_VOLUME_CAP: f32 = 124.0 / 128.0;

/// Default base directory path
/// On Windows: %LOCALAPPDATA%/linuxblaster/
/// On Unix: $XDG_DATA_HOME/linuxblaster/ or $HOME/.local/share/linuxblaster/
pub static DEFAULT_BASE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    #[cfg(windows)]
    {
        let appdata = env::var("LOCALAPPDATA")
            .expect("LOCALAPPDATA environment variable is not set");
        PathBuf::from(format!("{}/linuxblaster/", appdata))
    }

    #[cfg(not(windows))]
    {
        PathBuf::from(format!(
            "{}linuxblaster/",
            env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!(
                "{}/.local/share/",
                env::var("HOME").expect("HOME is not set")
            )),
        ))
    }
});

pub static DEVICE_CONNECTION: OnceLock<Mutex<HidDevice>> = OnceLock::new();

/// The udev rule that grants the logged-in user access to the G6.
/// Shipped in the packages, and installed by the app on request.
pub const UDEV_RULE: &str = include_str!("../packaging/70-linuxblaster-control.rules");
pub const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/70-linuxblaster-control.rules";

/// Why the app couldn't connect to the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// No G6 is plugged in (or it's switched off).
    NotFound,
    /// The G6 is there, but this user may not open its HID interface.
    /// This is what happens without the udev rule.
    PermissionDenied { path: String },
    /// Anything else, with a message for the logs/UI.
    Other(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Sound BlasterX G6 not found"),
            Self::PermissionDenied { path } => {
                write!(f, "No permission to access {}", path)
            }
            Self::Other(message) => write!(f, "{}", message),
        }
    }
}

impl Error for ConnectError {}

/// Health of the G6 control link, as seen by the periodic monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// The HID endpoint answered; no reopen was needed.
    Connected,
    /// The endpoint was silent, but answered on a fresh descriptor.
    Reconnected,
    /// The device is gone or still not answering; the monitor retries.
    Disconnected,
}

#[derive(Serialize, Deserialize)]
pub struct BlasterXG6 {
    pub features: Vec<Feature>,
    /// Counts successful writes, so the GUI knows when the last-state
    /// snapshot is stale. Not part of the serialized state/profile.
    #[serde(skip)]
    writes: AtomicU64,
    /// Optional upper bound for the hardware `Speaker` volume, as a
    /// fraction of its raw range. The GUI sets this from the SINAD fix
    /// setting; it is not part of the serialized device state.
    #[serde(skip)]
    volume_cap: Mutex<Option<f32>>,
}

impl BlasterXG6 {
    /// Connects to the device and reads its state.
    /// Panics if that's not possible; see `try_init()`.
    pub fn init() -> Self {
        Self::try_init().unwrap_or_else(|error| panic!("{}", error))
    }

    /// Connects to the device and reads its state.
    /// Can be called again after an error (e.g. once the G6 is plugged in).
    pub fn try_init() -> Result<Self, ConnectError> {
        if DEVICE_CONNECTION.get().is_none() {
            let connection = Self::open_device()?;
            // only fails if another thread was faster, which is fine too
            let _ = DEVICE_CONNECTION.set(Mutex::new(connection));
        }

        // The G6's HID endpoint can be slow to answer right after a cold
        // boot or a re-enumeration. Wait briefly and retry, reopening the
        // hidraw node in case the device re-enumerated in the meantime.
        //
        // Never reset the USB device here. A USBDEVFS_RESET wakes the HID
        // endpoint, but it also leaves the audio interface unusable until
        // the next re-enumeration, which breaks playback for the whole
        // session. The cold-boot helper does the reset before the audio
        // server opens the device; this launcher only waits and retries.
        if !Self::wait_for_ping() {
            return Err(ConnectError::Other(
                "The G6's control interface is not answering yet.".to_owned(),
            ));
        }

        let blaster = Self {
            features: features::all_features(),
            writes: AtomicU64::new(0),
            volume_cap: Mutex::new(None),
        };
        blaster.read_state_from_device();
        Ok(blaster)
    }

    /// Waits for the HID endpoint to answer, reopening the device between
    /// attempts. Returns `false` if it stays silent.
    ///
    /// Reopening is required because a re-enumeration invalidates the old
    /// hidraw file descriptor, and the kernel does not retarget it.
    pub(crate) fn wait_for_ping() -> bool {
        const ATTEMPTS: usize = 3;
        const INTERVAL: Duration = Duration::from_millis(300);

        for attempt in 0..ATTEMPTS {
            if features::ping_quiet() {
                return true;
            }
            if attempt + 1 == ATTEMPTS {
                break;
            }
            std::thread::sleep(INTERVAL);
            if let Ok(connection) = Self::open_device() {
                *DEVICE_CONNECTION.get().unwrap().lock().unwrap() =
                    connection;
            }
        }
        false
    }

    /// Replaces the global connection with a fresh hidraw descriptor.
    /// Returns `false` if the device is not currently enumerable.
    ///
    /// Used after a Direct Mode switch or any re-enumeration: the old
    /// descriptor is dead even though the USB device is still there.
    pub(crate) fn reopen() -> bool {
        match Self::open_device() {
            Ok(connection) => {
                let Some(global) = DEVICE_CONNECTION.get() else {
                    return false;
                };
                *global.lock().unwrap() = connection;
                true
            }
            Err(error) => {
                debug!("G6 reopen failed: {}", error);
                false
            }
        }
    }

    /// One monitor tick: verify the control link and recover from a
    /// re-enumeration (Direct Mode, replug, hardware button).
    ///
    /// A ping is cheap when the device is healthy. The reopen path only
    /// runs when it fails; if the device is unplugged, `open_device()` is
    /// a quick device-list scan, so the UI does not stall.
    pub fn ensure_connected(&self) -> LinkState {
        if features::ping_quiet() {
            return LinkState::Connected;
        }

        warn!("G6 HID endpoint is not answering; reopening it");
        if Self::reopen() && features::ping_quiet() {
            info!("G6 reconnected; re-reading its state");
            self.read_state_from_device();
            return LinkState::Reconnected;
        }

        warn!("G6 is still not answering");
        LinkState::Disconnected
    }

    fn open_device() -> Result<HidDevice, ConnectError> {
        let api = HidApi::new().map_err(|error| {
            ConnectError::Other(format!("Failed to initialize HID: {}", error))
        })?;
        let device =
            Self::find_device(&api).map_err(|_| ConnectError::NotFound)?;

        // hidapi only reports a generic error, so check the permissions
        // on the hidraw node directly to tell the user what's wrong
        let path = device.path().to_string_lossy().into_owned();
        if let Err(error) =
            std::fs::OpenOptions::new().read(true).write(true).open(&path)
            && error.kind() == ErrorKind::PermissionDenied
        {
            return Err(ConnectError::PermissionDenied { path });
        }

        let connection = device.open_device(&api).map_err(|error| {
            ConnectError::Other(format!("Failed to open {}: {}", path, error))
        })?;

        if connection.set_blocking_mode(false).is_err() {
            warn!("Failed to set blocking mode to false");
            warn!("Continuing with blocking mode in unknown state...");
        }

        Ok(connection)
    }

    /// Queries every feature from hardware individually.
    /// This will update the internal state of the features with the current hardware values.
    pub fn read_state_from_device(&self) {
        for feature in &self.features {
            feature.read_from_device();
        }
        // whatever got pushed in between is already covered by the reads
        features::UNSOLICITED.lock().unwrap().clear();
    }

    /// Sets an upper bound for the hardware `Speaker` volume.
    ///
    /// The value is a fraction of the raw 0..128 range. Pass `None` to
    /// remove the cap. Used by the SINAD fix, which stops the DAC at the
    /// -2.00 dB point (raw 124) instead of full scale.
    pub fn set_volume_cap(&self, cap: Option<f32>) {
        *self.volume_cap.lock().unwrap() =
            cap.map(|value| value.clamp(0.0, 1.0));
    }

    /// The current hardware volume cap, if any.
    pub fn volume_cap(&self) -> Option<f32> {
        *self.volume_cap.lock().unwrap()
    }

    /// Pulls the hardware `Speaker` volume down to the cap if a knob or
    /// another client raised it above the limit. Returns `true` if a
    /// write was made.
    pub fn enforce_volume_cap(&self) -> bool {
        let Some(cap) = self.volume_cap() else {
            return false;
        };
        let current = self.feature(FeatureId::SpeakerVolume).value();
        if current.is_nan() || current <= cap + 1e-4 {
            return false;
        }
        match self.set_feature(FeatureId::SpeakerVolume, Some(cap)) {
            Ok(()) => {
                debug!(
                    "Hardware volume {} exceeded the cap {}; lowered it",
                    current, cap
                );
                true
            }
            Err(error) => {
                warn!("Couldn't enforce the hardware volume cap: {}", error);
                false
            }
        }
    }

    /// Re-reads only the ALSA mixer features.
    /// They can change outside of this app (volume knob, pavucontrol, ...).
    pub fn read_mixer_state(&self) {
        // don't log an error per control every second if the card is missing
        if !mixer::is_available() {
            debug!("ALSA card not available, skipping mixer refresh");
            return;
        }
        for feature in &self.features {
            if feature.id.alsa_element().is_some() {
                feature.read_from_device();
            }
        }
        // The physical knob and other clients can move the Speaker control.
        self.enforce_volume_cap();
    }

    /// Applies state the device pushed on its own
    /// (after writes, and when the hardware buttons are used)
    /// to the feature cache, without any extra queries.
    ///
    /// Returns `true` if anything was processed.
    pub fn process_device_events(&self) -> bool {
        while let Some(packet) = features::read_packet(0) {
            features::UNSOLICITED.lock().unwrap().push(packet);
        }
        let packets = std::mem::take(&mut *features::UNSOLICITED.lock().unwrap());

        let mut output_changed = false;
        for packet in &packets {
            if packet[0] != 0x5a {
                continue;
            }
            match packet[1] {
                // StatusResponse, possibly with multiple features
                0x11 => {
                    for (family, feature_id, raw) in
                        features::status_entries(packet)
                    {
                        let Some(id) =
                            FeatureId::from_dsp_address(family, feature_id)
                        else {
                            continue;
                        };
                        debug!("Device pushed {} (raw: {})", id, raw);
                        self.feature(id).set_cached(id.decode(raw));
                    }
                }
                // GlobalProfile bitmask
                0x26 if packet[3] == 0x08 => {
                    let bitmask = packet[6];
                    debug!("Device pushed global bitmask 0x{:02x}", bitmask);
                    self.feature(FeatureId::SbxMaster).set_cached(
                        if bitmask & 0x01 != 0 { 1.0 } else { 0.0 },
                    );
                    self.feature(FeatureId::ScoutMode).set_cached(
                        if bitmask & 0x02 != 0 { 1.0 } else { 0.0 },
                    );
                }
                // OutputSelect read response
                0x2c if packet[3] == 0x01 => {
                    let output = features::output_decode(packet[4]);
                    if output != self.feature(FeatureId::Output).value() {
                        output_changed = true;
                    }
                }
                _ => {}
            }
        }

        // the device keeps separate settings per output
        if output_changed {
            info!("Output changed on the device, re-reading state");
            self.read_state_from_device();
        }

        !packets.is_empty()
    }

    /// Lookup a feature by ID.
    pub fn feature(&self, id: FeatureId) -> &Feature {
        self.features
            .iter()
            .find(|feature| feature.id == id)
            .unwrap_or_else(|| panic!("Feature {:?} not registered", id))
    }

    /// Number of writes performed since startup. The GUI uses this to
    /// decide when the last-state snapshot needs saving.
    pub fn write_count(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }

    /// Managed Write:
    /// enable dependencies, write, confirm, re-query dependents.
    ///
    /// Automatically calls `read_state_from_device()` after switching the Output.
    ///
    /// Pass `None` for value to toggle (flip between 0.0 and 1.0).
    pub fn set_feature(
        &self,
        id: FeatureId,
        value: Option<f32>,
    ) -> Result<(), Box<dyn Error>> {
        let feature = self.feature(id);

        let actual_value = match value {
            Some(value) => value,
            None => {
                if feature.as_bool() {
                    0.0
                } else {
                    1.0
                }
            }
        };

        // The SINAD fix caps the hardware Speaker control. Clamp here so
        // every write path (sliders, profiles, restore, guard) obeys it.
        let actual_value = if id == FeatureId::SpeakerVolume {
            match self.volume_cap() {
                Some(cap) => actual_value.min(cap),
                None => actual_value,
            }
        } else {
            actual_value
        };

        debug!("set_feature: {} = {}", id, actual_value);

        for &dependency_id in id.dependencies() {
            let dependency = self.feature(dependency_id);
            if dependency.value() != 1.0 {
                debug!("Enabling dependency: {} for {}", dependency_id, id);
                self.set_feature(dependency_id, Some(1.0))?;
            }
        }

        feature.write_to_device(actual_value);
        feature.read_from_device();

        for &dependent_id in id.dependents() {
            self.feature(dependent_id).read_from_device();
        }

        // changing the output changes the internal settings profile
        if feature.id == FeatureId::Output {
            self.read_state_from_device();
        }

        self.process_device_events();

        // SBX and Direct Mode cannot coexist: Direct Mode bypasses the DSP,
        // so SBX/Scout would silently do nothing. The SBX master setter
        // already switches Direct Mode; writes that turn SBX on as a side
        // effect (an effect toggle, a profile, Reset All) need the guard.
        if feature.id == FeatureId::SbxMaster {
            self.feature(FeatureId::DirectMode).read_from_device();
        } else if feature.id != FeatureId::DirectMode {
            self.disable_direct_mode_if_sbx_on();
        }

        self.writes.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Leaves Direct Mode if the device reports SBX back on.
    ///
    /// The device turns the SBX master on by itself when the first effect
    /// (or EQ band) is enabled. If the GUI was in Direct Mode, that write
    /// would restore the DSP path but leave the bypass on, so the effect
    /// would be inaudible. Direct Mode is the loser, always.
    fn disable_direct_mode_if_sbx_on(&self) {
        let direct = self.feature(FeatureId::DirectMode).value();
        if direct.is_nan() || direct <= 0.5 {
            return;
        }

        // The cached SBX bit may predate the device's global-profile push.
        self.feature(FeatureId::SbxMaster).read_from_device();
        if self.feature(FeatureId::SbxMaster).value() > 0.5 {
            warn!("SBX and Direct Mode are both on; leaving Direct Mode");
            self.feature(FeatureId::DirectMode).write_to_device(0.0);
            self.feature(FeatureId::DirectMode).read_from_device();
        }
    }

    pub fn find_device(api: &HidApi) -> Result<DeviceInfo, Box<dyn Error>> {
        let device = api
            .device_list()
            .find(|device| {
                debug!("Checking device: {:04x?}", device);
                device.vendor_id() == VENDOR_ID
                    && device.product_id() == PRODUCT_ID
                    && device.interface_number() == INTERFACE
            })
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "Device not found",
                ))
            })
            .cloned()?;
        Ok(device)
    }

    /// Checks whether the device answers a ping.
    pub fn is_connected(&self) -> bool {
        features::ping()
    }

    /// ### Important
    /// This does **not** reset the USB Connection!
    ///
    /// This is a managed reset function that takes device quirks into account.
    /// It resets "all" features to their default values, where "all"
    /// means a hard-coded list of features that this software touches.
    ///
    /// Does not reset the Output, Lighting, or the hardware Mixer
    /// (see `FeatureId::reset_value()`).
    ///
    /// If this software doesn't modify a feature, it won't be included in the reset.
    /// I don't know where this would become relevant,
    /// but I figured it'd be worth noting down.
    pub fn reset(&self) -> Result<(), Box<dyn Error>> {
        // yeah the return type might be stupid, I'll fix it soon™️
        let values = self
            .features
            .iter()
            .filter_map(|feature| {
                feature.id.reset_value().map(|value| (feature.id, value))
            })
            .collect();

        self.write_in_order(values);
        self.read_state_from_device();
        self.disable_direct_mode_if_sbx_on();
        self.writes.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Writes raw values without the dependency handling of `set_feature()`,
    /// in an order that avoids device side effects:
    /// 1. values (levels, EQ bands, modes, ...)
    /// 2. toggles, which also switch SBX on/off on the device by themselves
    /// 3. Scout Mode
    ///
    /// SBX Master itself is never written here: it reflects whether any
    /// playback effect is on, and turning it on with no effect active
    /// makes the device enable Surround on its own.
    fn write_in_order(&self, values: Vec<(FeatureId, f32)>) {
        let is_global = |id: FeatureId| {
            matches!(id, FeatureId::SbxMaster | FeatureId::ScoutMode)
        };
        let is_toggle =
            |id: FeatureId| matches!(id.value_kind(), ValueKind::Toggle);

        let values_first = values
            .iter()
            .filter(|(id, _)| !is_toggle(*id) && !is_global(*id));
        let toggles_second = values
            .iter()
            .filter(|(id, _)| is_toggle(*id) && !is_global(*id));
        let scout_last = values
            .iter()
            .filter(|(id, _)| *id == FeatureId::ScoutMode);

        for &(id, value) in values_first.chain(toggles_second).chain(scout_last)
        {
            self.feature(id).write_to_device(value);
        }

        features::UNSOLICITED.lock().unwrap().clear();
    }

    pub fn save_profile(&self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        // save the current state of the features to a profile

        let profile = BlasterXG6 {
            features: self
                .features
                .iter()
                .filter(|feature| feature.id.in_profile())
                .cloned()
                .collect(),
            writes: AtomicU64::new(0),
            volume_cap: Mutex::new(None),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&profile)?;
        std::fs::write(&path, json)?;
        info!("Saved Profile to {:?}", path);

        Ok(())
    }

    /// Applies a profile to the currently active output.
    ///
    /// Every saved value is written, including toggles that are OFF,
    /// and sliders are written as-is without switching their toggle on.
    pub fn apply_profile(&self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        let json = std::fs::read_to_string(&path)?;
        let saved: BlasterXG6 = serde_json::from_str(&json)?;

        let values = saved
            .features
            .iter()
            .filter(|feature| feature.id.in_profile())
            // NaN = the value couldn't be read when the profile was saved
            .filter(|feature| !feature.value().is_nan())
            .map(|feature| (feature.id, feature.value()))
            .collect();

        self.write_in_order(values);
        self.read_state_from_device();
        self.disable_direct_mode_if_sbx_on();
        self.writes.fetch_add(1, Ordering::Relaxed);

        info!("Applied Profile ({:?})", path);

        Ok(())
    }

    fn last_state_path() -> PathBuf {
        DEFAULT_BASE_PATH.join("last_state.json")
    }

    /// Saves the complete current state (including Direct Mode and the SBX
    /// master) as the state to re-assert on the next launch.
    ///
    /// The file is written atomically, so a power loss can't leave a
    /// truncated snapshot behind.
    pub fn save_last_state(&self) -> Result<(), Box<dyn Error>> {
        let path = Self::last_state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, json)?;
        std::fs::rename(&temporary, &path)?;
        debug!("Saved last state to {:?}", path);
        Ok(())
    }

    /// Re-applies the last saved state after a reboot or a fresh launch.
    ///
    /// Only differences are written, so a device that kept its NVRAM
    /// settings is not written to (and not re-enumerated) for nothing.
    /// Output goes first because the device stores a settings bank per
    /// output; SBX and Direct Mode go last because they switch the DSP
    /// path and can re-enumerate the USB device.
    pub fn restore_last_state(&self) -> Result<usize, Box<dyn Error>> {
        let path = Self::last_state_path();
        let json = match std::fs::read_to_string(&path) {
            Ok(json) => json,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                debug!("No last state at {:?}", path);
                return Ok(0);
            }
            Err(error) => return Err(error.into()),
        };

        let saved: BlasterXG6 = match serde_json::from_str(&json) {
            Ok(saved) => saved,
            Err(error) => {
                // An old or interrupted snapshot must not block every
                // launch. Keep it for inspection and start clean.
                warn!("Ignoring unreadable last state: {}", error);
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                return Ok(0);
            }
        };

        let saved_value = |id: FeatureId| -> Option<f32> {
            saved
                .features
                .iter()
                .find(|feature| feature.id == id)
                .map(Feature::value)
                .filter(|value| !value.is_nan())
        };

        let mut restored = 0usize;

        if let Some(output) = saved_value(FeatureId::Output)
            && !nearly_equal(self.feature(FeatureId::Output).value(), output)
        {
            self.set_feature(FeatureId::Output, Some(output))?;
            restored += 1;
        }

        let mut values = Vec::new();
        for feature in &saved.features {
            if !feature.id.in_profile()
                || matches!(
                    feature.id,
                    FeatureId::SbxMaster
                        | FeatureId::DirectMode
                        | FeatureId::SpeakerVolume
                )
            {
                continue;
            }

            let value = feature.value();
            if value.is_nan() {
                continue;
            }
            if nearly_equal(self.feature(feature.id).value(), value) {
                continue;
            }
            values.push((feature.id, value));
        }

        if !values.is_empty() {
            restored += values.len();
            self.write_in_order(values);
            self.read_state_from_device();
        }

        // The speaker switch is the output's mute, not its system volume, so
        // restoring it is safe. Its volume is intentionally left alone.
        if let Some(speaker) = saved_value(FeatureId::SpeakerSwitch)
            && !nearly_equal(
                self.feature(FeatureId::SpeakerSwitch).value(),
                speaker,
            )
        {
            self.set_feature(FeatureId::SpeakerSwitch, Some(speaker))?;
            restored += 1;
        }

        // The device switches Surround on by itself when the SBX master is
        // turned on without an effect, so the effects have to be in place
        // before the master bit is restored.
        if let Some(sbx) = saved_value(FeatureId::SbxMaster)
            && !nearly_equal(
                self.feature(FeatureId::SbxMaster).value(),
                sbx,
            )
        {
            self.set_feature(FeatureId::SbxMaster, Some(sbx))?;
            restored += 1;
        }

        // Normally the SBX write above already produced this. It also covers
        // a snapshot where both were off, which the SBX wiring never emits.
        if let Some(direct) = saved_value(FeatureId::DirectMode)
            && !nearly_equal(
                self.feature(FeatureId::DirectMode).value(),
                direct,
            )
        {
            self.set_feature(FeatureId::DirectMode, Some(direct))?;
            restored += 1;
        }

        info!("Restored {} last-state values from {:?}", restored, path);
        Ok(restored)
    }
}

/// Compares two feature values with the tolerance the GUI needs to avoid
/// rewriting a setting that is already in place.
pub(crate) fn nearly_equal(a: f32, b: f32) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() && b.is_nan();
    }
    (a - b).abs() <= 1e-4
}
