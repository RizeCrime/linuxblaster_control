#![allow(unused)]

use hidapi::{DeviceInfo, HidApi, HidDevice};
use rusb;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use tracing::{debug, error, info, warn};

#[cfg(test)]
mod tests;

pub mod features;
pub use features::{Feature, FeatureId, ValueKind};

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

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

#[derive(Serialize, Deserialize)]
pub struct BlasterXG6 {
    pub features: Vec<Feature>,
}

impl BlasterXG6 {
    pub fn init() -> Self {
        let device_connection = DEVICE_CONNECTION.get_or_init(|| {
            Self::reset_usb();

            let api = HidApi::new().expect("Failed to create HID API");
            let device =
                Self::find_device(&api).expect("Failed to find device");
            let connection = device
                .open_device(&api)
                .expect("Failed to open device connection");
            Mutex::new(connection)
        });

        let features = features::all_features();

        if device_connection
            .lock()
            .unwrap()
            .set_blocking_mode(false)
            .is_err()
        {
            warn!("Failed to set blocking mode to false");
            warn!("Continuing with blocking mode in unknown state...");
        }

        let blaster = Self { features };
        blaster.read_state_from_device();
        blaster
    }

    pub fn reset_usb() {
        let Some(handle) =
            rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID)
        else {
            error!("Failed to open device for USB reset");
            return;
        };

        info!("Resetting device...");
        let _ = handle.reset();
        drop(handle);
        // the kernel needs time to re-enumerate,
        // and the device sends some initial data we don't care about
        std::thread::sleep(std::time::Duration::from_secs(2));
        info!("Device reset complete.");
    }

    /// Queries every feature from hardware individually.
    /// This will update the internal state of the features with the current hardware values.
    pub fn read_state_from_device(&self) {
        for feature in &self.features {
            feature.read_from_device();
        }
    }

    /// Lookup a feature by ID.
    pub fn feature(&self, id: FeatureId) -> &Feature {
        self.features
            .iter()
            .find(|feature| feature.id == id)
            .unwrap_or_else(|| panic!("Feature {:?} not registered", id))
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
                if feature.value() == 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
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

        Ok(())
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

    pub fn is_connected(&self) -> bool {
        // check if device connection is healthy

        todo!()
    }

    /// ### Important
    /// This does **not** reset the USB Connection!
    ///
    /// This is a managed reset function that takes device quirks into account.
    /// It resets "all" features to their default values, where "all"
    /// means a hard-coded list of features that this software touches.
    ///
    /// Does not reset Output device.
    ///
    /// If this software doesn't modify a feature, it won't be included in the reset.
    /// I don't know where this would become relevant,
    /// but I figured it'd be worth noting down.
    pub fn reset(&self) -> Result<(), Box<dyn Error>> {
        // yeah the return type might be stupid, I'll fix it soon™️
        let features: Vec<Feature> = features::all_features();
        features.iter().for_each(|feature| {
            if feature.id == FeatureId::Output {
                return;
            }
            feature.write_to_device(0.0);
        });

        self.read_state_from_device();

        Ok(())
    }

    pub fn save_profile(&self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        // save the current state of the features to a profile

        let json = serde_json::to_string_pretty(&self)?;
        std::fs::write(&path, json)?;
        info!("Saved Profile to {:?}", path);

        Ok(())
    }

    pub fn apply_profile(&self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        // apply a profile to the features

        let json = std::fs::read_to_string(&path)?;
        let saved: BlasterXG6 = serde_json::from_str(&json)?;

        for feature in &saved.features {
            // don't write features that haven't been changed from defautl
            if feature.value() == 0.0 {
                continue;
            }

            // don't write sliders if their toggle is off
            if matches!(feature.id.value_kind(), ValueKind::Ranged { .. }) {
                if let Some(toggle_id) = feature.id.paired_toggle() {
                    if self.feature(toggle_id).value() == 0.0 {
                        continue;
                    }
                };
            }

            self.set_feature(feature.id, Some(feature.value()))?;
        }

        info!("Applied Profile ({:?})", path);

        Ok(())
    }
}
