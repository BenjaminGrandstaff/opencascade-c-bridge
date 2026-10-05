//! Feature colors carried to the faces of every feature output.

use super::*;
use std::collections::BTreeMap;

/// Colored faces of one output: face index (in `Session::subshapes` order)
/// and linear RGB color, ascending by index.
pub(crate) type FaceColors = Vec<(usize, [f64; 3])>;

/// Where each requested face of `input` went in `output`: itself when the
/// output kept it, otherwise its modified descendants; empty when removed.
/// One lookup for kept faces, then history queries only for the rest.
fn images(
    session: &Session,
    input: &Shape<'_>,
    output: &Shape<'_>,
    wanted: &[usize],
) -> Result<BTreeMap<usize, Vec<usize>>, ModelError> {
    let faces = session.subshapes(input, ShapeType::Face)?;
    let result = (|| {
        let references = wanted
            .iter()
            .filter_map(|index| faces.get(*index))
            .collect::<Vec<_>>();
        let kept = session.subshape_lookup(output, ShapeType::Face, &references)?;
        let mut images = BTreeMap::new();
        for (&index, found) in wanted.iter().zip(kept) {
            if let Some(found) = found {
                images.insert(index, vec![found]);
                continue;
            }
            let face = &faces[index];
            let count = session.history_count(output, face, HistoryRelation::Modified)?;
            let descendants = (0..count)
                .map(|position| session.history(output, face, HistoryRelation::Modified, position))
                .collect::<Result<Vec<_>, _>>();
            let descendants = descendants?;
            let refs = descendants.iter().collect::<Vec<_>>();
            let found = session.subshape_lookup(output, ShapeType::Face, &refs);
            cleanup_shapes(session, descendants);
            images.insert(index, found?.into_iter().flatten().collect());
        }
        Ok::<_, ModelError>(images)
    })();
    cleanup_shapes(session, faces);
    result
}

/// Colors of `feature`'s output faces: its inputs' colored faces carried
/// through the feature's history, then the feature's own color on the faces
/// no input face led to. O(input faces) lookups plus one history query per
/// carried or (for a colored feature) replaced face.
pub(crate) fn feature_face_colors(
    session: &Session,
    feature: &FeatureDefinition,
    own: Option<[f64; 3]>,
    shapes: &HashMap<String, Shape<'_>>,
    colored: &HashMap<String, FaceColors>,
) -> Result<FaceColors, ModelError> {
    let output = shape(shapes, &feature.id)?;
    let mut colors = BTreeMap::new();
    let mut reached = HashSet::new();
    for input in feature.operation.dependencies() {
        let Some(source) = shapes.get(input) else {
            continue;
        };
        let carried = colored.get(input).map(Vec::as_slice).unwrap_or_default();
        if carried.is_empty() && own.is_none() {
            continue;
        }
        let wanted = if own.is_some() {
            (0..session.subshape_count(source, ShapeType::Face)?).collect::<Vec<_>>()
        } else {
            carried.iter().map(|(index, _)| *index).collect()
        };
        let images = images(session, source, output, &wanted)?;
        reached.extend(images.values().flatten().copied());
        for (index, color) in carried {
            for target in images.get(index).into_iter().flatten() {
                colors.insert(*target, *color);
            }
        }
    }
    if let Some(color) = own {
        for index in 0..session.subshape_count(output, ShapeType::Face)? {
            if !reached.contains(&index) {
                colors.insert(index, color);
            }
        }
    }
    Ok(colors.into_iter().collect())
}
