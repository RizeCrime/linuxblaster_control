use std::fmt;
use std::sync::Mutex;

use serde::de::Error;
use serde::{Deserialize, Serialize, ser::Serializer};
use tracing::{debug, error, info};

use crate::DEVICE_CONNECTION;

// ─── FeatureId ───────────────────────────────────────────────────────────────

/// ### IMPORTANT!
/// The Device stores settings for each Output separately.
/// This means switching between outputs neccessitates re-querying every other Feature!
///
/// If you use the managed `set_feature()` method on `BlasterXG6` struct,
/// this is done automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureId {
    // GlobalProfile (0x26)
    SbxMaster,
    ScoutMode,

    // OutputSelect (0x2c)
    Output,

    // DSP 0x96 — SBX sub-features
    SurroundToggle,
    SurroundLevel,
    DialogPlusToggle,
    DialogPlusLevel,
    SmartVolToggle,
    SmartVolLevel,
    SmartVolMode,
    CrystalizerToggle,
    CrystalizerLevel,
    BassToggle,
    BassLevel,
    SurroundDistance,

    // DSP 0x96 — Equalizer
    EqToggle,
    EqPreAmp,
    Eq31Hz,
    Eq62Hz,
    Eq125Hz,
    Eq250Hz,
    Eq500Hz,
    Eq1kHz,
    Eq2kHz,
    Eq4kHz,
    Eq8kHz,
    Eq16kHz,
    // Placeholders for future protocols
    // DacFilter,
    // GainConfig,
}

impl fmt::Display for FeatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl FeatureId {
    pub const ALL: &[FeatureId] = &[
        Self::SbxMaster,
        Self::ScoutMode,
        Self::Output,
        Self::SurroundToggle,
        Self::SurroundLevel,
        Self::DialogPlusToggle,
        Self::DialogPlusLevel,
        Self::SmartVolToggle,
        Self::SmartVolLevel,
        Self::SmartVolMode,
        Self::CrystalizerToggle,
        Self::CrystalizerLevel,
        Self::BassToggle,
        Self::BassLevel,
        Self::SurroundDistance,
        Self::EqToggle,
        Self::EqPreAmp,
        Self::Eq31Hz,
        Self::Eq62Hz,
        Self::Eq125Hz,
        Self::Eq250Hz,
        Self::Eq500Hz,
        Self::Eq1kHz,
        Self::Eq2kHz,
        Self::Eq4kHz,
        Self::Eq8kHz,
        Self::Eq16kHz,
    ];

    pub const SBX_TOGGLES: &[FeatureId] = &[
        Self::SurroundToggle,
        Self::DialogPlusToggle,
        Self::SmartVolToggle,
        Self::CrystalizerToggle,
        Self::BassToggle,
    ];

    /// The 10 ISO EQ bands (no pre-amp).
    pub const EQ_BANDS: &[FeatureId] = &[
        Self::Eq31Hz,
        Self::Eq62Hz,
        Self::Eq125Hz,
        Self::Eq250Hz,
        Self::Eq500Hz,
        Self::Eq1kHz,
        Self::Eq2kHz,
        Self::Eq4kHz,
        Self::Eq8kHz,
        Self::Eq16kHz,
    ];

