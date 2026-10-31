use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use serde::de::Error;
use serde::{Deserialize, Serialize, ser::Serializer};
use tracing::{debug, error, info, trace, warn};

use crate::DEVICE_CONNECTION;
use crate::mixer;

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

    // Direct Mode (0x39) — hardware DSP bypass, tied to SbxMaster
    DirectMode,

    // OutputSelect (0x2c)
    Output,

    // DacFilter (0x6c)
    DacFilter,

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

    // DSP 0x95 — Recording Pane
    VoiceClarityToggle,
    VoiceClarityNoiseReduction,
    AcousticEchoCancellation,
    RecordingSmartVolToggle,
    MicEqToggle,
    MicEqBand1,
    MicEqBand2,
    MicEqBand3,
    MicEqBand4,
    MicEqBand5,
    MicEqBand6,
    MicEqBand7,
    MicEqBand8,

    // GainConfig (0x3c)
    MicBoost,

    // DSP 0x97 — Decoder
    Decoder,

    // Lighting (0x3a)
    LightingToggle,
    LightingColor,

    // ALSA — USB Audio Class mixer (hardware controls, not HID)
    SpeakerSwitch,
    SpeakerVolume,
    MonitorLineInSwitch,
    MonitorLineInVolume,
    MonitorMicSwitch,
    MonitorMicVolume,
    MonitorSpdifSwitch,
    MonitorSpdifVolume,
    RecordLineInSwitch,
    RecordLineInVolume,
    RecordMicSwitch,
    RecordMicVolume,
    RecordSpdifSwitch,
    RecordSpdifVolume,
    RecordWhatUHearSwitch,
    RecordWhatUHearVolume,
    CaptureSource,
}

impl fmt::Display for FeatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Which side of an ALSA mixer element a feature talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlsaDirection {
    Playback,
    Capture,
    /// Enumerated element (e.g. a source selector)
    Enum,
}

/// Mic EQ presets as sent by the Windows software (bands 0x95:0x14–0x1b, dB).
pub const MIC_EQ_PRESETS: &[(&str, [f32; 8])] = &[
    ("Preset 1", [-3.0, -4.0, 0.0, 2.0, 3.0, -3.0, 4.0, 5.0]),
    ("Preset 2", [-3.0, -4.0, 0.0, 2.0, 4.0, -2.0, 2.0, 4.0]),
    ("Preset 3", [-2.0, -3.0, 3.0, 4.0, 4.0, -4.0, 3.0, 2.0]),
    ("Preset 4", [-3.0, -5.0, 0.0, 4.0, 0.0, -3.0, 0.0, 0.0]),
    ("Preset 5", [-2.0, -3.0, 2.0, 4.0, 4.0, 0.0, -3.0, 2.0]),
    ("Preset 6", [-5.0, -4.0, -2.0, 0.0, 3.0, 4.0, 6.0, 7.0]),
    ("Preset 7", [0.0, 3.0, -2.0, -4.0, -4.0, -2.0, 5.0, 7.0]),
    ("Preset 8", [0.0, 0.0, 2.0, 2.0, 3.0, -4.0, 2.0, 4.0]),
    ("Preset 9", [0.0, 0.0, 2.0, 2.0, -2.0, 0.0, -4.0, 4.0]),
    ("Preset 10", [0.0, 2.0, -2.0, 0.0, 3.0, 5.0, 6.0, 5.0]),
    ("DM 1", [0.0, 8.0, 0.0, 12.0, 12.0, 4.0, 8.0, 10.0]),
];

