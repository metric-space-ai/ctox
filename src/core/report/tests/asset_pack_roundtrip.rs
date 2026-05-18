//! Asset-pack structural and consistency tests.

use crate::report::asset_pack::AssetPack;

#[test]
fn asset_pack_loads_and_validates() {
    let pack = AssetPack::load().expect("asset_pack loads");
    pack.validate().expect("asset_pack validates");
}

#[test]
fn asset_pack_has_seven_report_types() {
    let pack = AssetPack::load().expect("asset_pack loads");
    let ids: Vec<&str> = pack.report_types.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        7,
        "expected 7 report_types, got {}: {:?}",
        ids.len(),
        ids
    );
    let expected = [
        "feasibility_study",
        "market_research",
        "competitive_analysis",
        "technology_screening",
        "whitepaper",
        "literature_review",
        "decision_brief",
    ];
    for needle in expected {
        assert!(
            ids.contains(&needle),
            "missing expected report_type id {:?}; got {:?}",
            needle,
            ids
        );
    }
}

#[test]
fn asset_pack_block_library_keys_resolve() {
    let pack = AssetPack::load().expect("asset_pack loads");
    for report_type in &pack.report_types {
        for key in &report_type.block_library_keys {
            assert!(
                pack.block_library.contains_key(key),
                "report_type {} references unknown block_library key {:?}",
                report_type.id,
                key
            );
        }
    }
}

#[test]
fn asset_pack_reference_archetype_resources_resolve() {
    let pack = AssetPack::load().expect("asset_pack loads");
    let resource_ids: std::collections::HashSet<&str> = pack
        .reference_resources
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let rascon = pack
        .reference_archetypes
        .iter()
        .find(|a| a.id == "rascon_archetype")
        .expect("rascon_archetype reference_archetype is present");
    assert!(
        !rascon.uses_resource_ids.is_empty(),
        "rascon_archetype must reference at least one resource"
    );
    for resource_id in &rascon.uses_resource_ids {
        assert!(
            resource_ids.contains(resource_id.as_str()),
            "rascon_archetype references unknown resource_id {:?}",
            resource_id
        );
    }
}

#[test]
fn asset_pack_verdict_patterns_consistent() {
    let pack = AssetPack::load().expect("asset_pack loads");
    let report_type_ids: std::collections::HashSet<&str> =
        pack.report_types.iter().map(|r| r.id.as_str()).collect();
    for vp in &pack.verdict_patterns {
        assert!(
            report_type_ids.contains(vp.report_type_id.as_str()),
            "verdict_pattern references unknown report_type_id {:?}",
            vp.report_type_id
        );
    }
    // Every report_type with a non-null verdict_line_pattern must have at
    // least one verdict_pattern entry pointing back at it.
    for report_type in &pack.report_types {
        if report_type.verdict_line_pattern.is_some() {
            let count = pack
                .verdict_patterns
                .iter()
                .filter(|v| v.report_type_id == report_type.id)
                .count();
            assert!(
                count >= 1,
                "report_type {} declares verdict_line_pattern but has no verdict_patterns",
                report_type.id
            );
        }
    }
}

#[test]
fn asset_pack_style_profiles_referenced() {
    let pack = AssetPack::load().expect("asset_pack loads");
    let style_ids: std::collections::HashSet<&str> =
        pack.style_profiles.iter().map(|p| p.id.as_str()).collect();
    for profile in &pack.reference_profiles {
        if profile.style_profile_id.is_empty() {
            continue;
        }
        assert!(
            style_ids.contains(profile.style_profile_id.as_str()),
            "reference_profile {} references unknown style_profile_id {:?}",
            profile.id,
            profile.style_profile_id
        );
    }
}

#[test]
fn asset_pack_block_library_min_chars_positive() {
    let pack = AssetPack::load().expect("asset_pack loads");
    for (block_id, _value) in &pack.block_library {
        let entry = pack
            .block_library_entry(block_id)
            .expect("decode block_library entry");
        assert!(
            entry.min_chars > 0,
            "block_library entry {block_id} has non-positive min_chars ({})",
            entry.min_chars
        );
    }
}

#[test]
fn asset_pack_load_is_cached() {
    let a = AssetPack::load().expect("asset_pack first load");
    let b = AssetPack::load().expect("asset_pack second load");
    let pa: *const AssetPack = a;
    let pb: *const AssetPack = b;
    assert_eq!(
        pa, pb,
        "AssetPack::load() should return the same cached &'static reference"
    );
}