    /// Pre-amp + 10 ISO EQ bands.
    pub const EQ_ALL: &[FeatureId] = &[
        Self::EqPreAmp,
        Self::Eq31Hz,
        Self::Eq62Hz,
        Self::Eq125Hz,
        Self::Eq250Hz,
        Self::Eq500Hz,
        Self::Eq1kHz,
        Self::Eq2kHz,
        Self::Eq4kHz,
        Self::Eq8kHz,
        Self::Eq16kHz,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SbxMaster => "SBX",
            Self::ScoutMode => "Scout Mode",
            Self::Output => "Output",
            Self::SurroundToggle => "Surround",
            Self::SurroundLevel => "Surround Slider",
            Self::DialogPlusToggle => "Dialog+",
            Self::DialogPlusLevel => "Dialog+ Slider",
            Self::SmartVolToggle => "Smart Volume",
            Self::SmartVolLevel => "Smart Volume Slider",
            Self::SmartVolMode => "Smart Volume Mode",
            Self::CrystalizerToggle => "Crystalizer",
            Self::CrystalizerLevel => "Crystalizer Slider",
            Self::BassToggle => "Bass",
            Self::BassLevel => "Bass Slider",
            Self::SurroundDistance => "Surround Distance",
            Self::EqToggle => "Equalizer",
            Self::EqPreAmp => "EQ Pre-Amp",
            Self::Eq31Hz => "EQ 31Hz",
            Self::Eq62Hz => "EQ 62Hz",
            Self::Eq125Hz => "EQ 125Hz",
            Self::Eq250Hz => "EQ 250Hz",
            Self::Eq500Hz => "EQ 500Hz",
            Self::Eq1kHz => "EQ 1kHz",
            Self::Eq2kHz => "EQ 2kHz",
            Self::Eq4kHz => "EQ 4kHz",
            Self::Eq8kHz => "EQ 8kHz",
            Self::Eq16kHz => "EQ 16kHz",
        }
    }

    pub fn value_kind(&self) -> ValueKind {
        match self {
            Self::SbxMaster
            | Self::ScoutMode
            | Self::SurroundToggle
            | Self::DialogPlusToggle
            | Self::SmartVolToggle
            | Self::CrystalizerToggle
            | Self::BassToggle
            | Self::EqToggle => ValueKind::Toggle,

            Self::SurroundLevel
            | Self::DialogPlusLevel
            | Self::SmartVolLevel
            | Self::CrystalizerLevel
            | Self::BassLevel => ValueKind::Percentage,

            Self::EqPreAmp => ValueKind::Ranged {
                min: -6.0,
                max: 6.0,
            },

            Self::Eq31Hz
            | Self::Eq62Hz
            | Self::Eq125Hz
            | Self::Eq250Hz
            | Self::Eq500Hz
            | Self::Eq1kHz
            | Self::Eq2kHz
            | Self::Eq4kHz
            | Self::Eq8kHz
            | Self::Eq16kHz => ValueKind::Ranged {
                min: -12.0,
                max: 12.0,
            },

            Self::SurroundDistance => ValueKind::Ranged {
                min: 10.0,
                max: 300.0,
            },

            Self::SmartVolMode => {
                ValueKind::Preset(&["Normal", "Loud", "Night"])
            }

            Self::Output => ValueKind::Preset(&["Speakers", "Headphones"]),
        }
    }

    /// The `(family, feature_id)` pair for 0x96 DSP features.
    /// Returns `None` for features using other protocols.
    pub fn dsp_address(&self) -> Option<(u8, u8)> {
        match self {
            Self::SurroundToggle => Some((0x96, 0x00)),
            Self::SurroundLevel => Some((0x96, 0x01)),
            Self::DialogPlusToggle => Some((0x96, 0x02)),
            Self::DialogPlusLevel => Some((0x96, 0x03)),
            Self::SmartVolToggle => Some((0x96, 0x04)),
            Self::SmartVolLevel => Some((0x96, 0x05)),
            Self::SmartVolMode => Some((0x96, 0x06)),
            Self::CrystalizerToggle => Some((0x96, 0x07)),
            Self::CrystalizerLevel => Some((0x96, 0x08)),
            Self::EqToggle => Some((0x96, 0x09)),
            Self::EqPreAmp => Some((0x96, 0x0a)),
            Self::Eq31Hz => Some((0x96, 0x0b)),
            Self::Eq62Hz => Some((0x96, 0x0c)),
            Self::Eq125Hz => Some((0x96, 0x0d)),
            Self::Eq250Hz => Some((0x96, 0x0e)),
            Self::Eq500Hz => Some((0x96, 0x0f)),
            Self::Eq1kHz => Some((0x96, 0x10)),
            Self::Eq2kHz => Some((0x96, 0x11)),
            Self::Eq4kHz => Some((0x96, 0x12)),
            Self::Eq8kHz => Some((0x96, 0x13)),
            Self::Eq16kHz => Some((0x96, 0x14)),
            Self::SurroundDistance => Some((0x96, 0x17)),
            Self::BassToggle => Some((0x96, 0x18)),
            Self::BassLevel => Some((0x96, 0x19)),
            _ => None,
        }
    }

    /// Features that must be ON for this feature to work.
    pub fn dependencies(&self) -> &'static [FeatureId] {
        match self {
            Self::SbxMaster | Self::ScoutMode | Self::Output => &[],

            Self::SurroundToggle
            | Self::DialogPlusToggle
            | Self::SmartVolToggle
            | Self::CrystalizerToggle
            | Self::BassToggle
            | Self::EqToggle => &[Self::SbxMaster],

            Self::SurroundLevel | Self::SurroundDistance => {
                &[Self::SbxMaster, Self::SurroundToggle]
            }
            Self::DialogPlusLevel => &[Self::SbxMaster, Self::DialogPlusToggle],
            Self::SmartVolLevel | Self::SmartVolMode => {
                &[Self::SbxMaster, Self::SmartVolToggle]
            }
            Self::CrystalizerLevel => {
                &[Self::SbxMaster, Self::CrystalizerToggle]
            }
            Self::BassLevel => &[Self::SbxMaster, Self::BassToggle],

            Self::EqPreAmp
            | Self::Eq31Hz
            | Self::Eq62Hz
            | Self::Eq125Hz
            | Self::Eq250Hz
            | Self::Eq500Hz
            | Self::Eq1kHz
            | Self::Eq2kHz
            | Self::Eq4kHz
            | Self::Eq8kHz
            | Self::Eq16kHz => &[Self::SbxMaster, Self::EqToggle],
        }
    }

    /// Features whose cached state should be re-queried after this feature changes.
    pub fn dependents(&self) -> &'static [FeatureId] {
        match self {
            Self::SbxMaster => &[
                Self::SurroundToggle,
                Self::SurroundLevel,
                Self::SurroundDistance,
                Self::DialogPlusToggle,
                Self::DialogPlusLevel,
                Self::SmartVolToggle,
                Self::SmartVolLevel,
                Self::SmartVolMode,
                Self::CrystalizerToggle,
                Self::CrystalizerLevel,
                Self::BassToggle,
                Self::BassLevel,
                Self::EqToggle,
                Self::EqPreAmp,
                Self::Eq31Hz,
                Self::Eq62Hz,
                Self::Eq125Hz,
                Self::Eq250Hz,
                Self::Eq500Hz,
                Self::Eq1kHz,
                Self::Eq2kHz,
                Self::Eq4kHz,
                Self::Eq8kHz,
                Self::Eq16kHz,
            ],
            Self::SurroundToggle => {
                &[Self::SurroundLevel, Self::SurroundDistance]
            }
            Self::DialogPlusToggle => &[Self::DialogPlusLevel],
            Self::SmartVolToggle => &[Self::SmartVolLevel, Self::SmartVolMode],
            Self::CrystalizerToggle => &[Self::CrystalizerLevel],
            Self::BassToggle => &[Self::BassLevel],
            Self::EqToggle => &[
                Self::EqPreAmp,
                Self::Eq31Hz,
                Self::Eq62Hz,
                Self::Eq125Hz,
                Self::Eq250Hz,
                Self::Eq500Hz,
                Self::Eq1kHz,
                Self::Eq2kHz,
                Self::Eq4kHz,
                Self::Eq8kHz,
                Self::Eq16kHz,
            ],
            _ => &[],
        }
    }

    /// For toggle features that gate a slider, returns the paired slider's ID.
    pub fn paired_slider(&self) -> Option<FeatureId> {
        match self {
            Self::SurroundToggle => Some(Self::SurroundLevel),
            Self::DialogPlusToggle => Some(Self::DialogPlusLevel),
            Self::SmartVolToggle => Some(Self::SmartVolLevel),
            Self::CrystalizerToggle => Some(Self::CrystalizerLevel),
            Self::BassToggle => Some(Self::BassLevel),
            _ => None,
        }
    }

    pub fn paired_toggle(&self) -> Option<FeatureId> {
        match self {
            Self::SurroundLevel => Some(Self::SurroundToggle),
            Self::DialogPlusLevel => Some(Self::DialogPlusToggle),
            Self::SmartVolLevel => Some(Self::SmartVolToggle),
            Self::CrystalizerLevel => Some(Self::CrystalizerToggle),
            Self::BassLevel => Some(Self::BassToggle),
            _ => None,
        }
    }
}