impl FeatureId {
    pub const ALL: &[FeatureId] = &[
        Self::SbxMaster,
        Self::ScoutMode,
        Self::DirectMode,
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
        Self::DacFilter,
        Self::AcousticEchoCancellation,
        Self::VoiceClarityToggle,
        Self::VoiceClarityNoiseReduction,
        Self::MicEqToggle,
        Self::MicEqBand1,
        Self::MicEqBand2,
        Self::MicEqBand3,
        Self::MicEqBand4,
        Self::MicEqBand5,
        Self::MicEqBand6,
        Self::MicEqBand7,
        Self::MicEqBand8,
        Self::MicBoost,
        Self::RecordingSmartVolToggle,
        Self::Decoder,
        Self::LightingToggle,
        Self::LightingColor,
        Self::SpeakerSwitch,
        Self::SpeakerVolume,
        Self::MonitorLineInSwitch,
        Self::MonitorLineInVolume,
        Self::MonitorMicSwitch,
        Self::MonitorMicVolume,
        Self::MonitorSpdifSwitch,
        Self::MonitorSpdifVolume,
        Self::RecordLineInSwitch,
        Self::RecordLineInVolume,
        Self::RecordMicSwitch,
        Self::RecordMicVolume,
        Self::RecordSpdifSwitch,
        Self::RecordSpdifVolume,
        Self::RecordWhatUHearSwitch,
        Self::RecordWhatUHearVolume,
        Self::CaptureSource,
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

    /// The 8 Mic EQ bands, in the order of `MIC_EQ_PRESETS`.
    pub const MIC_EQ_BANDS: &[FeatureId] = &[
        Self::MicEqBand1,
        Self::MicEqBand2,
        Self::MicEqBand3,
        Self::MicEqBand4,
        Self::MicEqBand5,
        Self::MicEqBand6,
        Self::MicEqBand7,
        Self::MicEqBand8,
    ];

    /// Mixer rows as `(label, switch, volume)`, grouped like the Windows Mixer tab.
    pub const MIXER_PLAYBACK: &[(&str, FeatureId, FeatureId)] =
        &[("Speakers", Self::SpeakerSwitch, Self::SpeakerVolume)];
    pub const MIXER_MONITORING: &[(&str, FeatureId, FeatureId)] = &[
        ("Line In", Self::MonitorLineInSwitch, Self::MonitorLineInVolume),
        ("External Mic", Self::MonitorMicSwitch, Self::MonitorMicVolume),
        ("S/PDIF In", Self::MonitorSpdifSwitch, Self::MonitorSpdifVolume),
    ];
    pub const MIXER_RECORDING: &[(&str, FeatureId, FeatureId)] = &[
        ("Line In", Self::RecordLineInSwitch, Self::RecordLineInVolume),
        ("External Mic", Self::RecordMicSwitch, Self::RecordMicVolume),
        ("S/PDIF In", Self::RecordSpdifSwitch, Self::RecordSpdifVolume),
        (
            "What U Hear",
            Self::RecordWhatUHearSwitch,
            Self::RecordWhatUHearVolume,
        ),
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SbxMaster => "SBX",
            Self::ScoutMode => "Scout Mode",
            Self::DirectMode => "Direct Mode",
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
            Self::DacFilter => "DAC Filter",
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
            Self::AcousticEchoCancellation => "Acoustic Echo Cancellation",
            Self::VoiceClarityToggle => "Noise Reduction",
            Self::VoiceClarityNoiseReduction => "Noise Reduction Slider",
            Self::MicEqToggle => "Mic EQ",
            Self::MicEqBand1 => "Mic EQ Band 1",
            Self::MicEqBand2 => "Mic EQ Band 2",
            Self::MicEqBand3 => "Mic EQ Band 3",
            Self::MicEqBand4 => "Mic EQ Band 4",
            Self::MicEqBand5 => "Mic EQ Band 5",
            Self::MicEqBand6 => "Mic EQ Band 6",
            Self::MicEqBand7 => "Mic EQ Band 7",
            Self::MicEqBand8 => "Mic EQ Band 8",
            Self::MicBoost => "Mic Boost",
            Self::RecordingSmartVolToggle => "Smart Volume",
            Self::Decoder => "Decoder",
            Self::LightingToggle => "Lighting",
            Self::LightingColor => "Lighting Color",
            Self::SpeakerSwitch => "Speakers",
            Self::SpeakerVolume => "Speakers Volume",
            Self::MonitorLineInSwitch => "Monitoring Line In",
            Self::MonitorLineInVolume => "Monitoring Line In Volume",
            Self::MonitorMicSwitch => "Monitoring External Mic",
            Self::MonitorMicVolume => "Monitoring External Mic Volume",
            Self::MonitorSpdifSwitch => "Monitoring S/PDIF In",
            Self::MonitorSpdifVolume => "Monitoring S/PDIF In Volume",
            Self::RecordLineInSwitch => "Recording Line In",
            Self::RecordLineInVolume => "Recording Line In Volume",
            Self::RecordMicSwitch => "Recording External Mic",
            Self::RecordMicVolume => "Recording External Mic Volume",
            Self::RecordSpdifSwitch => "Recording S/PDIF In",
            Self::RecordSpdifVolume => "Recording S/PDIF In Volume",
            Self::RecordWhatUHearSwitch => "Recording What U Hear",
            Self::RecordWhatUHearVolume => "Recording What U Hear Volume",
            Self::CaptureSource => "Recording Source",
        }
    }

