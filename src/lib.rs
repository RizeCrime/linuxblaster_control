#![allow(unused)]

use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::ErrorKind;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use tracing::{debug, warn};

// #[cfg(test)]
// mod tests;

mod app;

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

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
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 31Hz",
        id: Format::SBX(0x0b),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 62Hz",
        id: Format::SBX(0x0c),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 125Hz",
        id: Format::SBX(0x0d),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 250Hz",
        id: Format::SBX(0x0e),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 500Hz",
        id: Format::SBX(0x0f),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 1kHz",
        id: Format::SBX(0x10),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 2kHz",
        id: Format::SBX(0x11),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 4kHz",
        id: Format::SBX(0x12),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 8kHz",
        id: Format::SBX(0x13),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
    },
    Feature {
        name: "EQ 16kHz",
        id: Format::SBX(0x14),
        value: FeatureType::Slider(0.0),
        dependencies: Some(&["SBX", "EQ"]),
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

#[derive(PartialEq, Clone, Debug)]
pub enum Format {
    Global(u8),
    SBX(u8),
    RGB(u8),
}

#[derive(PartialEq, Clone, Debug)]
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

#[derive(PartialEq, Clone, Debug)]
pub struct Feature {
    pub name: &'static str,
    pub id: Format,
    pub value: FeatureType,
    pub dependencies: Option<&'static [&'static str]>,
}

pub struct BlasterXG6 {
    pub device: DeviceInfo,
    pub connection: HidDevice,
    features: Vec<Feature>,
}

impl BlasterXG6 {
    pub fn init() -> Result<Self, Box<dyn Error>> {
        let device = Self::find_device()?;

        let api = HidApi::new()?;
        let connection = device.open_device(&api)?;
        let _ = connection.set_blocking_mode(false);

        Ok(Self {
            device: device.clone(),
            connection,
            features: FEATURES.to_vec(),
        })
    }

    pub fn find_device() -> Result<DeviceInfo, Box<dyn Error>> {
        let api = HidApi::new()?;
        let device = api
            .device_list()
            .find(|device| {
                device.vendor_id() == VENDOR_ID
                    && device.product_id() == PRODUCT_ID
            })
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "No SoundBlaster X G6 device found",
                ))
            })
            .cloned()?;
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
    fn get_feature(
        &self,
        feature: impl Into<String> + Clone,
    ) -> Result<(&Feature, Option<&[&'static str]>), Box<dyn Error>> {
        self.features
            .iter()
            .find(|f| f.name == feature.clone().into())
            .map(|f| {
                debug!(
                    feature = %feature.clone().into(),
                    dependencies = ?f.dependencies,
                    "Found feature entry"
                );
                (f, f.dependencies)
            })
            .ok_or_else(|| {
                debug!(feature = %feature.clone().into(), "Feature not found");
                Box::<dyn Error>::from(std::io::Error::new(
                    ErrorKind::NotFound,
                    format!("Feature {} not found", feature.clone().into()),
                ))
            })
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
        debug!(feature = %feature.clone().into(), value = ?value, "Setting feature");

        let (f_id, f_value, dependencies) = {
            let (f, dependencies) = self.get_feature(feature.clone())?;
            (
                f.id.clone(),
                f.value.clone(),
                dependencies.map(|d| d.to_vec()),
            )
        };
        debug!(feature_value = ?f_value, "Resolved feature entry");
        debug!(dependencies = ?dependencies, "Feature dependencies");

        if !matches!(f_value, FeatureType::Toggle(_)) {
            debug!(feature = %feature.clone().into(), "Feature is not a toggle");
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
        debug!(
            feature = %feature.clone().into(),
            final_value,
            "Determined final toggle value"
        );

        // Only set dependencies if we're turning the feature on
        if final_value {
            if let Some(dependencies) = dependencies {
                debug!(feature = %feature.clone().into(), "Setting required dependencies");
                dependencies.iter().try_for_each(|dependency| {
                    debug!(dependency = %dependency, "Enabling dependency");
                    self.set_feature(dependency.to_string(), Some(true))
                })?;
            }
        } else {
            let dependents = self.get_dependents(&feature.clone().into());
            for dependent in dependents {
                // Only disable if it's a toggle feature
                if let Ok((f, _)) = self.get_feature(dependent)
                    && matches!(f.value, FeatureType::Toggle(_))
                {
                    debug!(dependent = %dependent, "Disabling dependent feature");
                    self.set_feature(dependent, Some(false))?;
                }
            }
        }

        let value_byte = if final_value { 100 } else { 0 };
        let payload_array = create_payload(f_id, value_byte as f32);
        debug!(
            feature = %feature.clone().into(),
            value_byte,
            payload_head = %format_hex(&payload_array[..10]),
            "Prepared payload"
        );
        self.connection.write(&payload_array)?;

        debug!(feature = %feature.clone().into(), final_value, "Updating feature");
        self.update_feature_value(
            feature.clone().into().as_str(),
            FeatureType::Toggle(final_value),
        )?;

        debug!(feature = %feature.clone().into(), "Payload sent");

        Ok(())
    }

    /// Sets the Value of a Slider Feature
    /// Also sets any required dependencies to On
    pub fn set_slider(
        &mut self,
        feature: &'static str,
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

        if let Some(dependencies) = dependencies {
            dependencies.iter().try_for_each(|dependency| {
                self.set_feature(*dependency, Some(true))
            })?;
        }

        if !matches!(f_value, FeatureType::Slider(_)) {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::InvalidInput,
                format!("Feature {} is not a slider", feature),
            )));
        }

        let payload: &[u8] = &create_payload(f_id, value);
        self.connection.write(payload)?;

        self.update_feature_value(feature, FeatureType::Slider(value))?;

        Ok(())
    }

    fn update_feature_value(
        &mut self,
        feature: impl Into<String> + Clone,
        value: FeatureType,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(feature_entry) = self
            .features
            .iter_mut()
            .find(|f| f.name == feature.clone().into())
        {
            feature_entry.value = value;
            return Ok(());
        }
        Err(Box::new(std::io::Error::new(
            ErrorKind::NotFound,
            format!(
                "Failed to update feature value for {}",
                feature.clone().into()
            ),
        )))
    }
}

