//! Overhead of result validation on boolean chains and many-hole faces.

use super::*;

/// Drills `VALIDATED_CUTS` holes into a plate one boolean at a time, with
/// result validation off and on, and bounds the validation overhead.
pub(crate) fn validation_cases() -> Vec<Outcome> {
    let drill = |validate: bool| -> Result<Duration, ModelError> {
        let session = Session::new()?;
        session.set_options(SessionOptions {
            validate_results: validate,
            ..SessionOptions::default()
        })?;
        let mut plate =
            session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(400.0, 400.0, 10.0))?;
        let start = Instant::now();
        for index in 0..VALIDATED_CUTS {
            let (row, column) = ((index / 10) as f64, (index % 10) as f64);
            let hole = session.create_cylinder(
                Vec3::new(20.0 + column * 38.0, 20.0 + row * 38.0, -1.0),
                Vec3::new(0.0, 0.0, 1.0),
                6.0,
                12.0,
            )?;
            let drilled = session.cut(&plate, &hole)?;
            session.remove(hole)?;
            session.remove(std::mem::replace(&mut plate, drilled))?;
        }
        Ok(start.elapsed())
    };
    let mut baseline = Duration::ZERO;
    let unchecked = timed(
        format!("{VALIDATED_CUTS} sequential cuts: validation off"),
        ms(10_000),
        Expectation::Required,
        || {
            baseline = drill(false)?;
            Ok("baseline".into())
        },
    );
    let checked = timed(
        format!("{VALIDATED_CUTS} sequential cuts: validation on"),
        ms(15_000),
        Expectation::Required,
        || {
            let elapsed = drill(true)?;
            let ratio = elapsed.as_secs_f64() / baseline.as_secs_f64().max(1e-9);
            if ratio <= VALIDATION_OVERHEAD_LIMIT {
                Ok(format!("{ratio:.2}x of unvalidated"))
            } else {
                Err(failure(format!(
                    "validation costs {ratio:.2}x, limit {VALIDATION_OVERHEAD_LIMIT}x"
                )))
            }
        },
    );
    let mut outcomes = vec![unchecked, checked];
    outcomes.extend(many_hole_validation_cases());
    outcomes
}

/// Drills `MANY_HOLES` holes into one plate with a single boolean, with
/// validation off and on, and bounds the overhead of checking a face with
/// hundreds of wires.
pub(crate) fn many_hole_validation_cases() -> Vec<Outcome> {
    let drill = |validate: bool| -> Result<Duration, ModelError> {
        let session = Session::new()?;
        session.set_options(SessionOptions {
            validate_results: validate,
            ..SessionOptions::default()
        })?;
        let plate = session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(420.0, 420.0, 10.0))?;
        let holes = (0..MANY_HOLES)
            .map(|index| {
                let (row, column) = ((index / 20) as f64, (index % 20) as f64);
                session.create_cylinder(
                    Vec3::new(10.0 + column * 20.0, 10.0 + row * 20.0, -1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    4.0,
                    12.0,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tools = session.create_compound(&holes.iter().collect::<Vec<_>>())?;
        let start = Instant::now();
        let drilled = session.cut(&plate, &tools)?;
        let elapsed = start.elapsed();
        let faces = session.subshape_count(&drilled, ShapeType::Face)?;
        if faces != MANY_HOLES + 6 {
            return Err(failure(format!(
                "expected {} faces, found {faces}",
                MANY_HOLES + 6
            )));
        }
        Ok(elapsed)
    };
    let mut baseline = Duration::ZERO;
    let unchecked = timed(
        format!("{MANY_HOLES}-hole plate in one cut: validation off"),
        ms(5_000),
        Expectation::Required,
        || {
            baseline = drill(false)?;
            Ok("baseline".into())
        },
    );
    let checked = timed(
        format!("{MANY_HOLES}-hole plate in one cut: validation on"),
        ms(7_500),
        Expectation::Required,
        || {
            let elapsed = drill(true)?;
            let ratio = elapsed.as_secs_f64() / baseline.as_secs_f64().max(1e-9);
            if ratio <= MANY_HOLES_OVERHEAD_LIMIT {
                Ok(format!("{ratio:.2}x of unvalidated"))
            } else {
                Err(failure(format!(
                    "validation costs {ratio:.2}x, limit {MANY_HOLES_OVERHEAD_LIMIT}x"
                )))
            }
        },
    );
    vec![unchecked, checked]
}