    pub fn value_kind(&self) -> ValueKind {
        match self {
            Self::SbxMaster
            | Self::ScoutMode
            | Self::DirectMode
            | Self::SurroundToggle
            | Self::DialogPlusToggle
            | Self::SmartVolToggle
            | Self::CrystalizerToggle
            | Self::BassToggle
            | Self::EqToggle
            | Self::AcousticEchoCancellation
            | Self::VoiceClarityToggle
            | Self::MicEqToggle
            | Self::RecordingSmartVolToggle
            | Self::LightingToggle
            | Self::SpeakerSwitch
            | Self::MonitorLineInSwitch
            | Self::MonitorMicSwitch
            | Self::MonitorSpdifSwitch
            | Self::RecordLineInSwitch
            | Self::RecordMicSwitch
            | Self::RecordSpdifSwitch
            | Self::RecordWhatUHearSwitch => ValueKind::Toggle,

            Self::SurroundLevel
            | Self::DialogPlusLevel
            | Self::SmartVolLevel
            | Self::CrystalizerLevel
            | Self::BassLevel
            | Self::VoiceClarityNoiseReduction
            | Self::SpeakerVolume
            | Self::MonitorLineInVolume
            | Self::MonitorMicVolume
            | Self::MonitorSpdifVolume
            | Self::RecordLineInVolume
            | Self::RecordMicVolume
            | Self::RecordSpdifVolume
            | Self::RecordWhatUHearVolume => ValueKind::Percentage,

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
            | Self::Eq16kHz
            | Self::MicEqBand1
            | Self::MicEqBand2
            | Self::MicEqBand3
            | Self::MicEqBand4
            | Self::MicEqBand5
            | Self::MicEqBand6
            | Self::MicEqBand7
            | Self::MicEqBand8 => ValueKind::Ranged {
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

            Self::DacFilter => ValueKind::Preset(&[
                "Fast Roll-off, Minimum Phase",
                "Slow Roll-off, Minimum Phase",
                "NOS (Non-Oversampling)",
                "Fast Roll-off, Linear Phase",
                "Slow Roll-off, Linear Phase",
            ]),

            Self::MicBoost => {
                ValueKind::Preset(&["0 dB", "10 dB", "20 dB", "30 dB"])
            }

            Self::Decoder => ValueKind::Preset(&["Normal", "Full", "Night"]),

            Self::CaptureSource => ValueKind::Preset(&[
                "Line In",
                "External Mic",
                "S/PDIF In",
                "What U Hear",
            ]),

            Self::LightingColor => ValueKind::Color,
        }
    }

    /// The `(family, feature_id)` pair for DSP features (families 0x95, 0x96, 0x97)
    /// that are read via `0x11` and written via `0x12`.
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
            Self::AcousticEchoCancellation => Some((0x95, 0x00)),
            Self::VoiceClarityToggle => Some((0x95, 0x04)),
            Self::VoiceClarityNoiseReduction => Some((0x95, 0x05)),
            Self::MicEqToggle => Some((0x95, 0x13)),
            Self::MicEqBand1 => Some((0x95, 0x14)),
            Self::MicEqBand2 => Some((0x95, 0x15)),
            Self::MicEqBand3 => Some((0x95, 0x16)),
            Self::MicEqBand4 => Some((0x95, 0x17)),
            Self::MicEqBand5 => Some((0x95, 0x18)),
            Self::MicEqBand6 => Some((0x95, 0x19)),
            Self::MicEqBand7 => Some((0x95, 0x1a)),
            Self::MicEqBand8 => Some((0x95, 0x1b)),
            Self::RecordingSmartVolToggle => Some((0x95, 0x2c)),
            Self::Decoder => Some((0x97, 0x02)),
            _ => None,
        }
    }

    /// Reverse lookup of `dsp_address()`.
    pub fn from_dsp_address(family: u8, feature_id: u8) -> Option<FeatureId> {
        Self::ALL
            .iter()
            .copied()
            .find(|id| id.dsp_address() == Some((family, feature_id)))
    }

    /// The ALSA simple mixer element (on card "G6") backing this feature.
    /// Returns `None` for HID features.
    pub fn alsa_element(&self) -> Option<(&'static str, AlsaDirection)> {
        use AlsaDirection::*;
        match self {
            Self::SpeakerSwitch | Self::SpeakerVolume => {
                Some(("Speaker", Playback))
            }
            Self::MonitorLineInSwitch | Self::MonitorLineInVolume => {
                Some(("Line In", Playback))
            }
            Self::MonitorMicSwitch | Self::MonitorMicVolume => {
                Some(("External Mic", Playback))
            }
            Self::MonitorSpdifSwitch | Self::MonitorSpdifVolume => {
                Some(("S/PDIF In", Playback))
            }
            Self::RecordLineInSwitch | Self::RecordLineInVolume => {
                Some(("Line In", Capture))
            }
            Self::RecordMicSwitch | Self::RecordMicVolume => {
                Some(("External Mic", Capture))
            }
            Self::RecordSpdifSwitch | Self::RecordSpdifVolume => {
                Some(("S/PDIF In", Capture))
            }
            Self::RecordWhatUHearSwitch | Self::RecordWhatUHearVolume => {
                Some(("What U Hear", Capture))
            }
            Self::CaptureSource => Some(("PCM Capture Source", Enum)),
            _ => None,
        }
    }

    /// Converts a raw DSP f32 (as sent by the device) into the cached/UI value.
    pub fn decode(&self, raw: f32) -> f32 {
        match self {
            // Device stores 0.0–0.5; UI uses 0.0–1.0
            Self::VoiceClarityNoiseReduction => raw * 2.0,
            // Device: 2.0 = Normal, 1.0 = Full, 3.0 = Night
            Self::Decoder => match raw as u32 {
                2 => 0.0,
                1 => 1.0,
                3 => 2.0,
                other => {
                    warn!("Unknown decoder mode {}", other);
                    f32::NAN
                }
            },
            _ => raw,
        }
    }

    /// Inverse of `decode()`: converts a cached/UI value into the raw DSP f32.
    pub fn encode(&self, value: f32) -> f32 {
        match self {
            Self::VoiceClarityNoiseReduction => value / 2.0,
            Self::Decoder => match value as u32 {
                1 => 1.0,
                2 => 3.0,
                _ => 2.0,
            },
            _ => value,
        }
    }

    /// Features that must be ON for this feature to work.
    ///
    /// SBX Master is intentionally never a dependency:
    /// writing an SBX effect toggle turns SBX on by itself,
    /// while turning SBX on with no effect active makes the device
    /// enable Surround on its own (verified on hardware).
    pub fn dependencies(&self) -> &'static [FeatureId] {
        match self {
            Self::SurroundLevel | Self::SurroundDistance => {
                &[Self::SurroundToggle]
            }
            Self::DialogPlusLevel => &[Self::DialogPlusToggle],
            Self::SmartVolLevel | Self::SmartVolMode => &[Self::SmartVolToggle],
            Self::CrystalizerLevel => &[Self::CrystalizerToggle],
            Self::BassLevel => &[Self::BassToggle],

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
            | Self::Eq16kHz => &[Self::EqToggle],

            Self::VoiceClarityNoiseReduction => &[Self::VoiceClarityToggle],
            Self::MicEqBand1
            | Self::MicEqBand2
            | Self::MicEqBand3
            | Self::MicEqBand4
            | Self::MicEqBand5
            | Self::MicEqBand6
            | Self::MicEqBand7
            | Self::MicEqBand8 => &[Self::MicEqToggle],

            Self::LightingColor => &[Self::LightingToggle],

            _ => &[],
        }
    }

    /// Features whose cached state should be re-queried after this feature changes.
    pub fn dependents(&self) -> &'static [FeatureId] {
        match self {
            Self::SbxMaster => &[
                Self::ScoutMode,
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
            Self::ScoutMode => &[Self::SbxMaster],
            Self::SurroundToggle => {
                &[Self::SbxMaster, Self::SurroundLevel, Self::SurroundDistance]
            }
            Self::DialogPlusToggle => &[Self::SbxMaster, Self::DialogPlusLevel],
            Self::SmartVolToggle => {
                &[Self::SbxMaster, Self::SmartVolLevel, Self::SmartVolMode]
            }
            Self::CrystalizerToggle => {
                &[Self::SbxMaster, Self::CrystalizerLevel]
            }
            Self::BassToggle => &[Self::SbxMaster, Self::BassLevel],
            Self::EqToggle => &[
                Self::SbxMaster,
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
            Self::VoiceClarityToggle => &[Self::VoiceClarityNoiseReduction],
            Self::MicEqToggle => &[
                Self::MicEqBand1,
                Self::MicEqBand2,
                Self::MicEqBand3,
                Self::MicEqBand4,
                Self::MicEqBand5,
                Self::MicEqBand6,
                Self::MicEqBand7,
                Self::MicEqBand8,
            ],
            Self::LightingToggle => &[Self::LightingColor],
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
            Self::VoiceClarityToggle => Some(Self::VoiceClarityNoiseReduction),
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
            Self::VoiceClarityNoiseReduction => Some(Self::VoiceClarityToggle),
            _ => None,
        }
    }

    /// Whether this feature is stored in / applied from profiles.
    ///
    /// The output is excluded because the device keeps a separate set of
    /// settings per output, so a profile applies to whichever output is active.
    /// The speaker volume is the system volume and shouldn't jump on profile load.
    pub fn in_profile(&self) -> bool {
        !matches!(
            self,
            Self::Output
                | Self::SpeakerSwitch
                | Self::SpeakerVolume
                | Self::DirectMode
        )
    }

    /// The value written by "Reset All", or `None` if reset leaves it alone.
    pub fn reset_value(&self) -> Option<f32> {
        match self {
            // not part of the sound settings
            Self::Output | Self::LightingToggle | Self::LightingColor => None,
            // derived from SBX: the SBX switch owns this transition and
            // writing it here would re-enumerate the device twice
            Self::DirectMode => None,
            // 0.0 would be out of range, and the factory default is unknown
            Self::SurroundDistance => None,
            // hardware mixer: resetting to 0 would mute everything
            id if id.alsa_element().is_some() => None,
            _ => Some(0.0),
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
    /// 0xRRGGBB packed into the f32 (exact, since 24 bits fit the mantissa).
    Color,
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
            // NaN (failed read) is serialized as `null`
            value: Option<f32>,
        }
        let Raw { id, value } = Raw::deserialize(deserializer)?;

        let features = all_features();
        let feature = features
            .iter()
            .find(|feature| feature.id == id)
            .ok_or_else(|| {
                D::Error::custom(format!("Unknown feature id: {}", id))
            })?;
        *feature.value.lock().unwrap() = value.unwrap_or(f32::NAN);

        Ok(feature.clone())
    }
}

