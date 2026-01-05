use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

pub mod ui;

#[cfg(test)]
mod tests;

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

/// Convert a 0-100 value to 4 little-endian float bytes
#[must_use]
pub fn value_to_bytes(value: u8) -> [u8; 4] {
    let normalized: f32 = f32::from(value) / 100.0;
    normalized.to_le_bytes()
}

#[derive(Debug)]
pub struct BlasterXG6 {
    pub device: DeviceInfo,
    pub connection: HidDevice,
    // Feature states (with sliders)
    pub surround_sound_enabled: bool,
    pub surround_sound_value: u8,
    pub crystalizer_enabled: bool,
    pub crystalizer_value: u8,
    pub bass_enabled: bool,
    pub bass_value: u8,
    pub smart_volume_enabled: bool,
    pub smart_volume_value: u8,
    pub dialog_plus_enabled: bool,
    pub dialog_plus_value: u8,
    // Toggle features
    pub night_mode_enabled: bool,
    pub loud_mode_enabled: bool,
    pub equalizer_enabled: bool,
    // EQ pre-amp (dB value, -12.0 to +12.0)
    pub pre_amp: f32,
    // EQ bands (dB values, -12.0 to +12.0)
    pub eq_bands: [f32; 10],
}

impl BlasterXG6 {
    pub fn init() -> Result<Self, Box<dyn Error>> {
        let api = HidApi::new()?;
        let device = api
            .device_list()
            .find(|device| {
                device.vendor_id() == VENDOR_ID
                    && device.product_id() == PRODUCT_ID
                    && device.interface_number() == INTERFACE
            })
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "Device not found",
                )) as Box<dyn Error>
            })?;

        let connection = device.open_device(&api)?;
        let _ = connection.set_blocking_mode(false);

        Ok(Self {
            device: device.clone(),
            connection,
            // Initialize all feature states to defaults
            surround_sound_enabled: false,
            surround_sound_value: 50,
            crystalizer_enabled: false,
            crystalizer_value: 50,
            bass_enabled: false,
            bass_value: 50,
            smart_volume_enabled: false,
            smart_volume_value: 50,
            dialog_plus_enabled: false,
            dialog_plus_value: 50,
            night_mode_enabled: false,
            loud_mode_enabled: false,
            equalizer_enabled: false,
            pre_amp: 0.0,
            eq_bands: [0.0; 10],
        })
    }

    /// - sets all Features to OFF
    /// - sets all EQ bands to 0 dB
    pub fn reset(&mut self) -> Result<(), Box<dyn Error>> {
        self.disable(SoundFeature::SurroundSound)?;
        self.disable(SoundFeature::Crystalizer)?;
        self.disable(SoundFeature::Bass)?;
        self.disable(SoundFeature::SmartVolume)?;
        self.disable(SoundFeature::DialogPlus)?;
        self.disable(SoundFeature::NightMode)?;
        self.disable(SoundFeature::LoudMode)?;
        self.disable(SoundFeature::Equalizer)?;

        // Reset pre-amp
        self.set_pre_amp_db(0.0)?;

        for band in Equalizer::default().bands() {
            self.set_eq_band_db(band, 0.0)?;
        }

        Ok(())
    }

    pub fn enable(
        &mut self,
        feature: SoundFeature,
    ) -> Result<(), Box<dyn Error>> {
        let value = match feature {
            // NightMode uses special float value 2.0 (200/100)
            SoundFeature::NightMode => {
                self.night_mode_enabled = true;
                self.loud_mode_enabled = false; // Mutually exclusive
                self.smart_volume_enabled = true; // Mode of SmartVolume
                200
            }
            SoundFeature::LoudMode => {
                self.loud_mode_enabled = true;
                self.night_mode_enabled = false; // Mutually exclusive
                self.smart_volume_enabled = true; // Mode of SmartVolume
                100
            }
            SoundFeature::SurroundSound => {
                self.surround_sound_enabled = true;
                100
            }
            SoundFeature::Crystalizer => {
                self.crystalizer_enabled = true;
                100
            }
            SoundFeature::Bass => {
                self.bass_enabled = true;
                100
            }
            SoundFeature::SmartVolume => {
                self.smart_volume_enabled = true;
                100
            }
            SoundFeature::DialogPlus => {
                self.dialog_plus_enabled = true;
                100
            }
            SoundFeature::Equalizer => {
                self.equalizer_enabled = true;
                100
            }
            SoundFeature::EqBand(_) => 100, // EQ bands don't have enable/disable
        };
        let payload = Self::create_payload(feature.id(), value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn disable(
        &mut self,
        feature: SoundFeature,
    ) -> Result<(), Box<dyn Error>> {
        match feature {
            SoundFeature::NightMode => self.night_mode_enabled = false,
            SoundFeature::LoudMode => self.loud_mode_enabled = false,
            SoundFeature::SurroundSound => self.surround_sound_enabled = false,
            SoundFeature::Crystalizer => self.crystalizer_enabled = false,
            SoundFeature::Bass => self.bass_enabled = false,
            SoundFeature::SmartVolume => self.smart_volume_enabled = false,
            SoundFeature::DialogPlus => self.dialog_plus_enabled = false,
            SoundFeature::Equalizer => self.equalizer_enabled = false,
            SoundFeature::EqBand(_) => {} // EQ bands don't have enable/disable
        }
        let payload = Self::create_payload(feature.id(), 0)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn set_slider(
        &mut self,
        feature: SoundFeature,
        value: u8,
    ) -> Result<(), Box<dyn Error>> {
        match feature {
            SoundFeature::SurroundSound => self.surround_sound_value = value,
            SoundFeature::Crystalizer => self.crystalizer_value = value,
            SoundFeature::Bass => self.bass_value = value,
            SoundFeature::SmartVolume => self.smart_volume_value = value,
            SoundFeature::DialogPlus => self.dialog_plus_value = value,
            _ => {} // Other features don't have sliders
        }
        let payload = Self::create_payload(feature.id() + 1, value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn set_eq_band(
        &mut self,
        band: EqBand,
        value: u8,
    ) -> Result<(), Box<dyn Error>> {
        // Convert u8 (0-100) to dB value (-12.0 to +12.0)
        // Assuming 0 = -12dB, 50 = 0dB, 100 = +12dB
        let db_value = (f32::from(value) / 100.0).mul_add(24.0, -12.0);
        self.set_eq_band_db(band, db_value)
    }

    /// Set pre-amp using raw dB value, clamped to -12.0..=12.0
    pub fn set_pre_amp_db(
        &mut self,
        db_value: f32,
    ) -> Result<(), Box<dyn Error>> {
        let clamped = db_value.clamp(-12.0, 12.0);
        self.pre_amp = clamped;

        let pre_amp_band = Equalizer::default().band_pre_amp;
        let payload =
            Self::create_payload_raw(pre_amp_band.feature_id, clamped)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    /// Set EQ band using raw dB value, clamped to -12.0..=12.0
    pub fn set_eq_band_db(
        &mut self,
        band: EqBand,
        db_value: f32,
    ) -> Result<(), Box<dyn Error>> {
        let clamped = db_value.clamp(-12.0, 12.0);

        // Update the corresponding EQ band value
        let eq_bands = Equalizer::default().bands();
        if let Some(index) = eq_bands
            .iter()
            .position(|b| b.feature_id == band.feature_id)
        {
            self.eq_bands[index] = clamped;
        }

        let payload = Self::create_payload_raw(band.feature_id, clamped)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    fn send_payload(&self, payload: &Payload) -> Result<(), Box<dyn Error>> {
        self.connection.write(&payload.data)?;
        self.connection.write(&payload.commit)?;
        Ok(())
    }

    pub fn create_payload(
        feature_id: u8,
        value: u8,
    ) -> Result<Payload, Box<dyn Error>> {
        Self::create_payload_raw(feature_id, f32::from(value) / 100.0)
    }

    /// Create payload with raw float value (no normalization)
    pub fn create_payload_raw(
        feature_id: u8,
        value: f32,
    ) -> Result<Payload, Box<dyn Error>> {
        let value_bytes = value.to_le_bytes();

        // DATA packet: 65 bytes (1 report ID + 64 data)
        let mut data = vec![0u8; 65];
        data[0] = 0x00; // Report ID
        data[1] = 0x5a; // Magic byte
        data[2] = 0x12; // Request type high byte (DATA)
        data[3] = 0x07; // Request type low byte
        data[4] = 0x01; // Intermediate high
        data[5] = 0x96; // Intermediate low
        data[6] = feature_id;
        data[7..11].copy_from_slice(&value_bytes);

        // COMMIT packet: 65 bytes
        let mut commit = vec![0u8; 65];
        commit[0] = 0x00; // Report ID
        commit[1] = 0x5a; // Magic byte
        commit[2] = 0x11; // Request type high byte (COMMIT)
        commit[3] = 0x03; // Request type low byte
        commit[4] = 0x01; // Intermediate high
        commit[5] = 0x96; // Intermediate low
        commit[6] = feature_id;

        let payload = Payload { data, commit };

        #[cfg(debug_assertions)]
        {
            println!("DATA:   {:02x?}", &payload.data[..12]);
            println!("COMMIT: {:02x?}", &payload.commit[..12]);
        }

        // the commit packet is mostly hardcoded;
        // but just as a sanity check,
        // in case I change the implementation in the future
        assert_eq!(
            payload.commit[..8],
            [0x00, 0x5a, 0x11, 0x03, 0x01, 0x96, feature_id, 0x00][..]
        );

        Ok(payload)
    }

    /// Create a preset from the current device state
    #[must_use]
    pub fn to_preset(&self, name: String) -> Preset {
        let mut features = Vec::new();

        // Add slider features with their values
        if self.surround_sound_enabled {
            features
                .push((SoundFeature::SurroundSound, self.surround_sound_value));
        }
        if self.crystalizer_enabled {
            features.push((SoundFeature::Crystalizer, self.crystalizer_value));
        }
        if self.bass_enabled {
            features.push((SoundFeature::Bass, self.bass_value));
        }
        if self.smart_volume_enabled {
            features.push((SoundFeature::SmartVolume, self.smart_volume_value));
        }
        if self.dialog_plus_enabled {
            features.push((SoundFeature::DialogPlus, self.dialog_plus_value));
        }

        // Add toggle features (value 100 = enabled, 0 = disabled)
        if self.night_mode_enabled {
            features.push((SoundFeature::NightMode, 200)); // Special value for NightMode
        }
        if self.loud_mode_enabled {
            features.push((SoundFeature::LoudMode, 100));
        }
        if self.equalizer_enabled {
            features.push((SoundFeature::Equalizer, 100));
        }

        // Add pre-amp
        let mut eq_bands = Vec::new();
        let pre_amp_band = Equalizer::default().band_pre_amp;
        eq_bands.push((pre_amp_band, self.pre_amp));

        // Add EQ bands
        let eq_band_defs = Equalizer::default().bands();
        for (i, band) in eq_band_defs.iter().enumerate() {
            eq_bands.push((*band, self.eq_bands[i]));
        }

        Preset {
            name,
            features,
            eq_bands,
        }
    }

    /// Apply a preset to the device
    pub fn apply_preset(
        &mut self,
        preset: &Preset,
    ) -> Result<(), Box<dyn Error>> {
        // First, reset everything to a clean state
        self.reset()?;

        // Apply features
        for (feature, value) in &preset.features {
            match feature {
                SoundFeature::SurroundSound
                | SoundFeature::Crystalizer
                | SoundFeature::Bass
                | SoundFeature::SmartVolume
                | SoundFeature::DialogPlus => {
                    if *value > 0 {
                        self.enable(*feature)?;
                        self.set_slider(*feature, *value)?;
                    }
                }
                SoundFeature::NightMode
                | SoundFeature::LoudMode
                | SoundFeature::Equalizer => {
                    if *value > 0 {
                        self.enable(*feature)?;
                    }
                }
                SoundFeature::EqBand(_) => {
                    // EQ bands are handled separately
                }
            }
        }

        // Apply EQ bands (includes pre-amp)
        for (band, db_value) in &preset.eq_bands {
            if band.feature_id == Equalizer::default().band_pre_amp.feature_id {
                self.set_pre_amp_db(*db_value)?;
            } else {
                self.set_eq_band_db(*band, *db_value)?;
            }
        }

        Ok(())
    }
}

/// Get the presets directory path
pub fn presets_dir() -> Result<PathBuf, Box<dyn Error>> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set")?;
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("blaster_x_g6_control");
    path.push("presets");
    Ok(path)
}

/// Ensure the presets directory exists
pub fn ensure_presets_dir() -> Result<PathBuf, Box<dyn Error>> {
    let dir = presets_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Get the file path for a preset
pub fn preset_path(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut path = presets_dir()?;
    // Sanitize filename
    let sanitized = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    path.push(format!("{sanitized}.json"));
    Ok(path)
}

/// saves current settings to a preset
pub fn save_preset(
    device: &BlasterXG6,
    name: String,
) -> Result<(), Box<dyn Error>> {
    let preset = device.to_preset(name.clone());
    let path = preset_path(&name)?;
    ensure_presets_dir()?;

    let json = serde_json::to_string_pretty(&preset)?;
    fs::write(&path, json)?;
    Ok(())
}

/// loads a preset from disk and applies it
pub fn load_preset(
    device: &mut BlasterXG6,
    preset: &Preset,
) -> Result<(), Box<dyn Error>> {
    device.apply_preset(preset)?;
    Ok(())
}

/// loads a preset by name from disk
pub fn load_preset_by_name(
    device: &mut BlasterXG6,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    let path = preset_path(name)?;
    let json = fs::read_to_string(&path)?;
    let preset: Preset = serde_json::from_str(&json)?;
    load_preset(device, &preset)
}

/// removes a preset from disk
pub fn delete_preset(preset: &Preset) -> Result<(), Box<dyn Error>> {
    let path = preset_path(&preset.name)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// removes a preset by name from disk
pub fn delete_preset_by_name(name: &str) -> Result<(), Box<dyn Error>> {
    let path = preset_path(name)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// lists all presets on disk
pub fn list_presets() -> Result<Vec<Preset>, Box<dyn Error>> {
    let dir = presets_dir()?;
    let mut presets = Vec::new();

    if !dir.exists() {
        return Ok(presets);
    }

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            match fs::read_to_string(&path) {
                Ok(json) => {
                    if let Ok(preset) = serde_json::from_str::<Preset>(&json) {
                        presets.push(preset);
                    }
                }
                Err(_) => {
                    // Skip files that can't be read or parsed
                    continue;
                }
            }
        }
    }

    // Sort by name
    presets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(presets)
}

#[derive(Debug)]
pub struct Payload {
    pub data: Vec<u8>,
    pub commit: Vec<u8>,
}

/// # Feature IDs
/// Sliders are `feature_id` + 1
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SoundFeature {
    SurroundSound,
    Crystalizer,
    Bass,
    SmartVolume,
    DialogPlus,
    NightMode,
    LoudMode,
    Equalizer,
    EqBand(EqBand),
}

impl SoundFeature {
    /// # Feature IDs
    /// Sliders are `feature_id` + 1
    #[must_use]
    pub const fn id(&self) -> u8 {
        match self {
            Self::SurroundSound => 0x00,
            Self::Crystalizer => 0x07,
            Self::Bass => 0x18,
            Self::SmartVolume => 0x04,
            Self::DialogPlus => 0x02,
            Self::NightMode => 0x06,
            Self::LoudMode => 0x06,
            Self::Equalizer => 0x09,
            Self::EqBand(band) => band.feature_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EqBand {
    pub value: u8,
    pub feature_id: u8,
}

pub struct Equalizer {
    pub band_pre_amp: EqBand,
    pub band_31: EqBand,
    pub band_62: EqBand,
    pub band_125: EqBand,
    pub band_250: EqBand,
    pub band_500: EqBand,
    pub band_1k: EqBand,
    pub band_2k: EqBand,
    pub band_4k: EqBand,
    pub band_8k: EqBand,
    pub band_16k: EqBand,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self {
            band_pre_amp: EqBand {
                value: 0,
                feature_id: 0x0a,
            },
            band_31: EqBand {
                value: 0,
                feature_id: 0x0b,
            },
            band_62: EqBand {
                value: 0,
                feature_id: 0x0c,
            },
            band_125: EqBand {
                value: 0,
                feature_id: 0x0d,
            },
            band_250: EqBand {
                value: 0,
                feature_id: 0x0e,
            },
            band_500: EqBand {
                value: 0,
                feature_id: 0x0f,
            },
            band_1k: EqBand {
                value: 0,
                feature_id: 0x10,
            },
            band_2k: EqBand {
                value: 0,
                feature_id: 0x11,
            },
            band_4k: EqBand {
                value: 0,
                feature_id: 0x12,
            },
            band_8k: EqBand {
                value: 0,
                feature_id: 0x13,
            },
            band_16k: EqBand {
                value: 0,
                feature_id: 0x14,
            },
        }
    }
}

impl Equalizer {
    /// Returns all 10 EQ bands as an array
    #[must_use]
    pub const fn bands(&self) -> [EqBand; 10] {
        [
            self.band_31,
            self.band_62,
            self.band_125,
            self.band_250,
            self.band_500,
            self.band_1k,
            self.band_2k,
            self.band_4k,
            self.band_8k,
            self.band_16k,
        ]
    }
}

#[derive(Copy, Clone)]
pub enum RequestType {
    Data = 0x1207,
    Commit = 0x1103,
}

impl Display for RequestType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04x}", *self as u16)
    }
}

pub enum Slider {
    Enabled(SliderValue),
    Disabled,
}

pub struct SliderValue {
    pub value: u8,
    pub hex: String,
}

/// Preset structure for saving/loading device configurations
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Preset {
    pub name: String,
    pub features: Vec<(SoundFeature, u8)>,
    pub eq_bands: Vec<(EqBand, f32)>,
}
