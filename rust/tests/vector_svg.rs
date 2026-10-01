use omuse::{
    vector_path::{Anchor, FillRule, Point, StrokeStyle, Subpath, VectorPath},
    vector_svg::{SvgArtwork, decode, encode, export, import, prepare_export},
};
use resvg::{tiny_skia, usvg};
use std::fs;
use tempfile::tempdir;

fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn render(svg: &str, width: u32, height: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.take()
}

fn assert_point(actual: Point, expected: Point) {
    assert!(
        (actual.x - expected.x).abs() < 1.0e-4,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 1.0e-4,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn cubic_geometry_alpha_and_fill_rule_round_trip_editably() {
    let artwork = SvgArtwork {
        width: 80,
        height: 60,
        path: VectorPath {
            fill_rule: FillRule::EvenOdd,
            subpaths: vec![
                Subpath {
                    closed: true,
                    anchors: vec![
                        Anchor {
                            position: point(8.125, 10.25),
                            incoming: Some(point(4.5, 15.75)),
                            outgoing: Some(point(12.25, 2.875)),
                        },
                        Anchor {
                            position: point(55.75, 12.5),
                            incoming: Some(point(40.125, 1.5)),
                            outgoing: Some(point(70.0, 30.25)),
                        },
                        Anchor {
                            position: point(20.375, 50.625),
                            incoming: Some(point(60.5, 54.25)),
                            outgoing: Some(point(11.75, 42.125)),
                        },
                    ],
                },
                Subpath {
                    closed: false,
                    anchors: vec![
                        Anchor {
                            position: point(5.0, 55.0),
                            incoming: None,
                            outgoing: None,
                        },
                        Anchor {
                            position: point(70.0, 55.0),
                            incoming: None,
                            outgoing: None,
                        },
                    ],
                },
            ],
        },
        fill: Some([17, 34, 51, 129]),
        stroke: Some(StrokeStyle {
            color: [210, 90, 12, 77],
            width: 3.25,
        }),
    };

    let first = encode(&artwork).unwrap();
    assert!(first.contains("stroke-linecap=\"round\""));
    assert!(first.contains("stroke-linejoin=\"round\""));
    let decoded = decode(first.as_bytes()).unwrap();
    assert_eq!(decoded.width, artwork.width);
    assert_eq!(decoded.height, artwork.height);
    assert_eq!(decoded.fill, artwork.fill);
    assert_eq!(decoded.stroke, artwork.stroke);
    assert_eq!(decoded.path.fill_rule, FillRule::EvenOdd);
    assert_eq!(decoded.path.subpaths.len(), 2);
    assert!(decoded.path.subpaths[0].closed);
    assert!(!decoded.path.subpaths[1].closed);
    assert_eq!(decoded.path.subpaths[0].anchors.len(), 3);
    for (actual, expected) in decoded.path.subpaths[0]
        .anchors
        .iter()
        .zip(&artwork.path.subpaths[0].anchors)
    {
        assert_point(actual.position, expected.position);
        assert_point(actual.incoming.unwrap(), expected.incoming.unwrap());
        assert_point(actual.outgoing.unwrap(), expected.outgoing.unwrap());
    }

    let second = encode(&decoded).unwrap();
    assert_eq!(render(&first, 80, 60), render(&second, 80, 60));
    assert_eq!(decode(second.as_bytes()).unwrap(), decoded);
}

#[test]
fn normalizes_standard_shape_and_canvas_transform() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 50">
      <circle cx="25" cy="20" r="10" fill="#123456" fill-opacity="0.4"/>
    </svg>"##;
    let artwork = decode(svg).unwrap();
    assert_eq!((artwork.width, artwork.height), (200, 100));
    assert_eq!(artwork.fill, Some([0x12, 0x34, 0x56, 102]));
    assert_eq!(artwork.stroke, None);
    assert_eq!(artwork.path.subpaths.len(), 1);
    assert!(artwork.path.subpaths[0].closed);
    let (lo, hi) = artwork.path.bounds().unwrap();
    assert!((lo.x - 30.0).abs() < 1.0e-3);
    assert!((lo.y - 20.0).abs() < 1.0e-3);
    assert!((hi.x - 70.0).abs() < 1.0e-3);
    assert!((hi.y - 60.0).abs() < 1.0e-3);
}

#[test]
fn accepts_exact_quadratic_as_cubic() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="50" height="50">
      <path d="M 0 0 Q 15 30 30 0" fill="none" stroke="#010203"
        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>"##;
    let artwork = decode(svg).unwrap();
    let anchors = &artwork.path.subpaths[0].anchors;
    assert_eq!(anchors.len(), 2);
    assert_point(anchors[0].outgoing.unwrap(), point(10.0, 20.0));
    assert_point(anchors[1].incoming.unwrap(), point(20.0, 20.0));
}