impl Feature {
    /// Constructor for DSP family features (0x95/0x96/0x97)
    /// that all share the generic DSP getter/setter.
    pub fn dsp(id: FeatureId) -> Self {
        Self {
            id,
            value: Mutex::new(f32::NAN),
            getter: dsp_get,
            setter: dsp_set,
        }
    }

    /// Constructor for USB Audio Class mixer features, accessed through ALSA.
    pub fn alsa(id: FeatureId) -> Self {
        Self {
            id,
            value: Mutex::new(f32::NAN),
            getter: alsa_get,
            setter: alsa_set,
        }
    }

    /// Returns the cached value. No hardware I/O.
    /// `NaN` means the value couldn't be read (yet).
    pub fn value(&self) -> f32 {
        *self.value.lock().unwrap()
    }

    /// Overwrites the cached value without touching the hardware.
    /// Used for state the device pushes on its own.
    pub(crate) fn set_cached(&self, value: f32) {
        *self.value.lock().unwrap() = value;
    }

    /// Returns the bool interpretation of a Toggle feature's cached value.
    /// An unread value (NaN) counts as OFF.
    /// Panics if the value is anything else besides exactly 0.0 or 1.0 as a sanity check;
    /// a non-boolean value for a toggle means the protocol
    /// assumptions are broken and no functionality can be guaranteed.
    pub fn as_bool(&self) -> bool {
        let value = self.value();
        if value == 0.0 || value.is_nan() {
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
            id: FeatureId::DirectMode,
            value: Mutex::new(f32::NAN),
            getter: direct_mode_get,
            setter: direct_mode_set,
        },
        Feature {
            id: FeatureId::Output,
            value: Mutex::new(f32::NAN),
            getter: output_get,
            setter: output_set,
        },
        Feature {
            id: FeatureId::DacFilter,
            value: Mutex::new(f32::NAN),
            getter: dac_filter_get,
            setter: dac_filter_set,
        },
        // DSP 0x95 — Recording Pane
        Feature::dsp(FeatureId::AcousticEchoCancellation),
        Feature::dsp(FeatureId::VoiceClarityToggle),
        Feature::dsp(FeatureId::VoiceClarityNoiseReduction),
        Feature::dsp(FeatureId::MicEqToggle),
        Feature::dsp(FeatureId::MicEqBand1),
        Feature::dsp(FeatureId::MicEqBand2),
        Feature::dsp(FeatureId::MicEqBand3),
        Feature::dsp(FeatureId::MicEqBand4),
        Feature::dsp(FeatureId::MicEqBand5),
        Feature::dsp(FeatureId::MicEqBand6),
        Feature::dsp(FeatureId::MicEqBand7),
        Feature::dsp(FeatureId::MicEqBand8),
        Feature::dsp(FeatureId::RecordingSmartVolToggle),
        Feature {
            id: FeatureId::MicBoost,
            value: Mutex::new(f32::NAN),
            getter: gain_config_mic_boost_get,
            setter: gain_config_mic_boost_set,
        },
        // DSP 0x97 — Decoder
        Feature::dsp(FeatureId::Decoder),
        // Lighting (0x3a)
        Feature {
            id: FeatureId::LightingToggle,
            value: Mutex::new(f32::NAN),
            getter: lighting_toggle_get,
            setter: lighting_toggle_set,
        },
        Feature {
            id: FeatureId::LightingColor,
            value: Mutex::new(f32::NAN),
            getter: lighting_color_get,
            setter: lighting_color_set,
        },
        // ALSA — USB Audio Class mixer
        Feature::alsa(FeatureId::SpeakerSwitch),
        Feature::alsa(FeatureId::SpeakerVolume),
        Feature::alsa(FeatureId::MonitorLineInSwitch),
        Feature::alsa(FeatureId::MonitorLineInVolume),
        Feature::alsa(FeatureId::MonitorMicSwitch),
        Feature::alsa(FeatureId::MonitorMicVolume),
        Feature::alsa(FeatureId::MonitorSpdifSwitch),
        Feature::alsa(FeatureId::MonitorSpdifVolume),
        Feature::alsa(FeatureId::RecordLineInSwitch),
        Feature::alsa(FeatureId::RecordLineInVolume),
        Feature::alsa(FeatureId::RecordMicSwitch),
        Feature::alsa(FeatureId::RecordMicVolume),
        Feature::alsa(FeatureId::RecordSpdifSwitch),
        Feature::alsa(FeatureId::RecordSpdifVolume),
        Feature::alsa(FeatureId::RecordWhatUHearSwitch),
        Feature::alsa(FeatureId::RecordWhatUHearVolume),
        Feature::alsa(FeatureId::CaptureSource),
    ]
}

