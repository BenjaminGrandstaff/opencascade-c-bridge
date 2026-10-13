use super::*;
use occt_bridge::{BSplineBasis, BSplineRefinementAction};
use occt_parametric::SketchBSplineBasis;

impl Studio {
    /// Read-only polynomial coefficient transform; rational coordinates and weights
    /// remain authored expressions in the isolated client draft.
    pub(super) fn spline_refine(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            version: u64,
            basis: SketchBSplineBasis,
            pole_count: usize,
            action: String,
            knot: Option<f64>,
            degree: Option<i32>,
        }
        let request: Request =
            serde_json::from_slice(body).map_err(|e| format!("Invalid refinement request: {e}"))?;
        if request.version != self.version {
            return Err("The model changed. Reload the sketch before refining this draft.".into());
        }
        if request.basis.periodic && request.basis.non_clamped {
            return Err("Periodic and non-clamped flags cannot both be set.".into());
        }
        let action = match request.action.as_str() {
            "insert_knot" => {
                BSplineRefinementAction::InsertKnot(request.knot.ok_or("Enter a knot to insert.")?)
            }
            "elevate_degree" => BSplineRefinementAction::ElevateDegree(
                request.degree.ok_or("Enter an elevated degree.")?,
            ),
            _ => return Err("Choose knot insertion or degree elevation.".into()),
        };
        let basis = BSplineBasis {
            degree: request.basis.degree,
            periodic: request.basis.periodic,
            pole_count: request.pole_count,
            knots: &request.basis.knots,
            multiplicities: &request.basis.multiplicities,
        };
        let session = Session::new().map_err(|e| e.to_string())?;
        let result = session
            .refine_bspline(&basis, action)
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"version":self.version,"degree":result.degree,"knots":result.knots,"multiplicities":result.multiplicities,"coefficients":result.coefficients}),
        )
    }
}
