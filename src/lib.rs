#![allow(unused)]

use hidapi::{DeviceInfo, HidApi, HidDevice};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;

pub const VENDOR_ID: u16 = 0x041e;
pub const PRODUCT_ID: u16 = 0x3256;
pub const INTERFACE: i32 = 4;

/// Convert a 0-100 value to 4 little-endian float bytes
pub fn value_to_bytes(value: u8) -> [u8; 4] {
    let normalized: f32 = value as f32 / 100.0;
    normalized.to_le_bytes()
}

#[derive(Debug)]
pub struct BlasterXG6 {
    pub device: DeviceInfo,
    pub connection: HidDevice,
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
            .ok_or(Box::new(std::io::Error::new(
                ErrorKind::NotFound,
                "Device not found",
            )))?;

        let connection = device.open_device(&api)?;
        connection.set_blocking_mode(false);

        Ok(Self {
            device: device.clone(),
            connection,
        })
    }

    /// - sets all Features to OFF
    /// - sets all EQ bands to 0 dB
    pub fn reset(&self) -> Result<(), Box<dyn Error>> {
        self.disable(SoundFeature::SurroundSound)?;
        self.disable(SoundFeature::Crystalizer)?;
        self.disable(SoundFeature::Bass)?;
        self.disable(SoundFeature::SmartVolume)?;
        self.disable(SoundFeature::DialogPlus)?;
        self.disable(SoundFeature::NightMode)?;
        self.disable(SoundFeature::Equalizer)?;

        for band in Equalizer::default().bands() {
            self.set_eq_band_db(band, 0.0)?;
        }

        Ok(())
    }

    pub fn enable(&self, feature: SoundFeature) -> Result<(), Box<dyn Error>> {
        let value = match feature {
            // NightMode uses special float value 2.0 (200/100)
            SoundFeature::NightMode => 200,
            _ => 100,
        };
        let payload = self.create_payload(feature.id(), value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn disable(&self, feature: SoundFeature) -> Result<(), Box<dyn Error>> {
        let value = match feature {
            // Disabling NightMode = Loud mode (1.0 = 100)
            SoundFeature::NightMode => 100,
            _ => 0,
        };
        let payload = self.create_payload(feature.id(), value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn set_slider(
        &self,
        feature: SoundFeature,
        value: u8,
    ) -> Result<(), Box<dyn Error>> {
        let payload = self.create_payload(feature.id() + 1, value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    pub fn set_eq_band(
        &self,
        band: EqBand,
        value: u8,
    ) -> Result<(), Box<dyn Error>> {
        let payload = self.create_payload(band.feature_id, value)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    /// Set EQ band using raw dB value, clamped to -12.0..=12.0
    pub fn set_eq_band_db(
        &self,
        band: EqBand,
        db_value: f32,
    ) -> Result<(), Box<dyn Error>> {
        let clamped = db_value.clamp(-12.0, 12.0);
        let payload = self.create_payload_raw(band.feature_id, clamped)?;
        self.send_payload(&payload)?;
        Ok(())
    }

    fn send_payload(&self, payload: &Payload) -> Result<(), Box<dyn Error>> {
        self.connection.write(&payload.data)?;
        self.connection.write(&payload.commit)?;
        Ok(())
    }

    pub fn create_payload(
        &self,
        feature_id: u8,
        value: u8,
    ) -> Result<Payload, Box<dyn Error>> {
        self.create_payload_raw(feature_id, value as f32 / 100.0)
    }

    /// Create payload with raw float value (no normalization)
    pub fn create_payload_raw(
        &self,
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
}

#[derive(Debug)]
pub struct Payload {
    pub data: Vec<u8>,
    pub commit: Vec<u8>,
}

/// # Feature IDs
/// Sliders are feature_id + 1
#[derive(Clone, Copy, Debug)]
pub enum SoundFeature {
    SurroundSound,
    Crystalizer,
    Bass,
    SmartVolume,
    DialogPlus,
    NightMode,
    Equalizer,
    EqBand(EqBand),
}

impl SoundFeature {
    /// # Feature IDs
    /// Sliders are feature_id + 1
    pub fn id(&self) -> u8 {
        match self {
            SoundFeature::SurroundSound => 0x00,
            SoundFeature::Crystalizer => 0x07,
            SoundFeature::Bass => 0x18,
            SoundFeature::SmartVolume => 0x04,
            SoundFeature::DialogPlus => 0x02,
            SoundFeature::NightMode => 0x06,
            SoundFeature::Equalizer => 0x09,
            SoundFeature::EqBand(band) => band.feature_id,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub value: u8,
    pub feature_id: u8,
}

pub struct Equalizer {
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
    pub fn bands(&self) -> [EqBand; 10] {
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