// ─── USB Packet Helpers ──────────────────────────────────────────────────────

/// How many unrelated packets we skip while waiting for a specific response.
const MAX_READ_ATTEMPTS: usize = 30;
/// The device answers within a few ms, so a timeout means nothing is coming.
const READ_TIMEOUT_MS: i32 = 500;

/// Packets that arrived while waiting for something else.
/// Most are unsolicited state pushes from the device
/// (e.g. the GlobalProfile bitmask after toggling an effect).
/// `BlasterXG6::process_device_events()` applies them to the cache.
pub(crate) static UNSOLICITED: Mutex<Vec<[u8; 64]>> = Mutex::new(Vec::new());

/// Sends one frame (`5a CMD LEN ...`), zero-padded to 64 bytes.
/// The leading report ID byte required by hidapi is added here.
fn send(frame: &[u8]) {
    let mut payload = [0u8; 65];
    payload[1..=frame.len()].copy_from_slice(frame);

    let result = DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .write(&payload);

    if let Err(error) = result {
        error!("Failed to send {:02x?} to device: {:?}", frame, error);
    }
}

pub(crate) fn read_packet(timeout_ms: i32) -> Option<[u8; 64]> {
    let mut buffer = [0u8; 64];
    match DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap()
        .read_timeout(&mut buffer, timeout_ms)
    {
        Ok(0) => None,
        Ok(_) => Some(buffer),
        Err(error) => {
            error!("Failed to read packet: {:?}", error);
            None
        }
    }
}

/// Waits for the first packet accepted by `is_match`.
/// Everything else that arrives in the meantime is queued in `UNSOLICITED`.
fn receive(
    what: &str,
    is_match: impl Fn(&[u8; 64]) -> bool,
) -> Option<[u8; 64]> {
    for attempt in 0..MAX_READ_ATTEMPTS {
        let Some(packet) = read_packet(READ_TIMEOUT_MS) else {
            error!("No response for {} (timeout)", what);
            return None;
        };

        if packet[0] == 0x5a && is_match(&packet) {
            return Some(packet);
        }

        debug!(
            "Queued unrelated packet on attempt {} while waiting for {}: {:02x?}",
            attempt + 1,
            what,
            &packet[..12]
        );
        UNSOLICITED.lock().unwrap().push(packet);
    }

    error!(
        "No matching response after {} attempts for {}",
        MAX_READ_ATTEMPTS, what
    );
    None
}

fn read_ack() {
    let Some(packet) = receive("ACK", |packet| packet[1] == 0x02) else {
        return;
    };

    // 5a 02 0a [echoed_cmd] [status] ...
    // status 0x00 = OK; 0x80/0x81/0x83 observed for unsupported requests
    if packet[4] != 0x00 {
        error!(
            "Device rejected command 0x{:02x} (status 0x{:02x}): {:02x?}",
            packet[3],
            packet[4],
            &packet[..12]
        );
    } else {
        debug!("ACK received: {:02x?}", &packet[..12]);
    }
}

/// Iterates the `(family, feature_id, raw f32)` entries of a `0x11` StatusResponse.
/// Handles both single and multi-feature packets:
/// `5a 11 [len] [count] 00 ([family] [id] [f32 LE × 4]) × count`
pub fn status_entries(
    packet: &[u8; 64],
) -> impl Iterator<Item = (u8, u8, f32)> + '_ {
    let count = if packet[0] == 0x5a && packet[1] == 0x11 {
        // at most 9 entries fit into 64 bytes
        (packet[3] as usize).min((64 - 5) / 6)
    } else {
        0
    };

    (0..count).map(move |index| {
        let offset = 5 + index * 6;
        let value = f32::from_le_bytes(
            packet[offset + 2..offset + 6]
                .try_into()
                .expect("slice is 4 bytes"),
        );
        (packet[offset], packet[offset + 1], value)
    })
}

// ─── DSP Getter/Setter (0x95/0x96/0x97 families via 0x11/0x12) ──────────────

fn dsp_get(feature: &Feature) -> f32 {
    let (family, feature_id) = feature
        .id
        .dsp_address()
        .expect("dsp_get called on non-DSP feature");

    debug!(
        "Querying DSP feature: {} (0x{:02x}:0x{:02x})",
        feature.id, family, feature_id
    );

    send(&[0x5a, 0x11, 0x03, 0x01, family, feature_id]);

    let find = |packet: &[u8; 64]| {
        status_entries(packet)
            .find(|&(f, id, _)| f == family && id == feature_id)
            .map(|(_, _, raw)| raw)
    };

    // expected: 5a 11 08 01 00 [family] [id] [f32 LE × 4]
    // but the value may also arrive inside a multi-feature push
    let Some(response) = receive(&feature.id.to_string(), |packet| {
        find(packet).is_some()
    }) else {
        return feature.value();
    };

    let raw = find(&response).expect("matched packet contains the feature");
    let value = feature.id.decode(raw);
    debug!("Read {} = {} (raw: {})", feature.id, value, raw);
    feature.set_cached(value);
    value
}