// ─── ValueKind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum ValueKind {
    Toggle,
    Percentage,
    Ranged { min: f32, max: f32 },
    Preset(&'static [&'static str]),
}

// ─── Feature ─────────────────────────────────────────────────────────────────

fn serialize_mutex_f32<S>(
    value: &Mutex<f32>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer, {
    let value = value
        .lock()
        .map_err(|error| serde::ser::Error::custom(error.to_string()))?;
    value.serialize(serializer)
}

#[derive(Debug, Serialize)]
pub struct Feature {
    pub id: FeatureId,
    #[serde(serialize_with = "serialize_mutex_f32")]
    value: Mutex<f32>,

    // Setter and Getter function are stored as members,
    // because it provides comfortable flexibility while reverse Engineering.
    #[serde(skip)]
    getter: fn(&Feature) -> f32,
    #[serde(skip)]
    setter: fn(&Feature, f32),
}

impl Clone for Feature {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: Mutex::new(*self.value.lock().unwrap()),
            getter: self.getter,
            setter: self.setter,
        }
    }
}

impl<'de> Deserialize<'de> for Feature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>, {
        #[derive(Deserialize)]
        struct Raw {
            id: FeatureId,
            value: f32,
        }
        let Raw { id, value } = Raw::deserialize(deserializer)?;

