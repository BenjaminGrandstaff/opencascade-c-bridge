use super::*;

fn settings(d: f64, fastening: JointFastening) -> JointSettings {
    JointSettings {
        nominal_diameter: d,
        fastening,
        head: JointHead::Surface,
        washers: true,
        clearance: ClearanceSeries::Medium,
        length: None,
        engagement: None,
    }
}
fn spans(thicknesses: &[f64]) -> Vec<StackSpan> {
    let mut at = 0.0;
    thicknesses
        .iter()
        .map(|t| {
            let span = StackSpan {
                entry: at,
                exit: at + t,
            };
            at += t;
            span
        })
        .collect()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn error(result: Result<JointPlan, ModelError>) -> String {
    result.unwrap_err().message
}

#[test]
fn standard_lengths_follow_the_iso_4762_table() {
    assert_eq!(
        standard_screw_lengths(3.0),
        [5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 25.0, 30.0]
    );
    let m20 = standard_screw_lengths(20.0);
    assert_eq!((m20[0], *m20.last().unwrap()), (30.0, 200.0));
    assert!(m20.contains(&180.0) && !m20.contains(&170.0));
    assert!(standard_screw_lengths(14.0).is_empty());
    for (size, _) in SCREW_LENGTHS {
        let lengths = standard_screw_lengths(size);
        assert!(lengths.windows(2).all(|pair| pair[1] > pair[0]), "{size}");
        // Every size is a catalog screw.
        StandardPartKind::SocketHeadCapScrewIso4762
            .data(size)
            .unwrap();
    }
}

#[test]
fn a_nut_joint_picks_the_shortest_screw_past_the_nut() {
    // M6 through 10 + 8 mm plates, washers both sides: head washer 1.6,
    // nut washer 1.6, nut 5.0, two 1 mm pitches past it.
    let plan = plan_joint(
        &settings(6.0, JointFastening::Nut),
        &["lid", "base"],
        &[spans(&[10.0, 8.0]), spans(&[10.0, 8.0])],
    )
    .unwrap();
    close(plan.minimum_length, 1.6 + 18.0 + 1.6 + 5.0 + 2.0);
    assert_eq!((plan.length, plan.chosen, plan.pitch), (30.0, true, 1.0));
    let screw = &plan.screws[0];
    close(screw.seat, -1.6);
    assert_eq!(screw.head_washer, Some(-1.6));
    assert_eq!(screw.nut_washer, Some(18.0));
    close(screw.nut.unwrap(), 19.6);
    close(screw.grip, 21.2);
    close(screw.thread_engagement, -1.6 + 30.0 - 19.6 - 5.0);
    assert_eq!(screw.holes.len(), 2);
    assert_eq!(
        screw.holes[1],
        JointHole {
            kind: JointHoleKind::Clearance {
                diameter: 6.6,
                counterbore: None
            },
            entry: 10.0,
            exit: 18.0
        }
    );
    // Washer outside diameter is the widest piece.
    close(plan.footprint, 12.0);
    let items = plan.hardware();
    let lines: Vec<_> = items
        .iter()
        .map(|item| (item.designation.as_str(), item.quantity))
        .collect();
    assert_eq!(
        lines,
        [
            ("ISO 4762 M6 × 30", 2),
            ("ISO 7089 6", 4),
            ("ISO 4032 M6", 2)
        ]
    );
}

#[test]
fn a_tapped_joint_drills_a_blind_hole_with_room_for_its_point() {
    // M5 through a 6 mm lid into a 20 mm base: 1.0 mm washer, 7.5 mm
    // engagement, 0.8 mm pitch, 4.2 mm tap drill.
    let plan = plan_joint(
        &settings(5.0, JointFastening::Tapped),
        &["lid", "base"],
        &[spans(&[6.0, 20.0])],
    )
    .unwrap();
    close(plan.minimum_length, 1.0 + 6.0 + 7.5);
    assert_eq!(plan.length, 16.0);
    let screw = &plan.screws[0];
    assert_eq!((screw.nut, screw.nut_washer), (None, None));
    close(screw.thread_engagement, 9.0);
    close(screw.grip, 7.0);
    let JointHoleKind::Tapped {
        drill,
        depth,
        pitch,
    } = screw.holes[1].kind
    else {
        panic!("tapped hole expected");
    };
    close(drill, 4.2);
    close(pitch, 0.8);
    close(depth, 9.0 + 1.6);
    assert_eq!(
        plan.hardware()
            .iter()
            .map(|item| item.quantity)
            .collect::<Vec<_>>(),
        [1, 1]
    );
    // A thinner base cannot hold the hole and its point.
    let message = error(plan_joint(
        &settings(5.0, JointFastening::Tapped),
        &["lid", "base"],
        &[spans(&[6.0, 12.0])],
    ));
    assert!(
        message.contains("tapped hole in 'base'") && message.contains("12 mm thick"),
        "{message}"
    );
    // A longer engagement asks for a longer screw.
    let mut deeper = settings(5.0, JointFastening::Tapped);
    deeper.engagement = Some(10.0);
    let plan = plan_joint(&deeper, &["lid", "base"], &[spans(&[6.0, 30.0])]).unwrap();
    assert_eq!(plan.length, 20.0);
}

#[test]
fn a_counterbored_head_sits_flush_without_a_washer() {
    // Carr Lane M6 counterbore: 11.2 mm across, 6 mm deep.
    let mut sunk = settings(6.0, JointFastening::Nut);
    sunk.head = JointHead::Counterbored;
    let plan = plan_joint(&sunk, &["lid", "base"], &[spans(&[10.0, 8.0])]).unwrap();
    let screw = &plan.screws[0];
    close(screw.seat, 6.0);
    assert_eq!(screw.head_washer, None);
    assert!(screw.nut_washer.is_some());
    assert_eq!(
        screw.holes[0].kind,
        JointHoleKind::Clearance {
            diameter: 6.6,
            counterbore: Some((11.2, 6.0))
        }
    );
    close(plan.minimum_length, 18.0 - 6.0 + 1.6 + 5.0 + 2.0);
    assert_eq!(plan.length, 25.0);
    let message = error(plan_joint(&sunk, &["lid", "base"], &[spans(&[5.0, 8.0])]));
    assert!(
        message.contains("counterbore does not fit in 'lid'"),
        "{message}"
    );
}

#[test]
fn lengths_and_stacks_that_cannot_assemble_are_refused() {
    let two = ["lid", "base"];
    let nut = settings(6.0, JointFastening::Nut);
    // Parts must touch, in order.
    let mut gap = spans(&[10.0, 8.0]);
    gap[1].entry += 2.0;
    gap[1].exit += 2.0;
    let message = error(plan_joint(&nut, &two, &[spans(&[10.0, 8.0]), gap]));
    assert!(
        message.contains("'lid' and 'base' are 2 mm apart") && message.contains("position 2"),
        "{message}"
    );
    let mut overlap = spans(&[10.0, 8.0]);
    overlap[1].entry -= 1.0;
    assert!(error(plan_joint(&nut, &two, &[overlap])).contains("overlap by 1 mm"));
    // An explicit length must reach past the nut.
    let mut short = nut;
    short.length = Some(25.0);
    let message = error(plan_joint(&short, &two, &[spans(&[10.0, 8.0])]));
    assert!(
        message.contains("M6 × 25 screw is too short") && message.contains("28.2 mm"),
        "{message}"
    );
    // A long screw's plain shank would reach the nut.
    let mut long = nut;
    long.length = Some(60.0);
    let message = error(plan_joint(&long, &two, &[spans(&[10.0, 8.0])]));
    assert!(message.contains("thread (24 mm) starts"), "{message}");
    // No standard screw is long enough, and M14 has no length table.
    let message = error(plan_joint(&nut, &two, &[spans(&[30.0, 30.0])]));
    assert!(
        message.contains("longest standard one is 60 mm"),
        "{message}"
    );
    let message = error(plan_joint(
        &settings(14.0, JointFastening::Nut),
        &two,
        &[spans(&[10.0, 8.0])],
    ));
    assert!(message.contains("give the joint a length"), "{message}");
    let mut m14 = settings(14.0, JointFastening::Nut);
    m14.length = Some(45.0);
    plan_joint(&m14, &two, &[spans(&[10.0, 8.0])]).unwrap();
    // A tapped joint needs something to clamp; sizes must be catalog sizes.
    assert!(
        error(plan_joint(
            &settings(6.0, JointFastening::Tapped),
            &["base"],
            &[spans(&[20.0])]
        ))
        .contains("at least two parts")
    );
    assert!(
        error(plan_joint(
            &settings(7.0, JointFastening::Nut),
            &two,
            &[spans(&[10.0, 8.0])]
        ))
        .contains("no 7 mm size")
    );
}

#[test]
fn spacing_refuses_colliding_hardware() {
    check_spacing(&[[0.0, 0.0], [12.0, 0.0], [0.0, 12.0], [40.0, 40.0]], 12.0).unwrap();
    let message = check_spacing(&[[0.0, 0.0], [50.0, 0.0], [8.0, 6.0]], 12.0)
        .unwrap_err()
        .message;
    assert!(
        message.contains("positions 1 and 3 are 10 mm apart") && message.contains("need 12 mm"),
        "{message}"
    );
}
