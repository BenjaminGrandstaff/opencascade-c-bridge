use super::*;

/// Polynomial basis data; returned coefficients act on rational homogeneous poles.
pub struct BSplineBasis<'a> {
    pub degree: i32,
    pub periodic: bool,
    pub pole_count: usize,
    pub knots: &'a [f64],
    pub multiplicities: &'a [i32],
}
pub enum BSplineRefinementAction {
    InsertKnot(f64),
    ElevateDegree(i32),
}
pub struct BSplineRefinement {
    pub degree: i32,
    pub knots: Vec<f64>,
    pub multiplicities: Vec<i32>,
    pub coefficients: Vec<Vec<f64>>,
}
impl Session {
    /// Native knot insertion / degree elevation without allocating shape handles.
    /// Up to 256 input/output poles. Coefficients retain authored expression drivers.
    pub fn refine_bspline(
        &self,
        basis: &BSplineBasis<'_>,
        action: BSplineRefinementAction,
    ) -> Result<BSplineRefinement, BridgeError> {
        if basis.knots.len() != basis.multiplicities.len() || basis.pole_count > 256 {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                diagnostics: Vec::new(),
                message: "Refinement requires matching knot arrays and at most 256 poles".into(),
            });
        }
        let raw = RawBSplineBasis {
            degree: basis.degree,
            periodic: i32::from(basis.periodic),
            pole_count: basis.pole_count,
            knot_count: basis.knots.len(),
            knots: basis.knots.as_ptr(),
            multiplicities: basis.multiplicities.as_ptr(),
        };
        let mut knots = vec![0.; 514];
        let mut multiplicities = vec![0; 514];
        let mut coefficients = vec![0.; 256 * basis.pole_count];
        let mut out = RawBSplineRefinement {
            degree: 0,
            pole_count: 0,
            knot_count: 0,
            knots: knots.as_mut_ptr(),
            multiplicities: multiplicities.as_mut_ptr(),
            coefficients: coefficients.as_mut_ptr(),
            knot_capacity: knots.len(),
            coefficient_capacity: coefficients.len(),
        };
        let (action, knot, degree) = match action {
            BSplineRefinementAction::InsertKnot(k) => (0, k, 0),
            BSplineRefinementAction::ElevateDegree(d) => (1, 0., d),
        };
        // SAFETY: Input arrays match lengths; initialized output arrays cover the bounded C contract.
        self.check(unsafe {
            occt_bridge_bspline_refine(self.raw.as_ptr(), &raw, action, knot, degree, &mut out)
        })?;
        knots.truncate(out.knot_count);
        multiplicities.truncate(out.knot_count);
        coefficients.truncate(out.pole_count * basis.pole_count);
        Ok(BSplineRefinement {
            degree: out.degree,
            knots,
            multiplicities,
            coefficients: coefficients
                .chunks_exact(basis.pole_count)
                .map(|row| row.to_vec())
                .collect(),
        })
    }
}