        let features = all_features();
        let feature = features
            .iter()
            .find(|feature| feature.id == id)
            .ok_or_else(|| {
                D::Error::custom(format!("Unknown feature id: {}", id))
            })?;
        *feature.value.lock().unwrap() = value;

        Ok(feature.clone())
    }
}

impl Feature {
    /// Constructor for 0x96 family features
    /// that all share the generic DSP getter/setter.
    pub fn dsp(id: FeatureId) -> Self {
        Self {
            id,
            value: Mutex::new(f32::NAN),
            getter: dsp_get,
            setter: dsp_set,
        }
    }

    /// Returns the cached value. No hardware I/O.
    pub fn value(&self) -> f32 {
        *self.value.lock().unwrap()
    }

    /// Returns the bool interpretation of a Toggle feature's cached value.
    /// Panics if the value is not exactly 0.0 or 1.0 as a sanity check;
    /// a non-boolean value for a toggle means the protocol
    /// assumptions are broken and no functionality can be guaranteed.
    pub fn as_bool(&self) -> bool {
        let value = self.value();
        if value == 0.0 {
            false
        } else if value == 1.0 {
            true
        } else {
            panic!(
                "Toggle feature '{}' has invalid value {} (expected 0.0 or 1.0)",
                self.id.display_name(),
                value
            );
        }
    }

    /// Queries hardware for the current value, updates the cache, returns the fresh value.
    pub fn read_from_device(&self) -> f32 {
        (self.getter)(self)
    }

    /// sends one write packet to hardware and reads the ACK.
    /// logs an error if the ACK is missing or invalid.
    /// does NOT update the value cache.
    /// you'll have to call `read_from_device()` afterwards.
    pub fn write_to_device(&self, value: f32) {
        (self.setter)(self, value)
    }
}

// ─── Feature Registration ────────────────────────────────────────────────────

pub fn all_features() -> Vec<Feature> {
    vec![
        Feature::dsp(FeatureId::SurroundToggle),
        Feature::dsp(FeatureId::SurroundLevel),
        Feature::dsp(FeatureId::SurroundDistance),
        Feature::dsp(FeatureId::DialogPlusToggle),
        Feature::dsp(FeatureId::DialogPlusLevel),
        Feature::dsp(FeatureId::SmartVolToggle),
        Feature::dsp(FeatureId::SmartVolLevel),
        Feature::dsp(FeatureId::SmartVolMode),
        Feature::dsp(FeatureId::CrystalizerToggle),
        Feature::dsp(FeatureId::CrystalizerLevel),
        Feature::dsp(FeatureId::BassToggle),
        Feature::dsp(FeatureId::BassLevel),
        Feature::dsp(FeatureId::EqToggle),
        Feature::dsp(FeatureId::EqPreAmp),
        Feature::dsp(FeatureId::Eq31Hz),
        Feature::dsp(FeatureId::Eq62Hz),
        Feature::dsp(FeatureId::Eq125Hz),
        Feature::dsp(FeatureId::Eq250Hz),
        Feature::dsp(FeatureId::Eq500Hz),
        Feature::dsp(FeatureId::Eq1kHz),
        Feature::dsp(FeatureId::Eq2kHz),
        Feature::dsp(FeatureId::Eq4kHz),
        Feature::dsp(FeatureId::Eq8kHz),
        Feature::dsp(FeatureId::Eq16kHz),
        // manual struct instantiation used, so overrides are easily visible
        Feature {
            id: FeatureId::SbxMaster,
            value: Mutex::new(f32::NAN),
            getter: global_profile_get,
            setter: global_profile_set,
        },
        Feature {
            id: FeatureId::ScoutMode,
            value: Mutex::new(f32::NAN),
            getter: global_profile_get,
            setter: global_profile_set,
        },
        Feature {
            id: FeatureId::Output,
            value: Mutex::new(f32::NAN),
            getter: output_get,
            setter: output_set,
        },
    ]
}

