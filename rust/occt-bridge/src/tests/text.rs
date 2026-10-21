use super::*;

/// The first installed of several widely packaged sans fonts, or `None`
/// when the machine has none of them (the tests then only check errors).
fn sans(session: &Session) -> Option<String> {
    ["Liberation Sans", "DejaVu Sans", "Noto Sans", "FreeSans"]
        .into_iter()
        .find_map(|family| session.font_file(family, FontStyle::Regular).ok())
}

fn text<'a>(session: &'a Session, font: &str, s: &str, height: f64, align: TextAlign) -> Shape<'a> {
    session
        .create_text(
            s,
            font,
            height,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            align,
        )
        .unwrap()
}

#[test]
fn glyph_outlines_become_faces_with_holes_that_scale_exactly() {
    let session = Session::new().unwrap();
    let Some(font) = sans(&session) else {
        eprintln!("no common sans font installed; skipping glyph geometry checks");
        return;
    };
    assert!(std::path::Path::new(&font).exists());
    // H: one face; i: stem and dot; 8: one face with two holes; %: three.
    for (s, faces, holes) in [("H", 1, 0), ("i", 2, 0), ("8", 1, 2), ("%", 3, 2)] {
        let shape = text(&session, &font, s, 10.0, TextAlign::Start);
        assert!(session.is_valid(&shape).unwrap(), "{s}");
        assert_eq!(
            session.subshape_count(&shape, ShapeType::Face).unwrap(),
            faces,
            "{s}"
        );
        assert_eq!(
            session.subshape_count(&shape, ShapeType::Wire).unwrap(),
            faces + holes,
            "{s}"
        );
    }
    // Exact outlines scale with the square of the em size.
    let small = session
        .surface_area(&text(&session, &font, "Model 42", 5.0, TextAlign::Start))
        .unwrap();
    let large = session
        .surface_area(&text(&session, &font, "Model 42", 10.0, TextAlign::Start))
        .unwrap();
    assert!(
        (large - 4.0 * small).abs() < 1e-9 * large,
        "{small} {large}"
    );
    // Centring and end alignment move the advance width about the origin.
    let start = session
        .exact_bounds(&text(&session, &font, "Plate", 10.0, TextAlign::Start))
        .unwrap();
    let centre = session
        .exact_bounds(&text(&session, &font, "Plate", 10.0, TextAlign::Center))
        .unwrap();
    let end = session
        .exact_bounds(&text(&session, &font, "Plate", 10.0, TextAlign::End))
        .unwrap();
    let advance = 2.0 * (start.min.x - centre.min.x);
    assert!(advance > start.max.x - start.min.x - 1e-9);
    assert!((start.min.x - end.min.x - advance).abs() < 1e-9);
    // Planar on z = 0, up to edge tolerance in the bounds.
    assert!(start.min.z.abs() < 1e-6 && start.max.z.abs() < 1e-6);
}

#[test]
fn invalid_fonts_and_text_fail_without_storing_shapes() {
    let session = Session::new().unwrap();
    let count = session.shape_count().unwrap();
    assert!(
        session
            .font_file("No Such Family 4711", FontStyle::Regular)
            .is_err()
    );
    if let Some(font) = sans(&session) {
        let place = |s: &str, font: &str, height: f64| {
            session.create_text(
                s,
                font,
                height,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                TextAlign::Start,
            )
        };
        for (s, file, height, message) in [
            ("ok", font.as_str(), 0.0, "invalid text parameters"),
            ("two\nlines", font.as_str(), 5.0, "printable UTF-8"),
            ("   ", font.as_str(), 5.0, "no visible glyphs"),
            ("\u{10FFFD}", font.as_str(), 5.0, "no glyph for U+10FFFD"),
            (
                "ok",
                "/nonexistent/font.ttf",
                5.0,
                "not a readable scalable font",
            ),
        ] {
            let error = place(s, file, height).unwrap_err();
            assert!(
                error.message.contains(message),
                "{message}: {}",
                error.message
            );
        }
        let long = "x".repeat(1001);
        assert!(
            place(&long, &font, 5.0)
                .unwrap_err()
                .message
                .contains("1000 characters")
        );
    }
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn text_frames_preserve_holes_area_and_extruded_volume() {
    let session = Session::new().unwrap();
    let Some(font) = sans(&session) else { return };
    let count = session.shape_count().unwrap();
    for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
        for label in ["AV 8i", "ÅéΩ"] {
            let planar = text(&session, &font, label, 10.0, align);
            let placed = session
                .create_text(
                    label,
                    &font,
                    10.0,
                    Vec3::new(4.0, 5.0, 6.0),
                    Vec3::new(0.0, 0.0, 2.0),
                    Vec3::new(0.0, -3.0, 0.0),
                    align,
                )
                .unwrap();
            assert!(session.is_valid(&placed).unwrap());
            for kind in [ShapeType::Face, ShapeType::Wire] {
                assert_eq!(
                    session.subshape_count(&planar, kind).unwrap(),
                    session.subshape_count(&placed, kind).unwrap()
                );
            }
            let area = session.surface_area(&planar).unwrap();
            assert!((session.surface_area(&placed).unwrap() - area).abs() < area * 1e-9);
            let a = session.exact_bounds(&planar).unwrap();
            let b = session.exact_bounds(&placed).unwrap();
            for (actual, expected) in [
                (b.min.x, 4.0),
                (b.max.x, 4.0),
                (b.min.y, 5.0 - a.max.y),
                (b.max.y, 5.0 - a.min.y),
                (b.min.z, 6.0 + a.min.x),
                (b.max.z, 6.0 + a.max.x),
            ] {
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "{label}: {actual} {expected}"
                );
            }
            let mut volume = 0.0;
            for face in session.subshapes(&placed, ShapeType::Face).unwrap() {
                let solid = session
                    .create_prism_from_face(&face, Vec3::new(2.0, 0.0, 0.0))
                    .unwrap();
                assert!(session.is_valid(&solid).unwrap());
                volume += session.volume(&solid).unwrap();
            }
            assert!((volume - 2.0 * area).abs() < area * 1e-9);
        }
    }
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn malformed_utf8_and_axis_errors_preserve_text_failure_precedence() {
    let session = Session::new().unwrap();
    let file = CString::new("/nonexistent/font.ttf").unwrap();
    let count = session.shape_count().unwrap();
    let origin = Vec3::new(0.0, 0.0, 0.0);
    for bytes in [
        &b"\x80"[..],
        &b"\xc0\xaf"[..],
        &b"\xe2\x82"[..],
        &b"\xed\xa0\x80"[..],
        &b"\xf4\x90\x80\x80"[..],
        &b"a\0b"[..],
    ] {
        let mut shape = 123;
        let status = unsafe {
            occt_bridge_create_text(
                session.raw.as_ptr(),
                bytes.as_ptr().cast(),
                bytes.len(),
                file.as_ptr(),
                10.0,
                origin.into(),
                Vec3::new(1.0, 0.0, 0.0).into(),
                Vec3::new(0.0, 1.0, 0.0).into(),
                0,
                &mut shape,
            )
        };
        assert_eq!(status, 1);
        assert_eq!(shape, 0);
        assert!(session.error(status).message.contains("printable UTF-8"));
    }
    let error = session
        .create_text(
            "\n",
            file.to_str().unwrap(),
            10.0,
            origin,
            origin,
            Vec3::new(0.0, 1.0, 0.0),
            TextAlign::Start,
        )
        .unwrap_err();
    assert!(
        error
            .message
            .contains("axes must be nonzero and perpendicular")
    );
    assert_eq!(session.shape_count().unwrap(), count);
}
