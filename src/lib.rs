#![allow(unused)]

use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, create_dir_all};
use std::io::{BufReader, BufWriter, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use tracing::{debug, warn};

// #[cfg(test)]
// mod tests;

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

const ISO_BANDS: [f64; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// ### Important
/// Many Features have a Toggle and a Slider.
/// The Slider must always be named to match this format:
/// `format!("{} Slider", feature.name)`
pub const FEATURES: &[Feature] = &[
    // Master Features (Format 2)
    Feature {
        name: "SBX",
        id: Format::Global(0x01),
        value: FeatureType::Toggle(false),
        dependencies: None,
    },
    Feature {
        name: "Scout Mode",
        id: Format::Global(0x02),
        value: FeatureType::Toggle(false),
        dependencies: None,
    },
    // SBX Features (Format 1)
    Feature {
        name: "Surround",
        id: Format::SBX(0x00),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "Surround Slider",
        id: Format::SBX(0x01),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Surround"]),
    },
    Feature {
        name: "Dialog+",
        id: Format::SBX(0x02),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "Dialog+ Slider",
        id: Format::SBX(0x03),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Dialog+"]),
    },
    Feature {
        name: "Smart Volume",
        id: Format::SBX(0x04),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "Smart Volume Slider",
        id: Format::SBX(0x05),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Smart Volume"]),
    },
    Feature {
        name: "Smart Volume Special",
        id: Format::SBX(0x06),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Smart Volume"]),
    },
    Feature {
        name: "Crystalizer",
        id: Format::SBX(0x07),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "Crystalizer Slider",
        id: Format::SBX(0x08),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Crystalizer"]),
    },
    Feature {
        name: "Equalizer",
        id: Format::SBX(0x09),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "EQ Pre-Amp",
        id: Format::SBX(0x0a),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 31Hz",
        id: Format::SBX(0x0b),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 62Hz",
        id: Format::SBX(0x0c),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 125Hz",
        id: Format::SBX(0x0d),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 250Hz",
        id: Format::SBX(0x0e),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 500Hz",
        id: Format::SBX(0x0f),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 1kHz",
        id: Format::SBX(0x10),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 2kHz",
        id: Format::SBX(0x11),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 4kHz",
        id: Format::SBX(0x12),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 8kHz",
        id: Format::SBX(0x13),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "EQ 16kHz",
        id: Format::SBX(0x14),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Equalizer"]),
    },
    Feature {
        name: "Bass",
        id: Format::SBX(0x18),
        value: FeatureType::Toggle(false),
        dependencies: Some(&["SBX"]),
    },
    Feature {
        name: "Bass Slider",
        id: Format::SBX(0x19),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "Bass"]),
    },
];

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum Format {
    Global(u8),
    SBX(u8),
    RGB(u8),
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum FeatureType {
    Toggle(bool),
    Slider(f32),
}

impl Deref for FeatureType {
    type Target = bool;

    #[track_caller]
    fn deref(&self) -> &Self::Target {
        let location = std::panic::Location::caller();
        warn!(
            "Deref FeatureType as bool is deprecated (called at {}:{}:{})",
            location.file(),
            location.line(),
            location.column(),
        );
        match self {
            FeatureType::Toggle(v) => v,
            FeatureType::Slider(_) => panic!("Cannot deref Slider as bool"),
        }
    }
}

impl DerefMut for FeatureType {
    #[track_caller]
    fn deref_mut(&mut self) -> &mut Self::Target {
        let location = std::panic::Location::caller();
        warn!(
            "Deref mut FeatureType as bool is deprecated (called at {}:{}:{})",
            location.file(),
            location.line(),
            location.column(),
        );
        match self {
            FeatureType::Toggle(v) => v,
            FeatureType::Slider(_) => panic!("Cannot deref mut Slider as bool"),
        }
    }
}

impl FeatureType {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FeatureType::Toggle(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool_mut(&mut self) -> Option<&mut bool> {
        match self {
            FeatureType::Toggle(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            FeatureType::Slider(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f32_mut(&mut self) -> Option<&mut f32> {
        match self {
            FeatureType::Slider(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(PartialEq, Clone, Debug, Serialize)]
pub struct Feature {
    pub name: &'static str,
    pub id: Format,
    pub value: FeatureType,
    pub dependencies: Option<&'static [&'static str]>,
}

impl<'de> Deserialize<'de> for Feature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>, {
        #[derive(Deserialize)]
        struct FeatureData {
            name: String,
            value: FeatureType,
        }

        let data = FeatureData::deserialize(deserializer)?;

        if let Some(static_feature) =
            FEATURES.iter().find(|f| f.name == data.name)
        {
            let mut feature = static_feature.clone();
            feature.value = data.value;
            Ok(feature)
        } else {
            Err(serde::de::Error::custom(format!(
                "Feature not found: {}",
                data.name
            )))
        }
    }
}

#[derive(Serialize)]
pub struct BlasterXG6 {
    pub features: Vec<Feature>,
    #[serde(skip)]
    pub device: DeviceInfo,
    #[serde(skip)]
    pub connection: HidDevice,
    #[serde(skip)]
    pub profile_path: PathBuf,
}

impl BlasterXG6 {
    pub fn init() -> Result<Self, Box<dyn Error>> {
        let api = HidApi::new()?;
        let device = Self::find_device(&api)?;
        let connection = device.open_device(&api)?;
        let _ = connection.set_blocking_mode(false);

        Ok(Self {
            features: FEATURES.to_vec(),
            device,
            connection,
            profile_path: PathBuf::from(format!(
                "{}linuxblaster/profiles/",
                env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!(
                    "{}/.local/share/",
                    env::var("HOME").expect("HOME is not set")
                )),
            )),
        })
    }

    /// Loads a profile from a file and creates a new BlasterXG6
    pub fn from_profile(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        todo!()
    }

    /// Saves the current state of the features to a profile
    pub fn save_profile(&self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        debug!("===== save_profile =====");
        debug!("Profile:");
        debug!("- path:         {}", path.display());

        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }

        // don't save sliders, if they're 0
        // don't save toggles, if they're off
        let mut changed_features: Vec<Feature> = Vec::new();
        self.features
            .iter()
            .for_each(|feature| match feature.value {
                FeatureType::Toggle(value) => {
                    if !value {
                        return;
                    }
                    changed_features.push(feature.clone());
                }
                FeatureType::Slider(value) => {
                    if value == 0.0 {
                        return;
                    }
                    changed_features.push(feature.clone());
                }
            });

        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &changed_features)?;
        debug!("Profile saved ¯\\_(ツ)_/¯");
        debug!("===== save_profile completed =====");

        Ok(())
    }

    pub fn apply_profile(
        &mut self,
        path: PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        let features: Vec<Feature> = self.open_profile(path)?;
        features.iter().try_for_each(|feature| {
            match feature.value {
                FeatureType::Toggle(value) => {
                    debug!("Applying toggle: {}", feature.name);
                    debug!("- value: {}", value);
                    self.set_feature(feature.name, Some(value))?;
                }
                FeatureType::Slider(value) => {
                    if value.abs() > 0.0 {
                        debug!("Applying slider: {}", feature.name);
                        debug!("- value: {}", value);
                        self.set_slider(feature.name, value)?;
                    }
                }
            }
            Ok::<(), Box<dyn Error>>(())
        })?;
        Ok(())
    }

    pub fn open_profile(
        &self,
        path: PathBuf,
    ) -> Result<Vec<Feature>, Box<dyn Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let features: Vec<Feature> = serde_json::from_reader(reader)?;
        Ok(features.clone())
    }

    /// Resets all features to their default state (Sliders: 0, Toggles: Off)
    pub fn reset(&mut self) -> Result<(), Box<dyn Error>> {
        // reset sliders first, in case they can't be changed after toggles are off
        // don't know if necessary, hard to know with a reverse engineering protocol

        // Sliders
        let slider_names: Vec<String> = self
            .features
            .iter()
            .filter(|f| matches!(f.value, FeatureType::Slider(_)))
            .map(|f| f.name.to_string())
            .collect();

        for name in slider_names {
            // EQ sliders are 0x0A-0x14, which use raw values. 0.0 is 0dB (flat).
            // Other sliders use 0-100 range, so 0.0 is 0%.
            self.set_slider(&name, 0.0)?;
        }

        // Toggles
        let toggle_names: Vec<String> = self
            .features
            .iter()
            .filter(|f| matches!(f.value, FeatureType::Toggle(_)))
            .map(|f| f.name.to_string())
            .collect();

        for name in toggle_names {
            self.set_feature(name, Some(false))?;
        }

        Ok(())
    }

    pub fn find_device(api: &HidApi) -> Result<DeviceInfo, Box<dyn Error>> {
        let device: DeviceInfo = api
            .device_list()
            .find(|device| {
                device.vendor_id() == VENDOR_ID
                    && device.product_id() == PRODUCT_ID
                    && device.interface_number() == INTERFACE
            })
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "No SoundBlaster X G6 device found",
                ))
            })
            .cloned()?;

        debug!("Found device:");
        debug!("- vendor_id:     0x{:04x}", device.vendor_id());
        debug!("- product_id:    0x{:04x}", device.product_id());
        debug!("- interface:     {}", device.interface_number());
        debug!(
            "- manufacturer:  {}",
            device.manufacturer_string().unwrap_or("Unknown")
        );
        debug!(
            "- product:       {}",
            device.product_string().unwrap_or("Unknown")
        );
        debug!(
            "- serial_number: {}",
            device.serial_number().unwrap_or("Unknown")
        );

        Ok(device)
    }

    /// Gets the dependencies of a feature
    pub fn get_dependencies(
        &self,
        feature: &str,
    ) -> Option<&'static [&'static str]> {
        self.features
            .iter()
            .find(|f| f.name == feature)
            .and_then(|f| f.dependencies)
    }

    /// Gets the features that depend on a feature
    pub fn get_dependents(&self, feature: &str) -> Vec<&'static str> {
        self.features
            .iter()
            .filter(|f| {
                f.dependencies
                    .map(|deps| deps.contains(&feature))
                    .unwrap_or(false)
            })
            .map(|f| f.name)
            .collect()
    }

    // the return type is really not that complex ...
    // it's a tuple of a Feature and an Option of a slice of strings:
    // Result<(Feature, [str]), Error>
    // but all ampercented to make them stack allocated,
    // so it might look a little weird at first ...
    #[allow(clippy::type_complexity)]
    /// Gets a Feature by name and returns it along with its dependencies
    /// ### Returns a Tuple of
    /// - The Feature
    /// - The dependencies of the Feature as an array of &str
    pub fn get_feature(
        &self,
        feature: impl Into<String> + Clone,
    ) -> Result<(&Feature, Option<&[&'static str]>), Box<dyn Error>> {
        self.features
            .iter()
            .find(|f| f.name == feature.clone().into())
            .map(|f| {
                // debug!("Found feature entry:");
                // debug!("- feature: {}", feature.clone().into());
                // debug!("- dependencies: {:?}", f.dependencies);
                (f, f.dependencies)
            })
            .ok_or_else(|| {
                debug!("Feature not found:");
                debug!("- feature: {}", feature.clone().into());
                Box::<dyn Error>::from(std::io::Error::new(
                    ErrorKind::NotFound,
                    format!("Feature {} not found", feature.clone().into()),
                ))
            })
    }

    /// Gets a mutable reference to a Feature by name
    /// ### Returns a Mutable Reference to the Feature
    /// Note that it's assumed you don't want dependencies if you need a mutable reference.
    pub fn get_feature_mut(
        &mut self,
        feature: impl Into<String> + Clone,
    ) -> Result<&mut Feature, Box<dyn Error>> {
        self.features
            .iter_mut()
            .find(|f| f.name == feature.clone().into())
            .ok_or_else(|| {
                Box::<dyn Error>::from(std::io::Error::new(
                    ErrorKind::NotFound,
                    format!("Feature {} not found", feature.clone().into()),
                ))
            })
    }

    /// #### Returns 11 Bytes, actually
    /// - 1 Byte for the Pre-Amp
    /// - 10 Bytes for the 10 EQ Bands
    pub fn get_ten_band_eq(&self) -> Option<[f32; 11]> {
        let mut bands: [f32; 11] = [0.0; 11];
        bands[0] = self.get_feature("EQ Pre-Amp").ok()?.0.value.as_f32()?;

        for (idx, band) in ISO_BANDS.iter().enumerate() {
            let feature_name = if *band < 1000.0 {
                format!("EQ {}Hz", band)
            } else {
                format!("EQ {}kHz", band / 1000.0)
            };
            let Ok(feature) = self.get_feature(feature_name) else {
                return None;
            };
            bands[idx + 1] = feature.0.value.as_f32().unwrap_or(0.0);
        }

        Some(bands)
    }

    /// Sets the Value of a Feature to On of Off
    /// ### **None**:
    /// - Toggles the feature between On and Off
    /// ### **On**:
    /// - Sets the feature to On
    /// - Sets any required dependencies to On
    /// ### **Off**:
    /// - Sets the feature to Off
    /// - Sets any dependents to Off
    pub fn set_feature(
        &mut self,
        feature: impl Into<String> + Clone,
        value: Option<bool>,
    ) -> Result<(), Box<dyn Error>> {
        debug!("===== set_feature =====");
        debug!("feature: {}", feature.clone().into());
        debug!("value:   {:?}", value);

        let (f_id, f_value, dependencies) = {
            let (f, dependencies) = self.get_feature(feature.clone())?;
            (
                f.id.clone(),
                f.value.clone(),
                dependencies.map(|d| d.to_vec()),
            )
        };
        debug!("Resolved Feature:");
        debug!("- id:           {:?}", f_id);
        debug!("- value:        {:?}", f_value);
        debug!("- dependencies: {:?}", dependencies);

        if !matches!(f_value, FeatureType::Toggle(_)) {
            debug!("Feature is not a toggle");
            return Err(Box::new(std::io::Error::new(
                ErrorKind::InvalidInput,
                format!("Feature {} is not a toggle", feature.clone().into()),
            )));
        }

        // Determine the final value: explicit value or toggle current state
        let final_value = match value {
            Some(v) => v,
            None => {
                // Toggle: invert current state
                match f_value {
                    FeatureType::Toggle(current) => !current,
                    _ => unreachable!(), // Already checked above
                }
            }
        };
        debug!("Determined final toggle value:");
        debug!("- feature:    {}", feature.clone().into());
        debug!("- final_value: {}", final_value);

        // Enable dependencies if the feature is being turned on
        if final_value {
            if let Some(dependencies) = dependencies {
                debug!("Setting required dependencies:");
                debug!("- dependencies: {:?}", dependencies);

                dependencies.iter().try_for_each(|dependency| {
                    if let Ok((f, _)) = self.get_feature(dependency.to_string())
                        && f.value.as_bool() == Some(true)
                    {
                        debug!("Dependency already enabled: {}", dependency);
                        return Ok(());
                    }
                    debug!("Enabling dependency: {}", dependency);
                    self.set_feature(dependency.to_string(), Some(true))
                })?;
            }
        }
        // Disable dependents if the feature is being turned off
        else {
            let dependents = self.get_dependents(&feature.clone().into());
            debug!("Disabling dependents:");
            debug!("- dependents: {:?}", dependents);

            for dependent in dependents {
                let Ok((feature, _)) = self.get_feature(dependent) else {
                    continue;
                };
                match feature.value {
                    FeatureType::Toggle(value) => {
                        if value {
                            debug!("Dependent already disabled: {}", dependent);
                            continue;
                        }
                        debug!("Disabling dependent feature: {}", dependent);
                        let _ = self.set_feature(dependent, Some(false));
                    }
                    FeatureType::Slider(value) => {
                        // yes this is on purpose
                        // but only temporarily; let's see how long it'll stay
                        // if value == 0.0 {
                        //     debug!("Dependent already disabled: {}", dependent);
                        //     continue;
                        // }
                        // debug!("Disabling dependent feature: {}", dependent);
                        // let _ = self.set_slider(dependent, 0.0);
                    }
                }
            }
        }

        let value_byte = if final_value { 100 } else { 0 };
        let payload = create_payload(f_id, value_byte as f32);

        debug!("Sending payload to device...");

        self.connection.write(&payload.data)?;
        self.connection.write(&payload.commit)?;

        debug!("Payload sent ¯\\_(ツ)_/¯");

        debug!("Updating feature value...");
        self.update_feature_value(
            feature.clone().into().as_str(),
            FeatureType::Toggle(final_value),
        )?;

        debug!("===== set_feature completed =====");

        Ok(())
    }

    /// Sets the Value of a Slider Feature
    /// Also sets any required dependencies to On
    pub fn set_slider(
        &mut self,
        feature: &str,
        value: f32,
    ) -> Result<(), Box<dyn Error>> {
        let (f_id, f_value, dependencies) = {
            let (f, dependencies) = self.get_feature(feature)?;
            (
                f.id.clone(),
                f.value.clone(),
                dependencies.map(|d| d.to_vec()),
            )
        };

        if !matches!(f_value, FeatureType::Slider(_)) {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::InvalidInput,
                format!("Feature {} is not a slider", feature),
            )));
        }

        if let Some(dependencies) = dependencies {
            dependencies.iter().try_for_each(|dependency| {
                if let Ok((f, _)) = self.get_feature(*dependency)
                    && let Some(false) = f.value.as_bool()
                {
                    self.set_feature(*dependency, Some(true))?;
                }
                Ok::<(), Box<dyn Error>>(())
            })?;
        }

        let payload = create_payload(f_id, value);
        self.connection.write(&payload.data)?;
        self.connection.write(&payload.commit)?;

        self.update_feature_value(feature, FeatureType::Slider(value))?;

        Ok(())
    }

    fn update_feature_value(
        &mut self,
        feature: impl Into<String> + Clone,
        value: FeatureType,
    ) -> Result<(), Box<dyn Error>> {
        debug!("===== update_feature_value =====");
        debug!("feature: {}", feature.clone().into());
        debug!("value:   {:?}", value);

        if let Some(feature_entry) = self
            .features
            .iter_mut()
            .find(|f| f.name == feature.clone().into())
        {
            debug!(
                "Updating Feature Value {} -> {:?}",
                feature.clone().into(),
                value
            );
            feature_entry.value = value;
            return Ok(());
        }

        debug!("===== update_feature_value completed =====");

        Err(Box::new(std::io::Error::new(
            ErrorKind::NotFound,
            format!(
                "Failed to update feature value for {}",
                feature.clone().into()
            ),
        )))
    }
}