#[test]
fn every_u8_alpha_value_survives_svg_opacity_conversion() {
    let mut artwork = SvgArtwork {
        width: 2,
        height: 2,
        path: VectorPath {
            fill_rule: FillRule::NonZero,
            subpaths: vec![Subpath {
                closed: true,
                anchors: vec![
                    Anchor {
                        position: point(0.0, 0.0),
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: point(2.0, 0.0),
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: point(0.0, 2.0),
                        incoming: None,
                        outgoing: None,
                    },
                ],
            }],
        },
        fill: Some([3, 7, 11, 255]),
        stroke: None,
    };
    for alpha in 0..=u8::MAX {
        artwork.fill.as_mut().unwrap()[3] = alpha;
        let decoded = decode(encode(&artwork).unwrap().as_bytes()).unwrap();
        assert_eq!(decoded.fill.unwrap()[3], alpha, "alpha {alpha}");
    }
}

#[test]
fn rejects_content_that_cannot_remain_one_editable_path() {
    let cases = [
        (
            "text",
            r#"<text x="1" y="10">words</text>"#,
            "text is unsupported",
        ),
        (
            "bitmap",
            r#"<image href="file:///tmp/private.png" width="10" height="10"/>"#,
            "bitmap/media content is unsupported",
        ),
        (
            "gradient",
            r#"<defs><linearGradient id="g"><stop/></linearGradient></defs><path d="M0 0L5 0L5 5Z" fill="url(#g)"/>"#,
            "gradients and patterns are unsupported",
        ),
        (
            "filter",
            r#"<filter id="f"><feGaussianBlur stdDeviation="1"/></filter><path d="M0 0L5 0L5 5Z" filter="url(#f)"/>"#,
            "filters and effects are unsupported",
        ),
        (
            "mask",
            r#"<mask id="m"><path d="M0 0L5 0L5 5Z"/></mask><path d="M0 0L5 0L5 5Z" mask="url(#m)"/>"#,
            "masks and clipping paths are unsupported",
        ),
    ];
    for (name, body, expected) in cases {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">{body}</svg>"#
        );
        let error = decode(svg.as_bytes()).unwrap_err().to_string();
        assert!(error.contains(expected), "{name}: {error}");
    }

    let multiple = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
      <path d="M0 0L5 0L5 5Z"/><path d="M5 5L9 5L9 9Z"/>
    </svg>"#;
    assert!(
        decode(multiple)
            .unwrap_err()
            .to_string()
            .contains("exactly one")
    );
}

