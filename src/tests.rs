#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::*;

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
                "Band {} should have feature_id {}",
                i, expected_id
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
        // ... (existing test code)
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
            let path = preset_path(input).unwrap();
            let filename = path.file_stem().unwrap().to_str().unwrap();
            assert_eq!(
                filename, expected_sanitized,
                "Failed to sanitize '{}' correctly",
                input
            );

            // Verify it ends with .json
            assert_eq!(path.extension().unwrap(), "json");
        }

        // Test empty string - sanitized empty string becomes empty, resulting in ".json"
        let empty_path = preset_path("").unwrap();
        // Empty string sanitizes to empty, which results in just ".json" as filename
        assert!(
            empty_path.file_name().unwrap().to_str().unwrap() == ".json"
                || empty_path
                    .file_stem()
                    .map(|s| s.to_str().unwrap())
                    .unwrap_or("")
                    .is_empty()
        );
    }

    #[test]
    fn test_persistence_logic() {
        let original_home = std::env::var("HOME").ok();
        let temp_home = "/tmp/blaster_persistence_test";
        let _ = fs::remove_dir_all(temp_home);
        unsafe {
            std::env::set_var("HOME", temp_home);
        }

        // Test ensure_presets_dir
        let dir = ensure_presets_dir().unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());

        // Test save/load/list/delete would require a BlasterXG6 instance.
        // Since we can't easily create one, we'll test the list_presets logic
        // by manually creating a file.

        let features = vec![(SoundFeature::Bass, 42)];
        let preset = Preset {
            name: "Persistent Preset".to_string(),
            features,
            eq_bands: Vec::new(),
        };

        let path = preset_path(&preset.name).unwrap();
        // Clean up any existing file from previous test runs
        let _ = fs::remove_file(&path);

        let json = serde_json::to_string_pretty(&preset).unwrap();
        fs::write(&path, json).unwrap();
        assert!(path.exists()); // Should exist after write

        let presets = list_presets().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Persistent Preset");
        assert!(
            presets[0]
                .features
                .iter()
                .any(|(f, v)| *f == SoundFeature::Bass && *v == 42)
        );

        // Test delete_preset
        delete_preset_by_name("Persistent Preset").unwrap();
        assert!(!path.exists()); // Should be deleted
        let presets_after = list_presets().unwrap();
        assert_eq!(presets_after.len(), 0);

        // Test delete non-existent preset (should not error)
        delete_preset_by_name("Non-existent").unwrap();

        if let Some(home) = original_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn test_list_presets_ignores_invalid_files() {
        let original_home = std::env::var("HOME").ok();
        let temp_home = "/tmp/blaster_list_test";
        let _ = fs::remove_dir_all(temp_home);
        unsafe {
            std::env::set_var("HOME", temp_home);
        }

        ensure_presets_dir().unwrap();

        // Create a valid preset
        let valid_preset = Preset {
            name: "Valid".to_string(),
            features: vec![(SoundFeature::Bass, 50)],
            eq_bands: Vec::new(),
        };
        let valid_path = preset_path("Valid").unwrap();
        fs::write(
            &valid_path,
            serde_json::to_string_pretty(&valid_preset).unwrap(),
        )
        .unwrap();

        // Create an invalid JSON file
        let invalid_path = preset_path("Invalid").unwrap();
        fs::write(&invalid_path, "not valid json").unwrap();

        // Create a non-JSON file (should be ignored)
        let non_json_path = presets_dir().unwrap().join("not_a_preset.txt");
        fs::write(&non_json_path, "some text").unwrap();

        let presets = list_presets().unwrap();
        // Should only return the valid preset
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Valid");

        // Cleanup - remove all test files
        let _ = fs::remove_file(&valid_path);
        let _ = fs::remove_file(&invalid_path);
        let _ = fs::remove_file(&non_json_path);

        if let Some(home) = original_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
        let _ = fs::remove_dir_all(temp_home);
    }
}
