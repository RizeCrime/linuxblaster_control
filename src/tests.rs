#[cfg(test)]
mod tests {
    use crate::features::{self, FeatureId, MIC_EQ_PRESETS, ValueKind};

    /// Builds a 64-byte packet from a hex string like "5a 11 08 ...".
    fn packet(hex: &str) -> [u8; 64] {
        let mut packet = [0u8; 64];
        for (index, byte) in hex.split_whitespace().enumerate() {
            packet[index] = u8::from_str_radix(byte, 16).unwrap();
        }
        packet
    }

    #[test]
    fn all_feature_ids_are_registered() {
        let features = features::all_features();
        for &id in FeatureId::ALL {
            assert!(
                features.iter().any(|feature| feature.id == id),
                "FeatureId::{:?} is in ALL but not in all_features()",
                id
            );
        }
    }

    #[test]
    fn no_duplicate_features() {
        let features = features::all_features();
        for (index, feature) in features.iter().enumerate() {
            for other in features.iter().skip(index + 1) {
                assert_ne!(
                    feature.id, other.id,
                    "Duplicate feature registration: {:?}",
                    feature.id
                );
            }
        }
    }

    #[test]
    fn dependencies_reference_valid_feature_ids() {
        for &id in FeatureId::ALL {
            for &dependency in id.dependencies() {
                assert!(
                    FeatureId::ALL.contains(&dependency),
                    "{:?} depends on {:?} which is not in ALL",
                    id,
                    dependency
                );
            }
        }
    }

    #[test]
    fn dependents_reference_valid_feature_ids() {
        for &id in FeatureId::ALL {
            for &dependent in id.dependents() {
                assert!(
                    FeatureId::ALL.contains(&dependent),
                    "{:?} has dependent {:?} which is not in ALL",
                    id,
                    dependent
                );
            }
        }
    }

    #[test]
    fn dependencies_and_dependents_are_consistent() {
        for &id in FeatureId::ALL {
            for &dependency in id.dependencies() {
                assert!(
                    dependency.dependents().contains(&id),
                    "{:?} lists {:?} as a dependency, but {:?} does not list {:?} as a dependent",
                    id,
                    dependency,
                    dependency,
                    id
                );
            }
        }
    }

    #[test]
    fn no_feature_depends_on_itself() {
        for &id in FeatureId::ALL {
            assert!(
                !id.dependencies().contains(&id),
                "{:?} depends on itself",
                id
            );
            assert!(
                !id.dependents().contains(&id),
                "{:?} lists itself as a dependent",
                id
            );
        }
    }

    #[test]
    fn paired_sliders_are_valid_feature_ids() {
        for &id in FeatureId::ALL {
            let Some(slider_id) = id.paired_slider() else { continue };
            assert!(
                FeatureId::ALL.contains(&slider_id),
                "{:?} has paired slider {:?} which is not in ALL",
                id,
                slider_id
            );
        }
    }

    #[test]
    fn dsp_features_have_dsp_addresses() {
        let features = features::all_features();
        for feature in &features {
            if feature.id.dsp_address().is_some() {
                continue;
            }
            assert!(
                matches!(
                    feature.id,
                    FeatureId::SbxMaster
                        | FeatureId::ScoutMode
                        | FeatureId::DirectMode
                        | FeatureId::Output
                        | FeatureId::DacFilter
                        | FeatureId::MicBoost
                        | FeatureId::LightingToggle
                        | FeatureId::LightingColor
                ) || feature.id.alsa_element().is_some(),
                "{:?} has no DSP address and is not a known non-DSP feature",
                feature.id
            );
        }
    }

    #[test]
    fn eq_bands_constant_has_ten_entries() {
        assert_eq!(FeatureId::EQ_BANDS.len(), 10);
    }

    #[test]
    fn eq_all_constant_has_eleven_entries() {
        assert_eq!(FeatureId::EQ_ALL.len(), 11);
    }

    #[test]
    fn status_entries_parses_single_feature_response() {
        // set_crystalizer_on: CrystalizerToggle = 1.0
        let response = packet("5a 11 08 01 00 96 07 00 00 80 3f");
        let entries: Vec<_> = features::status_entries(&response).collect();
        assert_eq!(entries, vec![(0x96, 0x07, 1.0)]);
    }

    #[test]
    fn status_entries_parses_multi_feature_push() {
        // set_output_speakers: profile dump block with 8 features
        let response = packet(
            "5a 11 32 08 00 96 01 00 00 00 3f 96 03 00 00 00 3f \
             96 05 a4 70 3d 3f 96 06 00 00 00 00 96 07 00 00 80 3f \
             96 08 00 00 00 3f 96 09 00 00 80 3f 96 0a 00 00 00 00",
        );
        let entries: Vec<_> = features::status_entries(&response).collect();
        assert_eq!(entries.len(), 8);
        assert_eq!(entries[0], (0x96, 0x01, 0.5));
        assert_eq!(entries[2], (0x96, 0x05, 0.74));
        assert_eq!(entries[7], (0x96, 0x0a, 0.0));
    }

    #[test]
    fn status_entries_ignores_other_commands() {
        let response = packet("5a 26 0b 08 ff ff 05 00 00 00 00 00 00 00");
        assert_eq!(features::status_entries(&response).count(), 0);
    }

    #[test]
    fn dsp_addresses_are_unique_and_reversible() {
        for &id in FeatureId::ALL {
            let Some((family, feature_id)) = id.dsp_address() else {
                continue;
            };
            assert_eq!(
                FeatureId::from_dsp_address(family, feature_id),
                Some(id),
                "{:?} does not round-trip through its DSP address",
                id
            );
        }
    }