#[test]
fn rejects_strokes_with_different_raster_semantics() {
    let butt = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <path d="M2 2L18 18" fill="none" stroke="black" stroke-width="2"/>
    </svg>"#;
    assert!(
        decode(butt)
            .unwrap_err()
            .to_string()
            .contains("round line caps")
    );

    let dash = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <path d="M2 2L18 18" fill="none" stroke="black" stroke-width="2"
       stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="2 2"/>
    </svg>"#;
    assert!(decode(dash).unwrap_err().to_string().contains("Dashed"));

    let stretched = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <path d="M2 2L8 8" transform="scale(2 1)" fill="none" stroke="black"
       stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>"#;
    assert!(
        decode(stretched)
            .unwrap_err()
            .to_string()
            .contains("Non-uniform")
    );

    let object_opacity = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <path d="M2 2L18 2L18 18Z" fill="red" stroke="blue" opacity="0.5"
       stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>"#;
    assert!(
        decode(object_opacity)
            .unwrap_err()
            .to_string()
            .contains("object opacity")
    );

    let styled_opacity = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <path d="M2 2L18 2L18 18Z" style="fill:red; opacity:0.5"/>
    </svg>"#;
    assert!(
        decode(styled_opacity)
            .unwrap_err()
            .to_string()
            .contains("object opacity")
    );
}

#[test]
fn rejects_malformed_resource_and_work_limit_inputs() {
    let dtd = br#"<!DOCTYPE svg [<!ENTITY x "boom">]><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path d="M0 0L1 1"/></svg>"#;
    assert!(decode(dtd).unwrap_err().to_string().contains("DTD"));

    let external_style = br#"<?xml-stylesheet href="https://example.invalid/a.css"?><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path d="M0 0L1 1"/></svg>"#;
    assert!(
        decode(external_style)
            .unwrap_err()
            .to_string()
            .contains("stylesheets")
    );

    let processing = br#"<?omuse-resource href="file:///tmp/private"?><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path d="M0 0L1 1"/></svg>"#;
    assert!(
        decode(processing)
            .unwrap_err()
            .to_string()
            .contains("processing instructions")
    );

    let foreign_element = br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:x="https://example.invalid/ns" width="1" height="1"><x:path d="M0 0L1 1"/></svg>"#;
    assert!(
        decode(foreign_element)
            .unwrap_err()
            .to_string()
            .contains("Foreign XML namespace")
    );

    let foreign_attribute = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path xmlns:x="https://example.invalid/ns" x:source="file:///tmp/private" d="M0 0L1 1"/></svg>"#;
    assert!(
        decode(foreign_attribute)
            .unwrap_err()
            .to_string()
            .contains("Foreign XML namespace")
    );

    let over_limit = vec![b' '; 4 * 1024 * 1024 + 1];
    assert!(
        decode(&over_limit)
            .unwrap_err()
            .to_string()
            .contains("4 MiB")
    );

    let mut nested =
        String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#);
    for _ in 0..50 {
        nested.push_str("<g>");
    }
    nested.push_str(r#"<path d="M0 0L1 1"/>"#);
    for _ in 0..50 {
        nested.push_str("</g>");
    }
    nested.push_str("</svg>");
    assert!(
        decode(nested.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );

    let mut too_many_anchors = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path d="M0 0"#,
    );
    for _ in 0..100_001 {
        too_many_anchors.push_str(" L1 1");
    }
    too_many_anchors.push_str(r#""/></svg>"#);
    assert!(
        decode(too_many_anchors.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("too many vector anchors")
    );
}

#[test]
fn file_io_is_bounded_regular_and_never_overwrites() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("path.svg");
    let artwork = SvgArtwork {
        width: 10,
        height: 10,
        path: VectorPath {
            fill_rule: FillRule::NonZero,
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: point(1.0, 1.0),
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: point(9.0, 9.0),
                        incoming: None,
                        outgoing: None,
                    },
                ],
            }],
        },
        fill: None,
        stroke: Some(StrokeStyle {
            color: [1, 2, 3, 255],
            width: 1.0,
        }),
    };
    export(&destination, &artwork).unwrap();
    let original = fs::read(&destination).unwrap();
    assert_eq!(import(&destination).unwrap(), artwork);
    assert!(export(&destination, &artwork).is_err());
    assert_eq!(fs::read(&destination).unwrap(), original);
    assert!(
        fs::read_dir(dir.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse-vector-svg-")
        }),
        "failed publication left a staged sibling behind"
    );

    let oversized = dir.path().join("oversized.svg");
    let oversized_file = fs::File::create(&oversized).unwrap();
    oversized_file.set_len(4 * 1024 * 1024 + 1).unwrap();
    drop(oversized_file);
    assert!(
        import(&oversized)
            .unwrap_err()
            .to_string()
            .contains("4 MiB")
    );

    #[cfg(target_os = "linux")]
    {
        let link = dir.path().join("linked.svg");
        std::os::unix::fs::symlink(&destination, &link).unwrap();
        assert!(
            import(&link)
                .unwrap_err()
                .to_string()
                .contains("safely open")
        );

        use std::{
            ffi::CString,
            os::unix::ffi::OsStrExt,
            time::{Duration, Instant},
        };
        unsafe extern "C" {
            fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> i32;
        }
        let fifo = dir.path().join("input.fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: fifo_c is a live NUL-terminated path and mkfifo retains no pointer.
        assert_eq!(unsafe { mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        let started = Instant::now();
        assert!(
            import(&fifo)
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn prepared_export_drop_never_publishes_or_leaves_staging() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("cancelled.svg");
    let artwork = SvgArtwork {
        width: 10,
        height: 10,
        path: VectorPath {
            fill_rule: FillRule::NonZero,
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: point(1.0, 1.0),
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: point(9.0, 9.0),
                        incoming: None,
                        outgoing: None,
                    },
                ],
            }],
        },
        fill: None,
        stroke: Some(StrokeStyle {
            color: [1, 2, 3, 255],
            width: 1.0,
        }),
    };
    let prepared = prepare_export(&destination, &artwork).unwrap();
    assert_eq!(prepared.destination(), destination);
    assert!(!destination.exists());
    assert!(fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omuse-vector-svg-")
    }));
    drop(prepared);
    assert!(!destination.exists());
    assert!(fs::read_dir(dir.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omuse-vector-svg-")
    }));

    fs::write(&destination, b"existing").unwrap();
    let prepared = prepare_export(&destination, &artwork).unwrap();
    assert!(prepared.publish().is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"existing");
    assert!(fs::read_dir(dir.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omuse-vector-svg-")
    }));
}

