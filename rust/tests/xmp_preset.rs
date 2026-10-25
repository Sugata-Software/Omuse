//! Independent synthetic XML; no Adobe/proprietary preset fixtures copied.
use omuse::{camera_raw::Settings, xmp_preset::*};
fn target(kind: SourceKind) -> TargetIdentity {
    TargetIdentity {
        kind,
        layer_id: "layer-1".into(),
        source_id: "source-1".into(),
        editor_instance: 7,
        revision: 11,
        working_space: "DisplayP3".into(),
        processing: "ByteCompatible".into(),
        tone_mapping: "Legacy".into(),
        camera_make: Some("Synthetic camera maker".into()),
        camera_model: Some("Synthetic camera model".into()),
        camera_profile: Some("existing-target-profile".into()),
    }
}
fn packet(attributes: &str, children: &str) -> Vec<u8> {
    format!(r#"<?xpacket begin=""?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:c="http://ns.adobe.com/camera-raw-settings/1.0/" {attributes}>{children}</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#).into_bytes()
}
fn row<'a>(report: &'a MappingReport, key: &str) -> &'a MappingRow {
    report.rows.iter().find(|row| row.property == key).unwrap()
}
#[test]
fn modern_values_preserve_omitted_settings_and_exact_original_xml_identity() {
    let input = packet(
        r#"c:ProcessVersion="11.0" c:Exposure2012="+1.25" c:Contrast2012="-40" c:Highlights2012="-85" c:Shadows2012="70" c:Whites2012="30" c:Blacks2012="-25" c:Clarity2012="15" c:Vibrance="22" c:Saturation="-10""#,
        "",
    );
    let original = input.clone();
    let mut identity = target(SourceKind::Rendered);
    identity.tone_mapping = "SmoothV1".into();
    let plan = parse(&input, identity.clone()).unwrap();
    assert_eq!(plan.mapped_count(), 9);
    let mut base = Settings::for_new_edit();
    base.temperature = 17.;
    base.tint = -8.;
    base.dehaze = 33.;
    let before = serde_json::to_value(&base).unwrap();
    let settings = plan.reviewed_settings(&base, &identity).unwrap();
    assert_eq!(settings.exposure, 1.25);
    assert_eq!(settings.contrast, -40.);
    assert_eq!(settings.highlights, -85.);
    assert_eq!(settings.shadows, 70.);
    assert_eq!(settings.whites, 30.);
    assert_eq!(settings.blacks, -25.);
    assert_eq!(settings.clarity, 15.);
    assert_eq!(settings.vibrance, 22.);
    assert_eq!(settings.saturation, -10.);
    assert_eq!(settings.temperature, 17.);
    assert_eq!(settings.tint, -8.);
    assert_eq!(settings.dehaze, 33.);
    let after = serde_json::to_value(&settings).unwrap();
    assert_eq!(after.get("processing"), before.get("processing"));
    assert_eq!(after.get("toneMapping"), before.get("toneMapping"));
    assert_eq!(
        row(plan.report(), "ProcessVersion").disposition,
        Disposition::RetainedIdentity
    );
    assert!(
        plan.report()
            .rows
            .iter()
            .filter(|row| row.target.is_some())
            .all(|row| row.disposition == Disposition::Approximate)
    );
    assert_eq!(
        plan.report().source_sha256,
        omuse::asset_library::sha256_hex(&original)
    );
    assert_eq!(input, original, "sidecar never rewritten");
    let serialized = serde_json::to_vec(plan.report()).unwrap();
    assert_eq!(
        serde_json::from_slice::<MappingReport>(&serialized).unwrap(),
        *plan.report()
    );
}
#[test]
fn aliases_refuse_both_without_order_or_equal_value_precedence() {
    for attributes in [
        r#"c:Exposure="1" c:Exposure2012="2" c:Contrast="10" c:Contrast2012="10" c:Saturation="6""#,
        r#"c:Exposure2012="2" c:Exposure="1" c:Contrast2012="10" c:Contrast="10" c:Saturation="6""#,
    ] {
        let identity = target(SourceKind::Rendered);
        let plan = parse(&packet(attributes, ""), identity.clone()).unwrap();
        assert_eq!(plan.mapped_count(), 1);
        let mut base = Settings::default();
        base.exposure = -0.75;
        base.contrast = -30.;
        let settings = plan.reviewed_settings(&base, &identity).unwrap();
        assert_eq!(settings.exposure, -0.75);
        assert_eq!(settings.contrast, -30.);
        assert_eq!(settings.saturation, 6.);
        for name in ["Exposure", "Exposure2012", "Contrast", "Contrast2012"] {
            assert_eq!(row(plan.report(), name).disposition, Disposition::Refused);
            assert!(row(plan.report(), name).explanation.contains("aliases"));
        }
    }
    assert!(
        parse(
            &packet(
                r#"c:Exposure2012="1""#,
                "<c:Exposure2012>1</c:Exposure2012>"
            ),
            target(SourceKind::Rendered)
        )
        .is_err()
    );
}
#[test]
fn raw_profile_kelvin_crop_and_local_masks_are_inert_and_target_source_remains_exact() {
    let input = packet(
        r#"c:Exposure2012="0.5" c:Temperature="6500" c:Tint="23" c:WhiteBalance="As Shot" c:RawFileName="other-camera.dng" c:CameraProfile="Commercial Profile" c:CameraProfileDigest="ABCDEF" c:CameraModelRestriction="Different camera" c:ProcessVersion="future-unknown" c:CropTop="0.1" c:LensProfileName="Commercial lens.lcp""#,
        "<c:PaintBasedCorrections><rdf:Seq><rdf:li>unsupported local mask</rdf:li></rdf:Seq></c:PaintBasedCorrections>",
    );
    for kind in [SourceKind::Rendered, SourceKind::DecodedRaw] {
        let identity = target(kind);
        let plan = parse(&input, identity.clone()).unwrap();
        let mut base = Settings::default();
        base.temperature = -14.;
        base.tint = 11.;
        let output = plan.reviewed_settings(&base, &identity).unwrap();
        assert_eq!(output.temperature, -14.);
        assert_eq!(output.tint, 11.);
        assert_eq!(plan.report().target, identity);
        assert_eq!(row(plan.report(), "RawFileName").input, "other-camera.dng");
        assert_eq!(row(plan.report(), "ProcessVersion").input, "future-unknown");
        for key in [
            "Temperature",
            "Tint",
            "WhiteBalance",
            "CameraProfile",
            "CropTop",
            "LensProfileName",
            "PaintBasedCorrections",
        ] {
            assert_eq!(row(plan.report(), key).disposition, Disposition::Refused);
        }
        assert_eq!(row(plan.report(), "CameraProfileDigest").input, "ABCDEF");
    }
}
#[test]
fn nonfinite_out_of_adapter_range_and_legacy_nonequivalent_sliders_are_refused_not_clamped() {
    let identity = target(SourceKind::Rendered);
    let plan = parse(&packet(r#"c:Exposure2012="NaN" c:Contrast2012="101" c:Highlights2012="-101" c:Vibrance="inf" c:Shadows="50" c:Brightness="70" c:Saturation="12""#, ""), identity.clone()).unwrap();
    assert_eq!(plan.mapped_count(), 1);
    let base = Settings::default();
    let settings = plan.reviewed_settings(&base, &identity).unwrap();
    assert_eq!(settings.exposure, base.exposure);
    assert_eq!(settings.shadows, base.shadows);
    assert_eq!(settings.saturation, 12.);
    for key in [
        "Exposure2012",
        "Contrast2012",
        "Highlights2012",
        "Vibrance",
        "Shadows",
        "Brightness",
    ] {
        assert_eq!(row(plan.report(), key).disposition, Disposition::Refused);
    }
}
#[test]
fn parser_enforces_namespace_xml_work_budgets_and_no_implicit_resource_resolution() {
    let identity = target(SourceKind::Rendered);
    let only_resource = packet(
        "",
        r#"<c:CameraProfile rdf:resource="file:///never-load-commercial.dcp"/>"#,
    );
    let plan = parse(&only_resource, identity.clone()).unwrap();
    assert_eq!(plan.mapped_count(), 0);
    assert_eq!(plan.report().rows[0].disposition, Disposition::Refused);
    assert!(
        plan.reviewed_settings(&Settings::default(), &identity)
            .is_err()
    );
    assert!(
        parse(
            b"<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///etc/passwd'>]><x/>",
            identity.clone()
        )
        .is_err()
    );
    assert!(parse(b"<bad", identity.clone()).is_err());
    assert!(parse(&vec![b' '; MAX_INPUT_BYTES + 1], identity.clone()).is_err());
    assert!(
        parse(
            &packet(&format!("c:ProcessVersion=\"{}\"", "x".repeat(1025)), ""),
            identity.clone()
        )
        .is_err()
    );
    let properties: String = (0..129).map(|i| format!("c:Unknown{i}=\"1\" ")).collect();
    assert!(parse(&packet(&properties, ""), identity.clone()).is_err());
    let nested = format!("{}{}", "<c:a>".repeat(18), "</c:a>".repeat(18));
    assert!(parse(&packet("", &nested), identity.clone()).is_err());
    let spoof = packet(
        r#"xmlns:evil="https://example.invalid/spoof" evil:Exposure2012="3""#,
        "",
    );
    let plan = parse(&spoof, identity).unwrap();
    assert_eq!(plan.mapped_count(), 0);
}
#[test]
fn target_replacement_revision_and_source_kind_changes_invalidate_the_review() {
    let identity = target(SourceKind::Rendered);
    let plan = parse(&packet(r#"c:Exposure2012="1""#, ""), identity.clone()).unwrap();
    for altered in [
        TargetIdentity {
            revision: identity.revision + 1,
            ..identity.clone()
        },
        TargetIdentity {
            source_id: "replacement".into(),
            ..identity.clone()
        },
        TargetIdentity {
            editor_instance: 8,
            ..identity.clone()
        },
        TargetIdentity {
            kind: SourceKind::DecodedRaw,
            ..identity.clone()
        },
        TargetIdentity {
            processing: "Native16V1".into(),
            ..identity.clone()
        },
        TargetIdentity {
            camera_profile: Some("changed".into()),
            ..identity.clone()
        },
    ] {
        assert!(
            plan.reviewed_settings(&Settings::default(), &altered)
                .is_err()
        );
    }
}

#[test]
fn local_file_import_never_rewrites_source_and_refuses_profiles_oversize_and_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("user-owned.xmp");
    let input = packet(r#"c:Exposure2012="0.75""#, "");
    std::fs::write(&path, &input).unwrap();
    let plan = import(&path, target(SourceKind::Rendered)).unwrap();
    assert_eq!(plan.mapped_count(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), input);
    assert_eq!(
        plan.report().source_sha256,
        omuse::asset_library::sha256_hex(&input)
    );
    let profile = directory.path().join("commercial.dcp");
    std::fs::write(&profile, &input).unwrap();
    assert!(import(&profile, target(SourceKind::Rendered)).is_err());
    let oversized = directory.path().join("too-large.xmp");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(MAX_INPUT_BYTES as u64 + 1)
        .unwrap();
    assert!(import(&oversized, target(SourceKind::Rendered)).is_err());
    #[cfg(unix)]
    {
        let link = directory.path().join("linked.xmp");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(import(&link, target(SourceKind::Rendered)).is_err());
    }
}

#[test]
fn explicit_promotion_rebuilds_review_from_same_bounded_xml_without_repick_or_hidden_apply() {
    let before = target(SourceKind::Rendered);
    let plan = parse(&packet(r#"c:Exposure2012="1.25""#, ""), before.clone()).unwrap();
    let after = TargetIdentity {
        revision: before.revision + 1,
        source_id: "explicitly-promoted-source".into(),
        processing: "Native16V1".into(),
        ..before.clone()
    };
    let rebuilt = plan.for_promoted_target(after.clone()).unwrap();
    assert_eq!(rebuilt.report().source_sha256, plan.report().source_sha256);
    assert_eq!(rebuilt.mapped_count(), plan.mapped_count());
    assert_eq!(rebuilt.report().target, after);
    assert!(
        plan.reviewed_settings(&Settings::default(), &after)
            .is_err()
    );
    assert_eq!(
        before.processing, "ByteCompatible",
        "rebuilding never mutates the old review/target"
    );
}