    #[test]
    fn encode_and_decode_round_trip() {
        for &id in FeatureId::ALL {
            let values: &[f32] = match id.value_kind() {
                ValueKind::Preset(labels) => {
                    &[0.0, 1.0, 2.0][..labels.len().min(3)]
                }
                _ => &[0.0, 0.5, 1.0],
            };
            for &value in values {
                assert_eq!(
                    id.decode(id.encode(value)),
                    value,
                    "{:?} does not round-trip {}",
                    id,
                    value
                );
            }
        }
    }

    #[test]
    fn decoder_uses_device_values() {
        // Normal = 2.0, Full = 1.0, Night = 3.0 on the wire
        assert_eq!(FeatureId::Decoder.encode(0.0), 2.0);
        assert_eq!(FeatureId::Decoder.encode(1.0), 1.0);
        assert_eq!(FeatureId::Decoder.encode(2.0), 3.0);
    }

    #[test]
    fn noise_reduction_uses_half_scale_on_device() {
        // set_mic_noise_reduction_level-01: 0.11 on the wire = 22 %
        let raw = f32::from_le_bytes([0xae, 0x47, 0xe1, 0x3d]);
        let value = FeatureId::VoiceClarityNoiseReduction.decode(raw);
        assert!((value - 0.22).abs() < 1e-6);
    }

    #[test]
    fn mic_eq_presets_cover_all_bands_within_range() {
        for (name, bands) in MIC_EQ_PRESETS {
            assert_eq!(bands.len(), FeatureId::MIC_EQ_BANDS.len(), "{name}");
            assert!(
                bands.iter().all(|gain| (-12.0..=12.0).contains(gain)),
                "{name} has a band outside of ±12 dB"
            );
        }
    }

    #[test]
    fn reset_never_touches_mixer_or_output() {
        for &id in FeatureId::ALL {
            if id.alsa_element().is_some() || id == FeatureId::Output {
                assert_eq!(id.reset_value(), None, "{:?}", id);
            }
        }
    }

    #[test]
    fn direct_mode_state_decodes_the_stereo_bit() {
        assert_eq!(features::direct_mode_decode(0x00), 0.0);
        assert_eq!(features::direct_mode_decode(0x20), 1.0);
        // 0x40 is the constant S/PDIF flag in the same response.
        assert_eq!(features::direct_mode_decode(0x60), 1.0);
    }

    #[test]
    fn sbx_switch_is_the_inverse_of_direct_mode() {
        assert!(features::direct_mode_for_sbx(0.0));
        assert!(!features::direct_mode_for_sbx(1.0));
    }

    #[test]
    fn direct_mode_is_derived_and_not_profiled() {
        assert!(!FeatureId::DirectMode.in_profile());
        assert_eq!(FeatureId::DirectMode.reset_value(), None);
    }

    #[test]
    fn profile_round_trip_keeps_unread_values() {
        let features = features::all_features();
        let json = serde_json::to_string(&features).unwrap();
        let restored: Vec<features::Feature> =
            serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), features.len());
        assert!(restored.iter().all(|feature| feature.value().is_nan()));
    }

    #[test]
    fn profile_changes_only_include_differences_and_their_toggles() {
        let current = features::all_features();
        let set = |id: FeatureId, value: f32| {
            current
                .iter()
                .find(|feature| feature.id == id)
                .unwrap()
                .set_cached(value)
        };
        set(FeatureId::LightingToggle, 0.0);
        set(FeatureId::LightingColor, 0x00ff00 as f32);
        set(FeatureId::CrystalizerToggle, 1.0);
        set(FeatureId::CrystalizerLevel, 50.0);
        // EqToggle stays NaN: couldn't be read

        let saved = [
            (FeatureId::LightingToggle, 0.0),
            (FeatureId::LightingColor, 0xff0000 as f32),
            (FeatureId::CrystalizerToggle, 1.0),
            (FeatureId::CrystalizerLevel, 50.0),
            (FeatureId::EqToggle, 0.0),
        ];

        assert_eq!(
            crate::profile_changes(&current, &saved),
            vec![
                // unchanged, but the color write may switch it on
                (FeatureId::LightingToggle, 0.0),
                (FeatureId::LightingColor, 0xff0000 as f32),
                (FeatureId::EqToggle, 0.0),
            ]
        );

        set(FeatureId::LightingColor, 0xff0000 as f32);
        set(FeatureId::EqToggle, 0.0);
        assert!(crate::profile_changes(&current, &saved).is_empty());
    }

    #[test]
    fn nearly_equal_ignores_slider_rounding_and_handles_nan() {
        assert!(crate::nearly_equal(0.5, 0.5));
        assert!(crate::nearly_equal(0.5, 0.500_05));
        assert!(!crate::nearly_equal(0.5, 0.51));
        assert!(crate::nearly_equal(f32::NAN, f32::NAN));
        assert!(!crate::nearly_equal(f32::NAN, 0.0));
        assert!(!crate::nearly_equal(0.0, f32::NAN));
    }

    #[test]
    fn last_state_serializes_every_feature_but_not_the_write_counter() {
        let state = crate::BlasterXG6 {
            features: features::all_features(),
            writes: std::sync::atomic::AtomicU64::new(7),
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("writes"));
        assert!(json.contains("\"DirectMode\""));
        assert!(json.contains("\"SbxMaster\""));

        let restored: crate::BlasterXG6 = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.features.len(), state.features.len());
        assert_eq!(restored.write_count(), 0);
    }
}