fn dsp_set(feature: &Feature, value: f32) {
    let (family, feature_id) = feature
        .id
        .dsp_address()
        .expect("dsp_set called on non-DSP feature");

    let raw = feature.id.encode(value);

    debug!(
        "Writing DSP feature: {} = {} (raw: {}, 0x{:02x}:0x{:02x})",
        feature.id, value, raw, family, feature_id
    );

    // 5a 12 07 01 [family] [id] [f32 LE × 4] — no padding byte before the f32
    let [b0, b1, b2, b3] = raw.to_le_bytes();
    send(&[0x5a, 0x12, 0x07, 0x01, family, feature_id, b0, b1, b2, b3]);

    read_ack();
}

// ─── GlobalProfile Getter/Setter (0x26) ──────────────────────────────────────
//
// Bitmask bits (verified on hardware):
//   0x01 — playback DSP active (any SBX effect or EQ on); this is "SBX"
//   0x02 — Scout Mode
//   0x04 — recording DSP active (Voice Clarity / Noise Reduction on)

fn global_profile_bit(id: FeatureId) -> u8 {
    match id {
        FeatureId::SbxMaster => 0x01,
        FeatureId::ScoutMode => 0x02,
        _ => panic!("GlobalProfile feature expected, got {:?}", id),
    }
}

fn global_profile_get(feature: &Feature) -> f32 {
    let bitmask = global_profile_bit(feature.id);

    debug!(
        "Querying global profile: {} (bit 0x{:02x})",
        feature.id, bitmask
    );

    send(&[0x5a, 0x26, 0x03, 0x08, 0xff, 0xff]);

    // expected: 5a 26 0b 08 ff ff [bitmask] ...
    let Some(response) = receive(&feature.id.to_string(), |packet| {
        packet[1] == 0x26 && packet[3] == 0x08
    }) else {
        return feature.value();
    };

    let device_bitmask = response[6];
    let value = if device_bitmask & bitmask != 0 { 1.0 } else { 0.0 };
    debug!(
        "Read {} = {} (device bitmask: 0x{:02x})",
        feature.id, value, device_bitmask
    );
    feature.set_cached(value);
    value
}

fn global_profile_set(feature: &Feature, value: f32) {
    let profile_id = global_profile_bit(feature.id);
    let state = if value > 0.0 { 0x01u8 } else { 0x00u8 };

    debug!(
        "Writing global profile: {} = {} (id: 0x{:02x}, state: 0x{:02x})",
        feature.id, value, profile_id, state
    );

    send(&[0x5a, 0x26, 0x05, 0x07, profile_id, 0x00, state, 0x00]);

    read_ack();

    // SBX and Direct Mode are mutually exclusive: Direct Mode bypasses the
    // DSP, so SBX cannot affect the audio while it is on. The SBX master
    // switch is the single user-facing control for both paths.
    if feature.id == FeatureId::SbxMaster {
        // Changing the master can make the device re-enumerate by itself
        // (turning SBX on with no effect active enables Surround). Let that
        // settle, then use a fresh descriptor for the Direct Mode check.
        std::thread::sleep(Duration::from_millis(800));
        let _ = crate::BlasterXG6::reopen();

        let enable = direct_mode_for_sbx(value);
        let already = direct_mode_read_state();

        if already == Some(enable) {
            debug!(
                "Direct Mode already {}; keeping the current enumeration",
                if enable { "on" } else { "off" }
            );
        } else {
            debug!(
                "SBX master {} -> Direct Mode {}",
                if value > 0.0 { "on" } else { "off" },
                if enable { "on" } else { "off" }
            );
            write_direct_mode(enable);
        }
    }
}

// ─── Direct Mode Getter/Setter (0x39) ───────────────────────────────────────
//
// Direct Mode is the G6's hardware DSP bypass: the USB stream goes straight
// to the DAC, so SBX, EQ, Scout and the rest of the DSP cannot affect it.
// SBX and Direct Mode are therefore mutually exclusive.
//
//   set:  5a 39 03 00 05 [01|00] 00 00 00 00
//   get:  5a 39 01 01 00 00 00 00 00 00
//         -> 5a 39 05 01 [state] 40
//
// Byte 4 bit 0x20 is Stereo Direct. Byte 5 is 0x40 on this unit and left
// alone here (likely S/PDIF Out Direct, which is not part of this switch).
//
// A set is ACKed and then the device re-enumerates its USB connection.
// Never wait for that ACK: the hidraw fd disappears underneath it. Wait for
// the new node, then read the state back; retry if the write was lost in
// the re-enumeration.

const DIRECT_MODE_STEREO_BIT: u8 = 0x20;

pub(crate) fn direct_mode_decode(state: u8) -> f32 {
    if state & DIRECT_MODE_STEREO_BIT != 0 {
        1.0
    } else {
        0.0
    }
}

/// The SBX master switch is the single user-facing control for both paths:
/// SBX off means Direct Mode on, SBX on means Direct Mode off.
pub(crate) fn direct_mode_for_sbx(value: f32) -> bool {
    value <= 0.0
}