fn create_payload(id: Format, value: f32) -> [u8; 64] {
    debug!(?id, value, "create_payload called");
    let value_bytes = value.to_le_bytes();

    let mut payload = [0u8; 64];
    let mut commit = [0u8; 64];

    payload[0] = 0x5a; // Magic byte 
    commit[0] = 0x5a; // Magic byte 

    match id {
        Format::Global(id) => {
            payload[1] = 0x26;
            payload[2] = 0x05;
            payload[3] = 0x07;
            payload[4] = id;
            payload[5] = 0x00;
            payload[6..10].copy_from_slice(&value_bytes);

            commit[1] = 0x26;
            commit[2] = 0x03;
            commit[3] = 0x08;
            commit[4] = 0xff;
            commit[5] = 0xff;
        }
        Format::SBX(id) => {
            payload[1] = 0x12;
            payload[2] = 0x07;
            payload[3] = 0x01;
            payload[4] = 0x96;
            payload[5] = id;
            payload[6..10].copy_from_slice(&value_bytes);

            commit[1] = 0x11;
            commit[2] = 0x03;
            commit[3] = 0x01;
            commit[4] = 0x96;
            commit[5] = id;
            commit[6] = 0x00;
            commit[7] = 0x00;
            commit[8] = 0x00;
            commit[9] = 0x00;
        }
        Format::RGB(id) => {
            // RGB uses 0x3a command family
            // Pattern: [0x5a] [0x3a] [subcmd] [params...]
            // For basic toggle: [0x5a] [0x3a] [0x02] [0x06] [state] [0x00] ...
            // state: 0x00 = OFF, 0x01 = ON
            payload[1] = 0x3a; // Command family
            payload[2] = 0x02; // Sub-command
            payload[3] = 0x06; // Parameter
            payload[4] = if value > 0.0 { 0x01 } else { 0x00 }; // State: ON if value > 0, OFF if 0
            payload[5] = 0x00;

            // RGB doesn't use a commit pattern like Format 1/2, but populate commit array anyway
            // (Note: RGB ON actually requires 3 commands total, but this function only returns one)
            commit[1] = 0x3a;
            commit[2] = 0x02;
            commit[3] = 0x06;
            commit[4] = if value > 0.0 { 0x01 } else { 0x00 };
            commit[5] = 0x00;
        }
    }

    debug!(
        payload_head = %format_hex(&payload[..10]),
        commit_head = %format_hex(&commit[..10]),
        "create_payload completed"
    );
    payload
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
