#[cfg(test)]
mod tests {
    use crate::features::{self, FeatureId};

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
            if let Some(slider_id) = id.paired_slider() {
                assert!(
                    FeatureId::ALL.contains(&slider_id),
                    "{:?} has paired slider {:?} which is not in ALL",
                    id,
                    slider_id
                );
            }
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
                        | FeatureId::Output
                ),
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
}