#[test]
fn encode_rejects_non_drawable_and_invalid_dimensions() {
    let empty = SvgArtwork {
        width: 10,
        height: 10,
        path: VectorPath::default(),
        fill: Some([0, 0, 0, 255]),
        stroke: None,
    };
    assert!(encode(&empty).unwrap_err().to_string().contains("drawable"));
    let mut invalid = empty;
    invalid.width = 0;
    assert!(encode(&invalid).unwrap_err().to_string().contains("canvas"));
}

#[test]
fn oversized_high_precision_geometry_is_not_encoded_or_published() {
    let repeated = Anchor {
        position: point(-999_999.94, 999_999.94),
        incoming: Some(point(999_999.94, -999_999.94)),
        outgoing: Some(point(-999_999.94, 999_999.94)),
    };
    let artwork = SvgArtwork {
        width: 10,
        height: 10,
        path: VectorPath {
            fill_rule: FillRule::NonZero,
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![repeated; 70_000],
            }],
        },
        fill: Some([1, 2, 3, 4]),
        stroke: None,
    };
    let error = encode(&artwork).unwrap_err().to_string();
    assert!(error.contains("4 MiB reimport limit"), "{error}");

    let dir = tempdir().unwrap();
    let destination = dir.path().join("too-large.svg");
    fs::write(&destination, b"existing destination").unwrap();
    let error = export(&destination, &artwork).unwrap_err().to_string();
    assert!(error.contains("4 MiB reimport limit"), "{error}");
    assert_eq!(fs::read(&destination).unwrap(), b"existing destination");
    assert!(
        fs::read_dir(dir.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse-vector-svg-")
        }),
        "over-budget encode created a staging file"
    );
}