// ─── USB Packet Helpers ──────────────────────────────────────────────────────

const MAX_READ_ATTEMPTS: usize = 30;
const READ_TIMEOUT_MS: i32 = 500;

fn read_packet() -> Option<[u8; 64]> {
    let mut buffer = [0u8; 64];
    match DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .read_timeout(&mut buffer, READ_TIMEOUT_MS)
    {
        Ok(0) => None,
        Ok(_) => Some(buffer),
        Err(error) => {
            error!("Failed to read packet: {:?}", error);
            None
        }
    }
}

fn read_ack() {
    for attempt in 0..MAX_READ_ATTEMPTS {
        let Some(packet) = read_packet() else {
            error!(
                "Expected ACK but no response on attempt {}/{}",
                attempt + 1,
                MAX_READ_ATTEMPTS
            );
            continue;
        };

        if packet[0] == 0x5a && packet[1] == 0x02 {
            debug!("ACK received: {:02x?}", &packet[..12]);
            return;
        }

        debug!(
            "Discarded non-ACK packet while waiting for ACK on attempt {}: {:02x?}",
            attempt + 1,
            &packet[..12]
        );
    }

    error!("No ACK received after {} attempts", MAX_READ_ATTEMPTS);
}

// ─── DSP Getter/Setter (0x96 family via 0x11/0x12) ──────────────────────────

fn dsp_get(feature: &Feature) -> f32 {
    let (family, feature_id) = feature
        .id
        .dsp_address()
        .expect("dsp_get called on non-DSP feature");

    debug!(
        "Querying DSP feature: {} (0x{:02x}:0x{:02x})",
        feature.id, family, feature_id
    );

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x11;
    payload[3] = 0x03;
    payload[4] = 0x01;
    payload[5] = family;
    payload[6] = feature_id;

    DEVICE_CONNECTION
        .get()
        .expect("Device Connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send Query to Device");

    for attempt in 0..MAX_READ_ATTEMPTS {
        let Some(response) = read_packet() else {
            error!(
                "No response on attempt {}/{} for {}",
                attempt + 1,
                MAX_READ_ATTEMPTS,
                feature.id
            );
            continue;
        };

        // expected: 5a 11 08 01 00 [family] [id] [f32 LE × 4]
        if response[0] == 0x5a
            && response[1] == 0x11
            && response[5] == family
            && response[6] == feature_id
        {
            let value = f32::from_le_bytes(
                response[7..11]
                    .try_into()
                    .expect("Failed to parse f32 from response bytes"),
            );
            debug!("Read {} = {}", feature.id, value);
            *feature.value.lock().unwrap() = value;
            return value;
        }

        debug!(
            "Discarded stale packet on attempt {}: {:02x?}",
            attempt + 1,
            &response[..12]
        );
    }

    error!(
        "No matching response after {} attempts for {}",
        MAX_READ_ATTEMPTS, feature.id
    );
    0.0
}

fn dsp_set(feature: &Feature, value: f32) {
    let (family, feature_id) = feature
        .id
        .dsp_address()
        .expect("dsp_set called on non-DSP feature");

    debug!(
        "Writing DSP feature: {} = {} (0x{:02x}:0x{:02x})",
        feature.id, value, family, feature_id
    );

    let value_bytes = value.to_le_bytes();

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x12;
    payload[3] = 0x07;
    payload[4] = 0x01;
    payload[5] = family;
    payload[6] = feature_id;
    payload[7..11].copy_from_slice(&value_bytes);

    DEVICE_CONNECTION
        .get()
        .expect("Device Connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send Write to Device");

    read_ack();
}

