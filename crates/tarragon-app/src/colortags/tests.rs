use std::collections::BTreeSet;

use tarragon_core::color::ColorBucket;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};
use tarragon_render::color::{ColorTagConfig, ColorTags};

use tarragon_ui::swatch::Swatch;

use super::{read_config, swatch_name, tag_names};
use crate::filtering::tests::seeded;

#[test]
fn read_config_is_none_when_colour_tagging_is_off() {
    let (db, _dir) = seeded();
    let settings = Settings::new(&db);
    settings
        .set(SettingKey::ColorTagEnabled, SettingValue::Bool(false))
        .expect("set");
    assert_eq!(read_config(&db), None);
    settings
        .set(SettingKey::ColorTagEnabled, SettingValue::Bool(true))
        .expect("set");
    assert!(read_config(&db).is_some());
}

#[test]
fn read_config_uses_the_stored_values() {
    let (db, _dir) = seeded();
    let settings = Settings::new(&db);
    for (key, value) in [
        (SettingKey::ColorTagPaletteSize, SettingValue::Int(12)),
        (SettingKey::ColorTagMinShare, SettingValue::Float(0.2)),
        (
            SettingKey::ColorTagNeutralSThreshold,
            SettingValue::Float(0.3),
        ),
    ] {
        settings.set(key, value).expect("set");
    }
    let config = read_config(&db).expect("enabled");
    assert_eq!(config.palette_size, 12);
    assert!((config.min_share - 0.2).abs() < 1e-9);
    assert!((config.neutral_s_threshold - 0.3).abs() < 1e-9);
}

#[test]
fn read_config_gives_the_defaults_without_stored_values() {
    let (db, _dir) = seeded();
    assert_eq!(read_config(&db), Some(ColorTagConfig::default()));
}

#[test]
fn tag_names_follow_the_bucket_order_then_transparent() {
    let tags = ColorTags {
        buckets: BTreeSet::from([ColorBucket::Blue, ColorBucket::Red]),
        transparent: true,
    };
    assert_eq!(tag_names(&tags), ["red", "blue", "transparent"]);
}

#[test]
fn swatch_names_are_the_bucket_names_then_transparent() {
    let names: Vec<&str> = Swatch::ALL.into_iter().map(swatch_name).collect();
    assert_eq!(names.len(), 12);
    assert_eq!(names[0], "red");
    assert_eq!(names[10], "neutral");
    assert_eq!(names[11], "transparent");
}

#[test]
fn each_bucket_swatch_classifies_to_its_own_bucket() {
    use image::{DynamicImage, Rgba, RgbaImage};
    use tarragon_render::color::extract_dominant_colors;
    use tarragon_ui::swatch::bucket_color;

    // The swatch tokens are the design colours; the classifier ranges are
    // fitted to them, so each one must land in its own bucket.
    for bucket in ColorBucket::ALL {
        let c = bucket_color(bucket);
        let solid = RgbaImage::from_pixel(100, 100, Rgba([c.r(), c.g(), c.b(), 255]));
        let tags =
            extract_dominant_colors(&DynamicImage::ImageRgba8(solid), ColorTagConfig::default());
        assert_eq!(
            tags.buckets,
            BTreeSet::from([bucket]),
            "the {} swatch {:?} left its bucket",
            bucket.as_str(),
            (c.r(), c.g(), c.b()),
        );
    }
}

#[test]
fn tag_names_leave_out_transparent_when_it_is_not_set() {
    let red = ColorTags {
        buckets: BTreeSet::from([ColorBucket::Red]),
        transparent: false,
    };
    assert_eq!(tag_names(&red), ["red"]);
    assert_eq!(tag_names(&ColorTags::default()), Vec::<String>::new());
}

#[test]
fn tag_names_of_an_all_clear_image_is_transparent_only() {
    let tags = ColorTags {
        buckets: BTreeSet::new(),
        transparent: true,
    };
    assert_eq!(tag_names(&tags), ["transparent"]);
}