/// Reads Stereo Direct from the device without touching the feature cache.
/// `None` means the device did not answer (usually mid-re-enumeration).
fn direct_mode_read_state() -> Option<bool> {
    send(&[
        0x5a, 0x39, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);

    let response = receive("Direct Mode", |packet| {
        packet[1] == 0x39 && packet[3] == 0x01
    })?;

    Some(direct_mode_decode(response[4]) > 0.5)
}

fn write_direct_mode(enable: bool) -> bool {
    for attempt in 0..3 {
        if attempt > 0 {
            // The previous frame may have landed just as the device
            // detached. A short pause keeps the retry out of the teardown.
            std::thread::sleep(Duration::from_millis(500));
        }

        debug!(
            "Writing Direct Mode = {} (attempt {})",
            enable,
            attempt + 1
        );
        send(&[
            0x5a,
            0x39,
            0x03,
            0x00,
            0x05,
            if enable { 0x01 } else { 0x00 },
            0x00,
            0x00,
            0x00,
            0x00,
        ]);

        // The G6 ACKs and then detaches. A ping on the old descriptor can
        // still succeed while the USB core is tearing it down, so reading
        // straight after the write races the re-enumeration and sees the
        // old state. Wait past the detach, then reopen until the new node
        // answers. do not wait for the ACK: it is on the dying descriptor.
        std::thread::sleep(Duration::from_millis(1500));

        let mut connected = false;
        for _ in 0..6 {
            if crate::BlasterXG6::reopen() && ping_quiet() {
                connected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }

        if !connected {
            warn!(
                "Direct Mode write {}: the G6 did not come back",
                attempt + 1
            );
            continue;
        }

        match direct_mode_read_state() {
            Some(state) if state == enable => {
                info!(
                    "Direct Mode is now {}",
                    if enable { "on" } else { "off" }
                );
                return true;
            }
            other => {
                // The new node can answer before the firmware has applied
                // the switch. Poll briefly instead of writing again.
                debug!(
                    "Direct Mode verify {}: wanted {}, device reports {:?}",
                    attempt + 1,
                    enable,
                    other
                );
                for _ in 0..4 {
                    std::thread::sleep(Duration::from_millis(300));
                    match direct_mode_read_state() {
                        Some(state) if state == enable => {
                            info!(
                                "Direct Mode is now {}",
                                if enable { "on" } else { "off" }
                            );
                            return true;
                        }
                        _ => {}
                    }
                }
                warn!(
                    "Direct Mode write {}: wanted {}, device reports {:?}",
                    attempt + 1,
                    enable,
                    other
                );
            }
        }
    }

    error!("Failed to set Direct Mode = {}", enable);
    false
}

fn direct_mode_get(feature: &Feature) -> f32 {
    debug!("Querying Direct Mode");

    let Some(state) = direct_mode_read_state() else {
        return feature.value();
    };

    let value = if state { 1.0 } else { 0.0 };
    debug!("Read Direct Mode = {}", value);
    feature.set_cached(value);
    value
}

fn direct_mode_set(_feature: &Feature, value: f32) {
    write_direct_mode(value > 0.5);
}

// ─── Output Getter/Setter (0x2c) ────────────────────────────────────────────
//
// Cached value: 0.0 = Speakers (0x02), 1.0 = Headphones (0x04)

pub(crate) fn output_decode(mode: u8) -> f32 {
    match mode {
        0x02 => 0.0,
        0x04 => 1.0,
        other => {
            error!("Unknown output mode: 0x{:02x}", other);
            f32::NAN
        }
    }
}

fn output_get(feature: &Feature) -> f32 {
    debug!("Querying output device");

    send(&[0x5a, 0x2c, 0x01, 0x01]);

    // expected: 5a 2c 05 01 [mode] ...
    let Some(response) = receive("Output", |packet| {
        packet[1] == 0x2c && packet[3] == 0x01
    }) else {
        return feature.value();
    };

    let value = output_decode(response[4]);
    info!(
        "Current output: {}",
        if value == 0.0 { "Speakers" } else { "Headphones" }
    );
    feature.set_cached(value);
    value
}

fn output_set(feature: &Feature, value: f32) {
    let mode = if value > 0.0 { 0x04u8 } else { 0x02u8 };

    debug!("Setting output: mode 0x{:02x}", mode);

    send(&[0x5a, 0x2c, 0x05, 0x00, mode, 0x00, 0x00, 0x00]);

    read_ack();
}

// ─── DacFilter Getter/Setter (0x6c) ─────────────────────────────────────────
//
// Device byte 0x01–0x05 maps to zero-indexed float 0.0–4.0.
// Read:  5a 6c 01 01  →  5a 6c 03 01 [filter] 00
// Write: 5a 6c 03 00 [filter] 00

fn dac_filter_get(feature: &Feature) -> f32 {
    debug!("Querying DAC filter");

    send(&[0x5a, 0x6c, 0x01, 0x01]);

    let Some(response) = receive("DAC filter", |packet| {
        packet[1] == 0x6c && packet[3] == 0x01
    }) else {
        return feature.value();
    };

    let filter_byte = response[4];
    let value = (filter_byte.saturating_sub(1)) as f32;
    info!(
        "Current DAC filter: 0x{:02x} (index {})",
        filter_byte, value
    );
    feature.set_cached(value);
    value
}

fn dac_filter_set(feature: &Feature, value: f32) {
    let filter_byte = (value as u8).saturating_add(1).clamp(0x01, 0x05);

    debug!(
        "Setting DAC filter: index {} → byte 0x{:02x}",
        value, filter_byte
    );

    send(&[0x5a, 0x6c, 0x03, 0x00, filter_byte, 0x00]);

    read_ack();
}

// ─── GainConfig Mic Boost (0x3c) ────────────────────────────────────────────
//
// 0x3c enumerates exactly one gain control (id 0x02), which is the mic boost.
// Read:  5a 3c 02 01 00        →  5a 3c 04 01 00 02 [dB]
// Write: 5a 3c 04 00 00 02 [dB]
// Preset indices 0–3 map to 0, 10, 20, 30 dB. (verified on hardware)

fn gain_config_mic_boost_get(feature: &Feature) -> f32 {
    debug!("Querying mic boost");

    send(&[0x5a, 0x3c, 0x02, 0x01, 0x00]);

    let Some(response) = receive("Mic Boost", |packet| {
        packet[1] == 0x3c && packet[3] == 0x01 && packet[5] == 0x02
    }) else {
        return feature.value();
    };

    let db = response[6];
    let index = (db / 10).min(3) as f32;
    debug!("Read Mic Boost = {} dB (index {})", db, index);
    feature.set_cached(index);
    index
}

fn gain_config_mic_boost_set(feature: &Feature, value: f32) {
    let index = value.clamp(0.0, 3.0) as u8;
    let db = index * 10;

    debug!("Setting Mic Boost: index {} → {} dB", index, db);

    send(&[0x5a, 0x3c, 0x04, 0x00, 0x00, 0x02, db]);

    read_ack();
}

// ─── Lighting (0x3a) ────────────────────────────────────────────────────────
//
// Enabled:  read 5a 3a 01 07 → 5a 3a 02 07 [0|1]
//           write 5a 3a 02 06 [0|1]
// Color:    read 5a 3a 06 0b 01 00 01 01 01 → 5a 3a 09 0b 01 00 01 01 [A B G R]
//           write 5a 3a 06 04 00 03 01 00 01            (static mode)
//                 5a 3a 09 0a 00 03 01 01 [A B G R]     (color)
// (verified on hardware; A is always 0xff)

fn lighting_toggle_get(feature: &Feature) -> f32 {
    debug!("Querying lighting state");

    send(&[0x5a, 0x3a, 0x01, 0x07]);

    let Some(response) = receive("Lighting", |packet| {
        packet[1] == 0x3a && packet[3] == 0x07
    }) else {
        return feature.value();
    };

    let value = if response[4] != 0 { 1.0 } else { 0.0 };
    debug!("Read Lighting = {}", value);
    feature.set_cached(value);
    value
}

fn lighting_toggle_set(feature: &Feature, value: f32) {
    let state = if value > 0.0 { 0x01u8 } else { 0x00u8 };

    debug!("Setting Lighting: 0x{:02x}", state);

    send(&[0x5a, 0x3a, 0x02, 0x06, state]);

    read_ack();
}

fn lighting_color_get(feature: &Feature) -> f32 {
    debug!("Querying lighting color");

    send(&[0x5a, 0x3a, 0x06, 0x0b, 0x01, 0x00, 0x01, 0x01, 0x01]);

    let Some(response) = receive("Lighting Color", |packet| {
        packet[1] == 0x3a && packet[3] == 0x0b
    }) else {
        return feature.value();
    };

    let [_alpha, blue, green, red] = [
        response[8],
        response[9],
        response[10],
        response[11],
    ];
    let rgb = u32::from_be_bytes([0, red, green, blue]);
    debug!("Read Lighting Color = #{:06x}", rgb);
    let value = rgb as f32;
    feature.set_cached(value);
    value
}

fn lighting_color_set(feature: &Feature, value: f32) {
    let [_, red, green, blue] = (value as u32).to_be_bytes();

    debug!(
        "Setting Lighting Color: #{:02x}{:02x}{:02x}",
        red, green, blue
    );

    send(&[0x5a, 0x3a, 0x06, 0x04, 0x00, 0x03, 0x01, 0x00, 0x01]);
    read_ack();

    send(&[
        0x5a, 0x3a, 0x09, 0x0a, 0x00, 0x03, 0x01, 0x01, 0xff, blue, green, red,
    ]);
    read_ack();
}

// ─── ALSA Getter/Setter (USB Audio Class mixer) ─────────────────────────────
//
// These are hardware controls inside the G6 (UAC Feature Units / Selector Unit).
// ALSA only forwards them as standard USB requests; no software processing.

fn alsa_get(feature: &Feature) -> f32 {
    let (element, direction) = feature
        .id
        .alsa_element()
        .expect("alsa_get called on non-ALSA feature");

    let result = match feature.id.value_kind() {
        ValueKind::Toggle => mixer::get_switch(element, direction),
        ValueKind::Percentage => mixer::get_volume(element, direction),
        ValueKind::Preset(_) => mixer::get_enum(element),
        other => unreachable!("ALSA feature with value kind {:?}", other),
    };

    match result {
        Ok(value) => {
            // trace, not debug: the mixer is re-read every second
            trace!("Read {} = {}", feature.id, value);
            feature.set_cached(value);
            value
        }
        Err(error) => {
            error!("Failed to read {} from ALSA: {}", feature.id, error);
            feature.value()
        }
    }
}

fn alsa_set(feature: &Feature, value: f32) {
    let (element, direction) = feature
        .id
        .alsa_element()
        .expect("alsa_set called on non-ALSA feature");

    debug!("Writing {} = {} (ALSA)", feature.id, value);

    let result = match feature.id.value_kind() {
        ValueKind::Toggle => mixer::set_switch(element, direction, value),
        ValueKind::Percentage => mixer::set_volume(element, direction, value),
        ValueKind::Preset(_) => mixer::set_enum(element, value),
        other => unreachable!("ALSA feature with value kind {:?}", other),
    };

    if let Err(error) = result {
        error!("Failed to write {} to ALSA: {}", feature.id, error);
    }
}

// ─── Connection Check ───────────────────────────────────────────────────────

/// Ping (0x06): the device echoes `5a 06 01 01`.
/// If this fails, the interrupt IN endpoint isn't delivering responses.
pub(crate) fn ping() -> bool {
    send(&[0x5a, 0x06, 0x01, 0x01]);
    receive("Ping", |packet| packet[1] == 0x06).is_some()
}

/// Like [`ping`], but without logging a timeout.
///
/// Used while the G6 is still booting: a missing answer is expected there,
/// and the caller retries. Logging every attempt would flood the journal.
pub(crate) fn ping_quiet() -> bool {
    let mut payload = [0u8; 65];
    payload[1..5].copy_from_slice(&[0x5a, 0x06, 0x01, 0x01]);

    let mut buffer = [0u8; 64];
    let mut device = DEVICE_CONNECTION
        .get()
        .expect("Device connection must be initialized")
        .lock()
        .unwrap();

    if device.write(&payload).is_err() {
        return false;
    }

    // The device can push an unsolicited state change before the ping
    // response. Keep those for `process_device_events()` instead of
    // treating the first packet as the answer.
    for _ in 0..3 {
        match device.read_timeout(&mut buffer, READ_TIMEOUT_MS) {
            Ok(length) if length > 0 => {
                if buffer[0] == 0x5a && buffer[1] == 0x06 {
                    return true;
                }
                UNSOLICITED.lock().unwrap().push(buffer);
            }
            _ => return false,
        }
    }
    false
}