// ─── GlobalProfile Getter/Setter (0x26) ──────────────────────────────────────

fn global_profile_get(feature: &Feature) -> f32 {
    let bitmask = match feature.id {
        FeatureId::SbxMaster => 0x01u8,
        FeatureId::ScoutMode => 0x02u8,
        _ => panic!("global_profile_get called on {:?}", feature.id),
    };

    debug!(
        "Querying global profile: {} (bit 0x{:02x})",
        feature.id, bitmask
    );

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x26;
    payload[3] = 0x03;
    payload[4] = 0x08;
    payload[5] = 0xff;
    payload[6] = 0xff;

    DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send query to device");

    for attempt in 0..MAX_READ_ATTEMPTS {
        let Some(response) = read_packet() else {
            error!(
                "No response on attempt {}/{} for {}",
                attempt + 1,
                MAX_READ_ATTEMPTS,
                feature.id
            );
            continue;
        };

        // expected: 5a 26 0b 08 ff ff [bitmask] ...
        if response[0] == 0x5a && response[1] == 0x26 {
            let device_bitmask = response[6];
            let is_on = (device_bitmask & bitmask) != 0;
            let value = if is_on { 1.0 } else { 0.0 };
            debug!(
                "Read {} = {} (device bitmask: 0x{:02x})",
                feature.id, value, device_bitmask
            );
            *feature.value.lock().unwrap() = value;
            return value;
        }

        debug!(
            "Discarded stale packet on attempt {}: {:02x?}",
            attempt + 1,
            &response[..12]
        );
    }

    error!(
        "No matching response after {} attempts for {}",
        MAX_READ_ATTEMPTS, feature.id
    );
    0.0
}

fn global_profile_set(feature: &Feature, value: f32) {
    let profile_id = match feature.id {
        FeatureId::SbxMaster => 0x01u8,
        FeatureId::ScoutMode => 0x02u8,
        _ => panic!("global_profile_set called on {:?}", feature.id),
    };

    let state = if value > 0.0 { 0x01u8 } else { 0x00u8 };

    debug!(
        "Writing global profile: {} = {} (id: 0x{:02x}, state: 0x{:02x})",
        feature.id, value, profile_id, state
    );

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x26;
    payload[3] = 0x05;
    payload[4] = 0x07;
    payload[5] = profile_id;
    payload[6] = 0x00;
    payload[7] = state;
    payload[8] = 0x00;

    DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send write to device");

    read_ack();
}

// ─── Output Getter/Setter (0x2c) ────────────────────────────────────────────

fn output_get(feature: &Feature) -> f32 {
    debug!("Querying output device");

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x2c;
    payload[3] = 0x01;
    payload[4] = 0x01;

    DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send Query to Device");

    for attempt in 0..MAX_READ_ATTEMPTS {
        let Some(response) = read_packet() else {
            error!(
                "No response on attempt {}/{} for Output",
                attempt + 1,
                MAX_READ_ATTEMPTS
            );
            continue;
        };

        // expected: 5a 2c 05 01 [mode] ...
        if response[0] == 0x5a && response[1] == 0x2c {
            let value = match response[4] {
                0x02 => {
                    info!("Current output: Speakers");
                    0.0
                }
                0x04 => {
                    info!("Current output: Headphones");
                    1.0
                }
                other => {
                    error!("Unknown output mode: 0x{:02x}", other);
                    f32::NAN
                }
            };
            *feature.value.lock().unwrap() = value;
            return value;
        }

        debug!(
            "Discarded stale packet on attempt {}: {:02x?}",
            attempt + 1,
            &response[..12]
        );
    }

    error!(
        "No matching response after {} attempts for Output",
        MAX_READ_ATTEMPTS
    );
    0.0
}

fn output_set(feature: &Feature, value: f32) {
    let mode = if value > 0.0 { 0x04u8 } else { 0x02u8 };

    debug!("Setting output: mode 0x{:02x}", mode);

    let mut payload = [0u8; 65];
    payload[1] = 0x5a;
    payload[2] = 0x2c;
    payload[3] = 0x05;
    payload[4] = 0x00;
    payload[5] = mode;
    payload[6] = 0x00;
    payload[7] = 0x00;
    payload[8] = 0x00;

    DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload)
        .expect("Failed to send write to device");

    read_ack();
}
