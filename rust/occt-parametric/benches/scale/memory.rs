//! Memory per rigidly placed copy.

use super::*;

/// Places `PLACED_COPIES` rigid copies of a 20-hole plate and bounds the
/// resident memory each copy adds; shared geometry keeps it near constant.
pub(crate) fn placed_copy_memory() -> Outcome {
    timed(
        format!("place {PLACED_COPIES} copies: memory per copy"),
        ms(5_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut plate =
                session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(200.0, 200.0, 10.0))?;
            for index in 0..20 {
                let (row, column) = ((index / 5) as f64, (index % 5) as f64);
                let hole = session.create_cylinder(
                    Vec3::new(20.0 + column * 40.0, 20.0 + row * 40.0, -1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    6.0,
                    12.0,
                )?;
                plate = session.cut(&plate, &hole)?;
            }
            let before = resident_kib();
            let copies = (0..PLACED_COPIES)
                .map(|index| {
                    let turned = session.rotate(
                        &plate,
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(0.0, 0.0, 1.0),
                        0.1,
                    )?;
                    session.translate(&turned, Vec3::new(index as f64 * 250.0, 0.0, 0.0))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let Some(grown) = resident_kib()
                .zip(before)
                .map(|(after, start)| after.saturating_sub(start))
            else {
                return Ok(format!(
                    "{} copies; memory not measurable here",
                    copies.len()
                ));
            };
            let per_copy = grown as f64 / PLACED_COPIES as f64;
            if per_copy <= PLACED_COPY_BUDGET_KIB {
                Ok(format!(
                    "{per_copy:.2} KiB per copy (budget {PLACED_COPY_BUDGET_KIB} KiB)"
                ))
            } else {
                Err(failure(format!(
                    "{per_copy:.1} KiB per copy exceeds {PLACED_COPY_BUDGET_KIB} KiB"
                )))
            }
        },
    )
}

/// Resident set size in KiB on Linux; `None` elsewhere.
pub(crate) fn resident_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}