pub struct Payload {
    data: [u8; 65],
    commit: [u8; 65],
}

fn create_payload(id: Format, value: f32) -> Payload {
    debug!("===== create_payload =====");
    debug!("id:      {:?}", id);
    debug!("value:   {:?}", value);
    // 65 bytes: 1 byte Report ID + 64 bytes data
    let mut data = [0u8; 65];
    let mut commit = [0u8; 65];

    data[0] = 0x00; // HID Report ID
    data[1] = 0x5a; // Magic byte
    commit[0] = 0x00; // HID Report ID
    commit[1] = 0x5a; // Magic byte

    match id {
        Format::Global(id) => {
            data[2] = 0x26;
            data[3] = 0x05;
            data[4] = 0x07;
            data[5] = id;
            data[6] = 0x00;
            data[7] = if value > 0.0 { 0x01 } else { 0x00 };

            commit[2] = 0x26;
            commit[3] = 0x03;
            commit[4] = 0x08;
            commit[5] = 0xff;
            commit[6] = 0xff;
        }
        Format::SBX(id) => {
            // EQ Sliders (0x0A - 0x14) use raw values.
            // All other SBX features (Toggles, normalized sliders) need / 100.0 normalization
            // because the UI sends 0-100 range.
            let effective_value = if (0x0a..=0x14).contains(&id) {
                value
            } else {
                value / 100.0
            };
            let value_bytes = effective_value.to_le_bytes();

            data[2] = 0x12;
            data[3] = 0x07;
            data[4] = 0x01;
            data[5] = 0x96;
            data[6] = id;
            data[7..11].copy_from_slice(&value_bytes);

            commit[2] = 0x11;
            commit[3] = 0x03;
            commit[4] = 0x01;
            commit[5] = 0x96;
            commit[6] = id;
            commit[7] = 0x00;
            commit[8] = 0x00;
            commit[9] = 0x00;
            commit[10] = 0x00;
        }
        Format::RGB(id) => {
            println!("RGB payload not implemented yet :)");
        }
    }

    // debug!(
    //     payload_head = %format_hex(&data[..12]),
    //     commit_head = %format_hex(&commit[..12]),
    //     "create_payload completed"
    // );
    debug!("create_payload completed: {}", id);
    debug!("- data:   {} : {}", &data.len(), format_hex(&data[..12]));
    debug!(
        "- commit: {} : {}",
        &commit.len(),
        format_hex(&commit[..12])
    );

    debug!("===== create_payload completed =====");

    Payload { data, commit }
}

/// Converts a 0-100 Value to 4 little-endian float bytes (0.0 - 1.0)
pub fn value_to_bytes(value: u8) -> [u8; 4] {
    let normalized = value as f32 / 100.0;
    normalized.to_le_bytes()
}

trait ToLeFloat {
    fn to_le_float(&self) -> [u8; 4];
}

impl ToLeFloat for u8 {
    fn to_le_float(&self) -> [u8; 4] {
        let normalized = *self as f32 / 100.0;
        normalized.to_le_bytes()
    }
}

fn format_hex(bytes: &[u8]) -> String {
    format!(
        "[{}]",
        bytes
            .iter()
            .map(|b| format!("0x{:02x}", b))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
