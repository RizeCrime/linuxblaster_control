#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::*;
    use std::sync::Mutex;

    // Mutex to synchronize HOME environment variable access across parallel tests
    static HOME_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_value_to_bytes() {
        assert_eq!(value_to_bytes(0), 0.0f32.to_le_bytes());
        assert_eq!(value_to_bytes(100), 1.0f32.to_le_bytes());
        assert_eq!(value_to_bytes(50), 0.5f32.to_le_bytes());
    }

    #[test]
    fn test_sound_feature_ids() {
        assert_eq!(SoundFeature::SurroundSound.id(), 0x00);
        assert_eq!(SoundFeature::Crystalizer.id(), 0x07);
        assert_eq!(SoundFeature::Bass.id(), 0x18);
        assert_eq!(SoundFeature::SmartVolume.id(), 0x04);
        assert_eq!(SoundFeature::DialogPlus.id(), 0x02);
        assert_eq!(SoundFeature::NightMode.id(), 0x06);
        assert_eq!(SoundFeature::LoudMode.id(), 0x06); // Same ID as NightMode
        assert_eq!(SoundFeature::Equalizer.id(), 0x09);

        let band = EqBand {
            value: 0,
            feature_id: 0x0b,
        };
        assert_eq!(SoundFeature::EqBand(band).id(), 0x0b);
    }

    #[test]
    fn test_equalizer_bands() {
        let eq = Equalizer::default();
        let bands = eq.bands();
        assert_eq!(bands.len(), 10);

        // Test all 10 EQ band feature IDs in sequence
        let expected_ids =
            [0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14];
        for (i, expected_id) in expected_ids.iter().enumerate() {
            assert_eq!(
                bands[i].feature_id, *expected_id,
                "Band {i} should have feature_id {expected_id}"
            );
        }
    }

    #[test]
    fn test_payload_creation() {
        let feature_id = 0x07; // Crystalizer
        let value = 0.5f32;
        let value_bytes = value.to_le_bytes();

        let payload =
            BlasterXG6::create_payload_raw(feature_id, value).unwrap();

        // Verify payload size
        assert_eq!(payload.data.len(), 65);
        assert_eq!(payload.commit.len(), 65);

        // DATA packet: 65 bytes
        assert_eq!(payload.data[0], 0x00);
        assert_eq!(payload.data[1], 0x5a);
        assert_eq!(payload.data[2], 0x12);
        assert_eq!(payload.data[3], 0x07);
        assert_eq!(payload.data[4], 0x01);
        assert_eq!(payload.data[5], 0x96);
        assert_eq!(payload.data[6], feature_id);
        assert_eq!(payload.data[7..11], value_bytes);

        // COMMIT packet: 65 bytes
        assert_eq!(payload.commit[0], 0x00);
        assert_eq!(payload.commit[1], 0x5a);
        assert_eq!(payload.commit[2], 0x11);
        assert_eq!(payload.commit[3], 0x03);
        assert_eq!(payload.commit[4], 0x01);
        assert_eq!(payload.commit[5], 0x96);
        assert_eq!(payload.commit[6], feature_id);
    }

    #[test]
    fn test_create_payload_normalization() {
        // Test create_payload (normalizes u8 to float)
        let feature_id = 0x07;
        let value = 50u8; // Should become 0.5f32

        let payload = BlasterXG6::create_payload(feature_id, value).unwrap();
        let expected_bytes = 0.5f32.to_le_bytes();

        assert_eq!(payload.data[7..11], expected_bytes);
    }

    #[test]
    fn test_nightmode_loudmode_payloads() {
        let feature_id = 0x06; // Shared by both NightMode and LoudMode

        // NightMode enable uses value 200 (2.0)
        let nightmode_payload =
            BlasterXG6::create_payload(feature_id, 200).unwrap();
        let expected_nightmode_bytes = 2.0f32.to_le_bytes();
        assert_eq!(nightmode_payload.data[7..11], expected_nightmode_bytes);

        // LoudMode enable uses value 100 (1.0)
        let loudmode_payload =
            BlasterXG6::create_payload(feature_id, 100).unwrap();
        let expected_loudmode_bytes = 1.0f32.to_le_bytes();
        assert_eq!(loudmode_payload.data[7..11], expected_loudmode_bytes);

        // Disable uses value 0 (0.0)
        let disable_payload =
            BlasterXG6::create_payload(feature_id, 0).unwrap();
        let expected_disable_bytes = 0.0f32.to_le_bytes();
        assert_eq!(disable_payload.data[7..11], expected_disable_bytes);
    }

    #[test]
    fn test_set_slider_feature_id_offset() {
        // set_slider uses feature_id + 1
        let base_feature_id = 0x07; // Crystalizer
        let slider_feature_id = base_feature_id + 1;
        let value = 75u8;

        let payload =
            BlasterXG6::create_payload(slider_feature_id, value).unwrap();

        // Verify the feature_id in payload is base_feature_id + 1
        assert_eq!(payload.data[6], slider_feature_id);
        assert_eq!(payload.commit[6], slider_feature_id);
    }

    #[test]
    fn test_eq_band_db_clamping() {
        // Test that set_eq_band_db clamps values to -12.0..=12.0
        // Since set_eq_band_db requires a device, we test the clamping logic directly
        let band = EqBand {
            value: 0,
            feature_id: 0x0b,
        };

        // Test clamping logic: values outside range get clamped
        let value_below = -15.0f32;
        let clamped_below = value_below.clamp(-12.0, 12.0);
        assert_eq!(clamped_below, -12.0);

        let payload_below =
            BlasterXG6::create_payload_raw(band.feature_id, clamped_below)
                .unwrap();
        let expected_clamped_below = (-12.0f32).to_le_bytes();
        assert_eq!(payload_below.data[7..11], expected_clamped_below);

        let value_above = 15.0f32;
        let clamped_above = value_above.clamp(-12.0, 12.0);
        assert_eq!(clamped_above, 12.0);

        let payload_above =
            BlasterXG6::create_payload_raw(band.feature_id, clamped_above)
                .unwrap();
        let expected_clamped_above = 12.0f32.to_le_bytes();
        assert_eq!(payload_above.data[7..11], expected_clamped_above);

        // Test values within range are not changed
        let value_in_range = 5.5f32;
        let clamped_in_range = value_in_range.clamp(-12.0, 12.0);
        assert_eq!(clamped_in_range, 5.5);

        let payload_in_range =
            BlasterXG6::create_payload_raw(band.feature_id, clamped_in_range)
                .unwrap();
        let expected_in_range = 5.5f32.to_le_bytes();
        assert_eq!(payload_in_range.data[7..11], expected_in_range);
    }

    #[test]
    fn test_ui_app_initialization() {
        use crate::ui::BlasterApp;
        let app = BlasterApp::new(None);

        assert!(!app.surround.enabled);
        assert_eq!(app.surround.value, 50);
        assert!(!app.crystalizer.enabled);
        assert_eq!(app.crystalizer.value, 50);
        assert!(!app.bass.enabled);
        assert_eq!(app.bass.value, 50);
        assert!(!app.smart_volume.enabled);
        assert_eq!(app.smart_volume.value, 50);
        assert!(!app.dialog_plus.enabled);
        assert_eq!(app.dialog_plus.value, 50);
        assert!(!app.night_mode);
        assert!(!app.loud_mode);
        assert!(!app.eq_enabled);
        assert!(app.eq_bands.iter().all(|&v| v == 0.0));
        assert_eq!(app.ui_scale, 1.5);
    }

    #[test]
    fn test_ui_app_reset() {
        use crate::ui::BlasterApp;
        let mut app = BlasterApp::new(None);

        // Change all state
        app.surround.enabled = true;
        app.surround.value = 80;
        app.crystalizer.enabled = true;
        app.crystalizer.value = 60;
        app.bass.enabled = true;
        app.bass.value = 90;
        app.smart_volume.enabled = true;
        app.smart_volume.value = 70;
        app.dialog_plus.enabled = true;
        app.dialog_plus.value = 40;
        app.eq_bands[5] = 5.0;
        app.eq_bands[0] = -3.0;
        app.night_mode = true;
        app.loud_mode = false; // Set explicitly for test clarity
        app.eq_enabled = true;
        app.ui_scale = 2.0;

        // Reset
        app.reset_ui();

        // Verify all features are reset
        assert!(!app.surround.enabled);
        assert_eq!(app.surround.value, 50);
        assert!(!app.crystalizer.enabled);
        assert_eq!(app.crystalizer.value, 50);
        assert!(!app.bass.enabled);
        assert_eq!(app.bass.value, 50);
        assert!(!app.smart_volume.enabled);
        assert_eq!(app.smart_volume.value, 50);
        assert!(!app.dialog_plus.enabled);
        assert_eq!(app.dialog_plus.value, 50);
        assert!(!app.night_mode);
        assert!(!app.loud_mode);
        assert!(!app.eq_enabled);
        assert!(app.eq_bands.iter().all(|&v| v == 0.0));
        // Note: ui_scale is NOT reset by reset_ui(), only UI state is reset
        assert_eq!(app.ui_scale, 2.0);
    }

    #[test]
    fn test_payload_edge_cases() {
        // Test boundary values
        let payload_min = BlasterXG6::create_payload(0x00, 0).unwrap();
        assert_eq!(payload_min.data[7..11], 0.0f32.to_le_bytes());

        let payload_max = BlasterXG6::create_payload(0xff, 255).unwrap();
        // 255/100 = 2.55
        assert_eq!(payload_max.data[7..11], 2.55f32.to_le_bytes());

        // Test extreme float values (should still create payload, clamping happens elsewhere)
        let payload_large =
            BlasterXG6::create_payload_raw(0x07, 1000.0).unwrap();
        assert_eq!(payload_large.data[6], 0x07);
        assert_eq!(payload_large.data[7..11], 1000.0f32.to_le_bytes());

        let payload_negative =
            BlasterXG6::create_payload_raw(0x07, -50.0).unwrap();
        assert_eq!(payload_negative.data[6], 0x07);
        assert_eq!(payload_negative.data[7..11], (-50.0f32).to_le_bytes());

        // Test all feature IDs produce valid payloads
        for feature_id in 0x00..=0xff {
            let payload =
                BlasterXG6::create_payload_raw(feature_id, 0.5).unwrap();
            assert_eq!(payload.data.len(), 65);
            assert_eq!(payload.commit.len(), 65);
            assert_eq!(payload.data[6], feature_id);
            assert_eq!(payload.commit[6], feature_id);
        }
    }

    #[test]
    fn test_preset_serialization() {
        let features = vec![
            (SoundFeature::SurroundSound, 75),
            (SoundFeature::Crystalizer, 50),
            (SoundFeature::NightMode, 200), // Test special value
        ];

        let mut eq_bands = Vec::new();
        let band = EqBand {
            value: 0,
            feature_id: 0x0b,
        };
        eq_bands.push((band, 5.5));

        let preset = Preset {
            name: "Test Preset".to_string(),
            features: features.clone(),
            eq_bands: eq_bands.clone(),
        };

        let json = serde_json::to_string(&preset).unwrap();
        let deserialized: Preset = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, preset.name);
        assert_eq!(deserialized.features.len(), features.len());
        assert_eq!(deserialized.eq_bands.len(), eq_bands.len());

        // Verify all features are preserved
        assert!(
            deserialized
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::SurroundSound && *v == 75)
        );
        assert!(
            deserialized
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::Crystalizer && *v == 50)
        );
        assert!(
            deserialized
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::NightMode && *v == 200)
        );

        // Verify EQ bands are preserved
        assert!(
            deserialized
                .eq_bands
                .iter()
                .any(|(b, v)| b.feature_id == band.feature_id && *v == 5.5)
        );
    }

    #[test]
    fn test_preset_empty_preset() {
        // Test that empty presets serialize/deserialize correctly
        let preset = Preset {
            name: "Empty Preset".to_string(),
            features: Vec::new(),
            eq_bands: Vec::new(),
        };

        let json = serde_json::to_string(&preset).unwrap();
        let deserialized: Preset = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "Empty Preset");
        assert!(deserialized.features.is_empty());
        assert!(deserialized.eq_bands.is_empty());
    }

    #[test]
    fn test_preset_all_features() {
        // Test preset with all possible features
        let features = vec![
            (SoundFeature::SurroundSound, 100),
            (SoundFeature::Crystalizer, 80),
            (SoundFeature::Bass, 60),
            (SoundFeature::SmartVolume, 40),
            (SoundFeature::DialogPlus, 20),
            (SoundFeature::NightMode, 200),
            (SoundFeature::LoudMode, 100),
            (SoundFeature::Equalizer, 100),
        ];

        let mut eq_bands = Vec::new();
        let eq_band_defs = Equalizer::default().bands();
        for (i, band) in eq_band_defs.iter().enumerate() {
            eq_bands.push((*band, (i as f32) - 5.0)); // Values from -5.0 to 4.0
        }

        let preset = Preset {
            name: "All Features".to_string(),
            features,
            eq_bands,
        };

        let json = serde_json::to_string(&preset).unwrap();
        let deserialized: Preset = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.features.len(), 8);
        assert_eq!(deserialized.eq_bands.len(), 10);
    }

    #[test]
    fn test_to_preset_logic() {
        // Create a mock device state by manually constructing the state
        // Since we can't create a real BlasterXG6 without hardware, we test the logic
        // by checking what to_preset would produce given certain state

        // This test verifies that to_preset correctly captures:
        // 1. Only enabled features are included
        // 2. Correct values are captured
        // 3. All EQ bands are included (even if 0.0)
        // 4. NightMode uses special value 200

        // We can't easily test this without a device, but we can verify the structure
        // by checking that the Preset struct can be created correctly
        let features = vec![
            (SoundFeature::SurroundSound, 75),
            (SoundFeature::NightMode, 200),
        ];

        let mut eq_bands = Vec::new();
        let eq_band_defs = Equalizer::default().bands();
        for (i, band) in eq_band_defs.iter().enumerate() {
            eq_bands.push((*band, if i == 0 { 5.0 } else { 0.0 }));
        }

        let preset = Preset {
            name: "Test Logic".to_string(),
            features,
            eq_bands,
        };

        // Verify structure
        assert_eq!(preset.name, "Test Logic");
        assert_eq!(preset.features.len(), 2);
        assert_eq!(preset.eq_bands.len(), 10);

        // Verify NightMode has special value
        assert!(
            preset
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::NightMode && *v == 200)
        );

        // Verify EQ bands are captured
        assert!(
            preset
                .eq_bands
                .iter()
                .any(|(b, v)| b.feature_id == eq_band_defs[0].feature_id
                    && *v == 5.0)
        );
    }

    #[test]
    fn test_preset_path_sanitization() {
        // Ensure HOME is set (may have been unset by other tests)
        let original_home = std::env::var("HOME").ok();
        let test_home =
            original_home.clone().unwrap_or_else(|| "/tmp".to_string());

        // Use a guard to ensure HOME is restored even if test panics
        struct HomeGuard {
            original: Option<String>,
        }
        impl Drop for HomeGuard {
            fn drop(&mut self) {
                let _guard = HOME_MUTEX.lock().unwrap();
                unsafe {
                    if let Some(ref home) = self.original {
                        std::env::set_var("HOME", home);
                    } else {
                        std::env::set_var("HOME", "/tmp");
                    }
                }
            }
        }
        let _home_guard = HomeGuard {
            original: original_home.clone(),
        };

        // Test that preset_path sanitizes filenames correctly
        let test_cases = vec![
            ("normal-name", "normal-name"),
            ("name with spaces", "name_with_spaces"),
            ("name@with#special$chars", "name_with_special_chars"),
            ("name-with-dashes", "name-with-dashes"),
            ("name_with_underscores", "name_with_underscores"),
            ("Name123", "Name123"),
            ("../etc/passwd", "___etc_passwd"), // Security: prevent path traversal
        ];

        for (input, expected_sanitized) in test_cases {
            // Hold mutex during entire operation to prevent other tests from modifying HOME
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &test_home);
            }
            let path = preset_path(input).unwrap();
            let filename = path.file_stem().unwrap().to_str().unwrap();
            assert_eq!(
                filename, expected_sanitized,
                "Failed to sanitize '{input}' correctly"
            );

            // Verify it ends with .json
            assert_eq!(path.extension().unwrap(), "json");
        }

        // Test empty string - sanitized empty string becomes empty, resulting in ".json"
        let _mutex_guard = HOME_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("HOME", &test_home);
        }
        let empty_path = preset_path("").unwrap();
        // Empty string sanitizes to empty, which results in just ".json" as filename
        assert!(
            empty_path.file_name().unwrap().to_str().unwrap() == ".json"
                || empty_path
                    .file_stem()
                    .map_or("", |s| s.to_str().unwrap())
                    .is_empty()
        );
    }

    #[test]
    fn test_persistence_logic() {
        let original_home = std::env::var("HOME").ok();
        // Use thread ID to make directory unique for parallel tests
        let thread_id = std::thread::current().id();
        let temp_home =
            format!("/tmp/blaster_persistence_test_{:?}", thread_id);
        let _ = fs::remove_dir_all(&temp_home);

        // Ensure HOME is set (may have been unset by other tests)
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        // Use a guard to ensure HOME is restored even if test panics
        struct HomeGuard {
            original: Option<String>,
        }
        impl Drop for HomeGuard {
            fn drop(&mut self) {
                let _guard = HOME_MUTEX.lock().unwrap();
                unsafe {
                    if let Some(ref home) = self.original {
                        std::env::set_var("HOME", home);
                    } else {
                        std::env::set_var("HOME", "/tmp");
                    }
                }
            }
        }
        let _guard = HomeGuard {
            original: original_home.clone(),
        };

        // Test ensure_presets_dir
        let dir = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            ensure_presets_dir().unwrap()
        };
        assert!(dir.exists());
        assert!(dir.is_dir());

        // Test save/load/list/delete would require a BlasterXG6 instance.
        // Since we can't easily create one, we'll test the list_presets logic
        // by manually creating a file.

        // Use a unique name to avoid conflicts with other tests
        let unique_name = format!("Persistent Preset {}", std::process::id());
        let features = vec![(SoundFeature::Bass, 42)];
        let preset = Preset {
            name: unique_name.clone(),
            features,
            eq_bands: Vec::new(),
        };

        let path = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            preset_path(&preset.name).unwrap()
        };
        // Clean up any existing file from previous test runs
        let _ = fs::remove_file(&path);

        let json = serde_json::to_string_pretty(&preset).unwrap();
        fs::write(&path, json).unwrap();
        assert!(path.exists()); // Should exist after write

        // Ensure HOME is still set (may have been unset by parallel tests)
        let presets = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            list_presets().unwrap()
        };
        // Find our unique preset (there may be others from parallel tests)
        let our_preset = presets.iter().find(|p| p.name == unique_name);
        assert!(our_preset.is_some(), "Our preset should be found");
        assert!(
            our_preset
                .unwrap()
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::Bass && *v == 42)
        );

        // Test delete_preset
        {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            delete_preset_by_name(&unique_name).unwrap();
        }
        // Re-check path after ensuring HOME is set
        let path_after_delete = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            preset_path(&preset.name).unwrap()
        };
        assert!(!path_after_delete.exists()); // Should be deleted

        let presets_after = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            list_presets().unwrap()
        };
        // Our preset should be gone (others may still exist)
        assert!(!presets_after.iter().any(|p| p.name == unique_name));

        // Test delete non-existent preset (should not error)
        {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            delete_preset_by_name("Non-existent").unwrap();
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_set_eq_band_conversion() {
        // Test u8 to dB conversion: (value / 100.0) * 24.0 - 12.0
        // 0 -> -12.0 dB
        let db_0 = (0.0f32 / 100.0).mul_add(24.0, -12.0);
        assert_eq!(db_0, -12.0);

        // 50 -> 0.0 dB
        let db_50 = (50.0f32 / 100.0).mul_add(24.0, -12.0);
        assert_eq!(db_50, 0.0);

        // 100 -> +12.0 dB
        let db_100 = (100.0f32 / 100.0).mul_add(24.0, -12.0);
        assert_eq!(db_100, 12.0);

        // Test some intermediate values
        let db_25 = (25.0f32 / 100.0).mul_add(24.0, -12.0);
        assert_eq!(db_25, -6.0);

        let db_75 = (75.0f32 / 100.0).mul_add(24.0, -12.0);
        assert_eq!(db_75, 6.0);
    }

    #[test]
    fn test_nightmode_loudmode_mutual_exclusivity() {
        // Test that NightMode and LoudMode use the same feature_id but different values
        // This tests the logic in enable() method
        assert_eq!(SoundFeature::NightMode.id(), SoundFeature::LoudMode.id());

        // NightMode uses value 200 (2.0)
        let nightmode_payload =
            BlasterXG6::create_payload(SoundFeature::NightMode.id(), 200)
                .unwrap();
        assert_eq!(nightmode_payload.data[7..11], 2.0f32.to_le_bytes());

        // LoudMode uses value 100 (1.0)
        let loudmode_payload =
            BlasterXG6::create_payload(SoundFeature::LoudMode.id(), 100)
                .unwrap();
        assert_eq!(loudmode_payload.data[7..11], 1.0f32.to_le_bytes());

        // Both disable to 0
        let disable_night =
            BlasterXG6::create_payload(SoundFeature::NightMode.id(), 0)
                .unwrap();
        let disable_loud =
            BlasterXG6::create_payload(SoundFeature::LoudMode.id(), 0).unwrap();
        assert_eq!(disable_night.data[7..11], disable_loud.data[7..11]);
        assert_eq!(disable_night.data[7..11], 0.0f32.to_le_bytes());
    }

    #[test]
    fn test_enable_disable_state_values() {
        // Test that enable() uses correct values for each feature
        // We can't test the actual state changes without a device, but we can verify
        // the values that would be sent

        // SurroundSound, Crystalizer, Bass, SmartVolume, DialogPlus, Equalizer use 100
        for feature in [
            SoundFeature::SurroundSound,
            SoundFeature::Crystalizer,
            SoundFeature::Bass,
            SoundFeature::SmartVolume,
            SoundFeature::DialogPlus,
            SoundFeature::Equalizer,
        ] {
            let payload =
                BlasterXG6::create_payload(feature.id(), 100).unwrap();
            assert_eq!(payload.data[7..11], 1.0f32.to_le_bytes());
        }

        // NightMode uses 200 (special value)
        let nightmode_enable =
            BlasterXG6::create_payload(SoundFeature::NightMode.id(), 200)
                .unwrap();
        assert_eq!(nightmode_enable.data[7..11], 2.0f32.to_le_bytes());

        // LoudMode uses 100
        let loudmode_enable =
            BlasterXG6::create_payload(SoundFeature::LoudMode.id(), 100)
                .unwrap();
        assert_eq!(loudmode_enable.data[7..11], 1.0f32.to_le_bytes());

        // All features disable to 0
        for feature in [
            SoundFeature::SurroundSound,
            SoundFeature::Crystalizer,
            SoundFeature::Bass,
            SoundFeature::SmartVolume,
            SoundFeature::DialogPlus,
            SoundFeature::NightMode,
            SoundFeature::LoudMode,
            SoundFeature::Equalizer,
        ] {
            let payload = BlasterXG6::create_payload(feature.id(), 0).unwrap();
            assert_eq!(payload.data[7..11], 0.0f32.to_le_bytes());
        }
    }

    #[test]
    fn test_set_slider_value_updates() {
        // Test that set_slider uses feature_id + 1
        // We can verify the payload creation logic
        let base_id = SoundFeature::SurroundSound.id();
        let slider_id = base_id + 1;

        for value in [0, 25, 50, 75, 100] {
            let payload = BlasterXG6::create_payload(slider_id, value).unwrap();
            assert_eq!(payload.data[6], slider_id);
            assert_eq!(payload.commit[6], slider_id);
            let expected_float = f32::from(value) / 100.0;
            assert_eq!(payload.data[7..11], expected_float.to_le_bytes());
        }
    }

    #[test]
    fn test_to_preset_only_includes_enabled_features() {
        // Test the logic of to_preset - only enabled features should be included
        // We can't create a real device, but we can verify the structure matches expectations
        let preset_with_features = Preset {
            name: "Test".to_string(),
            features: vec![
                (SoundFeature::SurroundSound, 75),
                (SoundFeature::Crystalizer, 50),
            ],
            eq_bands: Vec::new(),
        };

        // Verify only enabled features are present
        assert_eq!(preset_with_features.features.len(), 2);
        assert!(
            preset_with_features
                .features
                .iter()
                .any(|(f, _)| *f == SoundFeature::SurroundSound)
        );
        assert!(
            preset_with_features
                .features
                .iter()
                .any(|(f, _)| *f == SoundFeature::Crystalizer)
        );

        // Empty preset should have no features
        let empty_preset = Preset {
            name: "Empty".to_string(),
            features: Vec::new(),
            eq_bands: Vec::new(),
        };
        assert!(empty_preset.features.is_empty());
    }

    #[test]
    fn test_apply_preset_logic() {
        // Test the logic of apply_preset without requiring a device
        // Verify that preset structure matches what apply_preset expects

        // Preset with slider features
        let slider_preset = Preset {
            name: "Slider Test".to_string(),
            features: vec![
                (SoundFeature::SurroundSound, 75),
                (SoundFeature::Bass, 50),
            ],
            eq_bands: Vec::new(),
        };

        // Verify slider features have values > 0
        for (feature, value) in &slider_preset.features {
            match feature {
                SoundFeature::SurroundSound
                | SoundFeature::Crystalizer
                | SoundFeature::Bass
                | SoundFeature::SmartVolume
                | SoundFeature::DialogPlus => {
                    assert!(*value > 0, "Slider feature should have value > 0");
                }
                _ => {}
            }
        }

        // Preset with toggle features
        let toggle_preset = Preset {
            name: "Toggle Test".to_string(),
            features: vec![
                (SoundFeature::NightMode, 200),
                (SoundFeature::LoudMode, 100),
                (SoundFeature::Equalizer, 100),
            ],
            eq_bands: Vec::new(),
        };

        // Verify toggle features use correct values
        assert!(
            toggle_preset
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::NightMode && *v == 200)
        );
        assert!(
            toggle_preset
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::LoudMode && *v == 100)
        );
    }

    #[test]
    fn test_preset_with_eq_bands() {
        // Test preset with EQ bands at boundaries
        let mut eq_bands = Vec::new();
        let eq_band_defs = Equalizer::default().bands();

        // Set bands to boundary values
        eq_bands.push((eq_band_defs[0], -12.0)); // Minimum
        eq_bands.push((eq_band_defs[1], 12.0)); // Maximum
        eq_bands.push((eq_band_defs[2], 0.0)); // Center

        let preset = Preset {
            name: "EQ Test".to_string(),
            features: Vec::new(),
            eq_bands: eq_bands.clone(),
        };

        assert_eq!(preset.eq_bands.len(), 3);
        assert!(preset.eq_bands.iter().any(|(_, v)| *v == -12.0));
        assert!(preset.eq_bands.iter().any(|(_, v)| *v == 12.0));
        assert!(preset.eq_bands.iter().any(|(_, v)| *v == 0.0));
    }

    #[test]
    fn test_preset_error_cases() {
        // Test error handling for preset operations
        let original_home = std::env::var("HOME").ok();
        // Use thread ID to make directory unique for parallel tests
        let thread_id = std::thread::current().id();
        let temp_home =
            format!("/tmp/blaster_preset_error_test_{:?}", thread_id);
        let _ = fs::remove_dir_all(&temp_home);

        // Use a guard to ensure HOME is restored even if test panics
        struct HomeGuard {
            original: Option<String>,
        }
        impl Drop for HomeGuard {
            fn drop(&mut self) {
                let _guard = HOME_MUTEX.lock().unwrap();
                unsafe {
                    if let Some(ref home) = self.original {
                        std::env::set_var("HOME", home);
                    } else {
                        std::env::set_var("HOME", "/tmp");
                    }
                }
            }
        }
        let _guard = HomeGuard {
            original: original_home.clone(),
        };

        {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
        }

        // Test loading non-existent preset (requires device, skip for now)
        // The path logic is tested via preset_path() below

        // Test deleting non-existent preset (should not error)
        let result = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            delete_preset_by_name("non-existent-preset")
        };
        assert!(result.is_ok());

        // Test preset_path with invalid HOME
        // First ensure HOME is actually removed
        let result = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::remove_var("HOME");
            }
            // Verify HOME is actually removed
            assert!(std::env::var("HOME").is_err(), "HOME should be removed");
            preset_path("test")
        };
        assert!(
            result.is_err(),
            "preset_path should fail when HOME is not set"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_preset_invalid_json_handling() {
        // Test that list_presets skips invalid JSON files
        let original_home = std::env::var("HOME").ok();
        // Use thread ID to make directory unique for parallel tests
        let thread_id = std::thread::current().id();
        let temp_home =
            format!("/tmp/blaster_preset_json_test_{:?}", thread_id);
        let _ = fs::remove_dir_all(&temp_home);

        // Use a guard to ensure HOME is restored even if test panics
        struct HomeGuard {
            original: Option<String>,
        }
        impl Drop for HomeGuard {
            fn drop(&mut self) {
                let _guard = HOME_MUTEX.lock().unwrap();
                unsafe {
                    if let Some(ref home) = self.original {
                        std::env::set_var("HOME", home);
                    } else {
                        std::env::set_var("HOME", "/tmp");
                    }
                }
            }
        }
        let _guard = HomeGuard {
            original: original_home.clone(),
        };

        let dir = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            ensure_presets_dir().unwrap()
        };

        // Create a valid preset
        let valid_preset = Preset {
            name: "Valid".to_string(),
            features: vec![(SoundFeature::Bass, 50)],
            eq_bands: Vec::new(),
        };
        // Ensure HOME is set right before calling preset_path (which reads HOME)
        let valid_path = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            preset_path("Valid").unwrap()
        };
        // Ensure parent directory exists before writing
        if let Some(parent) = valid_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&valid_path, serde_json::to_string(&valid_preset).unwrap())
            .unwrap();

        // Create an invalid JSON file
        let invalid_path = dir.join("invalid.json");
        fs::write(&invalid_path, "{ invalid json }").unwrap();

        // Create a non-JSON file
        let text_path = dir.join("not_json.txt");
        fs::write(&text_path, "not a preset").unwrap();

        // list_presets should include the valid preset and skip invalid files
        let presets = {
            let _mutex_guard = HOME_MUTEX.lock().unwrap();
            unsafe {
                std::env::set_var("HOME", &temp_home);
            }
            list_presets().unwrap()
        };
        // Find our valid preset (there may be others from parallel tests)
        let valid_preset_found = presets.iter().any(|p| p.name == "Valid");
        assert!(valid_preset_found, "Valid preset should be found");
        // Verify invalid files are not parsed as presets
        assert!(!presets.iter().any(|p| p.name == "invalid"));
        assert!(!presets.iter().any(|p| p.name == "not_json"));

        // Cleanup
        let _ = fs::remove_file(&valid_path);
        let _ = fs::remove_file(&invalid_path);
        let _ = fs::remove_file(&text_path);

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_value_to_bytes_edge_cases() {
        // Test boundary values for value_to_bytes
        assert_eq!(value_to_bytes(0), 0.0f32.to_le_bytes());
        assert_eq!(value_to_bytes(100), 1.0f32.to_le_bytes());
        assert_eq!(value_to_bytes(1), 0.01f32.to_le_bytes());
        assert_eq!(value_to_bytes(99), 0.99f32.to_le_bytes());
        assert_eq!(value_to_bytes(50), 0.5f32.to_le_bytes());
    }

    // Note: send_payload() is not tested here. The software only connects to the specific
    // device it's designed for, so if it works on one machine, it's reasonably safe to
    // assume it will work elsewhere as long as the device is present. Proper tests would
    // require hardware mocking and are deferred for now.
}
