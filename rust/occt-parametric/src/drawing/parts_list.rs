//! Parts lists (bills of materials) and item balloons on drawings.
//!
//! Instances shown in a drawing's views are grouped into items by part
//! identity: family, effective parameter values, shown output and material.
//! Items are numbered by first appearance (view order, then output order),
//! so numbers are stable while views and outputs keep their order.
use super::*;
use crate::assembly::{compose, rigid};

/// A parts-list table, drawn with its top-left corner at `position_mm`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingPartsList {
    pub position_mm: [f64; 2],
    /// Part numbers by family ID; families without one show their ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub part_numbers: BTreeMap<String, String>,
}

/// An item balloon: a numbered circle with a leader to an instance in a view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingBalloon {
    pub id: String,
    pub view: String,
    pub instance: String,
    /// A datum on the instance for the leader; by default the instance's
    /// placed origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<DatumRef>,
    /// Balloon center relative to the leader point, in paper millimeters.
    pub offset_mm: [f64; 2],
}

/// One parts-list row.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PartsListItem {
    pub item: usize,
    pub quantity: usize,
    /// Part number, or the family ID with a variant suffix when one family
    /// appears with different parameter values.
    pub part: String,
    pub family: String,
    pub output: String,
    pub material: Option<String>,
    /// Instances in the item, in order of first appearance.
    pub instances: Vec<String>,
}

const ROW_MM: f64 = 7.0;
const COLUMNS_MM: [f64; 4] = [12.0, 12.0, 64.0, 40.0];
const HEADERS: [&str; 4] = ["ITEM", "QTY", "PART", "MATERIAL"];
const BALLOON_RADIUS_MM: f64 = 4.0;

impl DrawingDefinition {
    /// The drawing's items, grouping the unsuppressed instances its views
    /// show. O(shown instances × parameters) plus one resolution each.
    pub fn parts_list_items(
        &self,
        graph: &InstanceGraph<'_>,
    ) -> Result<Vec<PartsListItem>, ModelError> {
        let part_numbers = self.parts_list.as_ref().map(|list| &list.part_numbers);
        let mut items: Vec<PartsListItem> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut seen = HashSet::new();
        let mut variants: HashMap<String, Vec<String>> = HashMap::new();
        // Each item's parameter values, for naming variants of one family.
        let mut item_values: Vec<String> = Vec::new();
        for view in &self.views {
            for output in &view.outputs {
                if graph.is_suppressed(&output.instance) || !seen.insert(output.instance.as_str()) {
                    continue;
                }
                let resolved = graph.resolve(&output.instance)?;
                let family = resolved.definition.id.clone();
                let parameters: BTreeMap<_, _> =
                    resolve_parameters(resolved.definition, &resolved.overrides)?
                        .into_iter()
                        .collect();
                let material = graph
                    .material_of(&output.instance)?
                    .map(|material| material.name.clone());
                let values = serde_json::to_string(&parameters)
                    .map_err(|error| ModelError::new(format!("part identity: {error}")))?;
                let key = serde_json::to_string(&(&family, &output.output, &material, &values))
                    .map_err(|error| ModelError::new(format!("part identity: {error}")))?;
                match index.get(&key) {
                    Some(&row) => {
                        items[row].quantity += 1;
                        items[row].instances.push(output.instance.clone());
                    }
                    None => {
                        let family_variants = variants.entry(family.clone()).or_default();
                        if !family_variants.contains(&values) {
                            family_variants.push(values.clone());
                        }
                        index.insert(key, items.len());
                        item_values.push(values);
                        items.push(PartsListItem {
                            item: items.len() + 1,
                            quantity: 1,
                            part: String::new(),
                            family,
                            output: output.output.clone(),
                            material,
                            instances: vec![output.instance.clone()],
                        });
                    }
                }
            }
        }
        for (item, values) in items.iter_mut().zip(&item_values) {
            let family_variants = &variants[&item.family];
            let base = part_numbers
                .and_then(|numbers| numbers.get(&item.family))
                .cloned()
                .unwrap_or_else(|| item.family.clone());
            item.part = if family_variants.len() > 1 {
                let variant = family_variants
                    .iter()
                    .position(|candidate| candidate == values)
                    .expect("variant recorded")
                    + 1;
                format!("{base} / variant {variant}")
            } else {
                base
            };
        }
        Ok(items)
    }
}

/// Validates balloons and the table's placement. O(balloons + shown outputs).
pub(super) fn validate(
    drawing: &DrawingDefinition,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    if let Some(list) = &drawing.parts_list {
        if !finite_pair(list.position_mm) {
            return Err(ModelError::new("parts list position must be finite"));
        }
        for (family, number) in &list.part_numbers {
            if number.trim().is_empty() || number.len() > 40 || family.is_empty() {
                return Err(ModelError::new(
                    "part numbers must be 1-40 characters for a named family",
                ));
            }
        }
    }
    // Each view's shown instances, indexed once rather than per balloon.
    let shown: HashMap<&str, HashSet<&str>> = drawing
        .views
        .iter()
        .map(|view| {
            (
                view.id.as_str(),
                view.outputs.iter().map(|o| o.instance.as_str()).collect(),
            )
        })
        .collect();
    let mut ids = HashSet::new();
    for balloon in &drawing.balloons {
        if balloon.id.is_empty() || !ids.insert(balloon.id.as_str()) {
            return Err(ModelError::new("balloons need unique nonempty IDs"));
        }
        let view = views
            .get(balloon.view.as_str())
            .ok_or_else(|| ModelError::new("balloon references an unknown view"))?;
        if !shown[balloon.view.as_str()].contains(balloon.instance.as_str())
            || graph.is_suppressed(&balloon.instance)
        {
            return Err(ModelError::new(
                "balloon instance must be an unsuppressed instance shown in its view",
            ));
        }
        if let Some(anchor) = &balloon.anchor
            && anchor.instance != balloon.instance
        {
            return Err(ModelError::new(
                "balloon anchor must be on its own instance",
            ));
        }
        let length = balloon.offset_mm[0].hypot(balloon.offset_mm[1]);
        if !finite_pair(balloon.offset_mm) || length < BALLOON_RADIUS_MM + 4.0 {
            return Err(ModelError::new(
                "balloon offset must be finite and at least 8 mm long",
            ));
        }
        leader_point(balloon, view, graph)?;
    }
    Ok(())
}

/// Where a balloon's leader touches its instance, in paper millimeters.
fn leader_point(
    balloon: &DrawingBalloon,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
) -> Result<[f64; 2], ModelError> {
    let point = match &balloon.anchor {
        Some(anchor) => datum_origin(graph, anchor)?,
        None => {
            let resolved = graph.resolve_with_placement(&balloon.instance)?;
            let mut world = rigid(resolved.placement)?;
            for frame in resolved.frames {
                world = compose(&rigid(frame)?, &world);
            }
            Vec3::new(world[3], world[7], world[11])
        }
    };
    view.paper(view.project(point)?)
}

/// The table and balloons as laid out for `items`, with their exact line
/// vertex count for the drawing's budget.
pub(super) struct Layout {
    items: Vec<PartsListItem>,
}

impl Layout {
    pub(super) fn new(
        drawing: &DrawingDefinition,
        graph: &InstanceGraph<'_>,
    ) -> Result<Self, ModelError> {
        let items = if drawing.parts_list.is_some() || !drawing.balloons.is_empty() {
            drawing.parts_list_items(graph)?
        } else {
            Vec::new()
        };
        Ok(Self { items })
    }

    /// Outer rectangle 5, header and row separators 2 each, column
    /// separators 2 each; each balloon: leader 2, circle 33, end dot 9.
    pub(super) fn vertex_count(&self, drawing: &DrawingDefinition) -> usize {
        let table = if drawing.parts_list.is_some() {
            5 + 2 * self.items.len() + 2 * (COLUMNS_MM.len() - 1)
        } else {
            0
        };
        table + drawing.balloons.len() * 44
    }

    pub(super) fn append(
        &self,
        drawing: &DrawingDefinition,
        views: &HashMap<&str, &DrawingView>,
        graph: &InstanceGraph<'_>,
        out: &mut GeneratedDrawing,
    ) -> Result<(), ModelError> {
        if let Some(list) = &drawing.parts_list {
            // Rows depend on the model, so the fit is checked when drawing.
            let [x, top] = list.position_mm;
            let [width, height] = drawing.effective_paper_size_mm();
            let bottom = top - ROW_MM * (self.items.len() + 1) as f64;
            if x < 0.0 || bottom < 0.0 || x + COLUMNS_MM.iter().sum::<f64>() > width || top > height
            {
                return Err(ModelError::new(format!(
                    "parts list of {} items must fit on the paper",
                    self.items.len()
                )));
            }
            self.append_table(list, out);
        }
        let item_of: HashMap<&str, usize> = self
            .items
            .iter()
            .flat_map(|item| item.instances.iter().map(move |i| (i.as_str(), item.item)))
            .collect();
        for balloon in &drawing.balloons {
            let view = views[balloon.view.as_str()];
            let point = leader_point(balloon, view, graph)?;
            let center = [
                point[0] + balloon.offset_mm[0],
                point[1] + balloon.offset_mm[1],
            ];
            let number = item_of
                .get(balloon.instance.as_str())
                .ok_or_else(|| ModelError::new("balloon instance has no parts-list item"))?;
            // The leader stops at the balloon's rim and ends in a dot on the part.
            let length = balloon.offset_mm[0].hypot(balloon.offset_mm[1]);
            let rim = [
                center[0] - balloon.offset_mm[0] / length * BALLOON_RADIUS_MM,
                center[1] - balloon.offset_mm[1] / length * BALLOON_RADIUS_MM,
            ];
            line(out, vec![point, rim]);
            circle(out, center, BALLOON_RADIUS_MM, 32);
            circle(out, point, 0.5, 8);
            let text = number.to_string();
            out.gdt_labels.push(DrawingLabel {
                position_mm: [center[0] - 1.0 * text.len() as f64, center[1] - 1.2],
                text,
                stack: None,
            });
        }
        Ok(())
    }

    fn append_table(&self, list: &DrawingPartsList, out: &mut GeneratedDrawing) {
        let [x, top] = list.position_mm;
        let width: f64 = COLUMNS_MM.iter().sum();
        let rows = self.items.len() + 1;
        let bottom = top - ROW_MM * rows as f64;
        out.sheet_lines.push(DrawingPolyline {
            points_mm: vec![
                [x, top],
                [x + width, top],
                [x + width, bottom],
                [x, bottom],
                [x, top],
            ],
            hidden: false,
        });
        for row in 1..rows {
            let y = top - ROW_MM * row as f64;
            out.sheet_lines.push(DrawingPolyline {
                points_mm: vec![[x, y], [x + width, y]],
                hidden: false,
            });
        }
        let mut column_x = x;
        for width in &COLUMNS_MM[..COLUMNS_MM.len() - 1] {
            column_x += width;
            out.sheet_lines.push(DrawingPolyline {
                points_mm: vec![[column_x, top], [column_x, bottom]],
                hidden: false,
            });
        }
        let cells =
            std::iter::once(HEADERS.map(str::to_owned)).chain(self.items.iter().map(|item| {
                [
                    item.item.to_string(),
                    item.quantity.to_string(),
                    item.part.clone(),
                    item.material.clone().unwrap_or_else(|| "—".into()),
                ]
            }));
        for (row, values) in cells.enumerate() {
            let mut column_x = x;
            for (value, width) in values.into_iter().zip(COLUMNS_MM) {
                out.sheet_labels.push(DrawingLabel {
                    position_mm: [column_x + 1.5, top - ROW_MM * (row as f64 + 1.0) + 2.0],
                    text: value,
                    stack: None,
                });
                column_x += width;
            }
        }
    }
}

fn line(out: &mut GeneratedDrawing, points_mm: Vec<[f64; 2]>) {
    out.gdt_lines.push(DrawingPolyline {
        points_mm,
        hidden: false,
    });
}

fn circle(out: &mut GeneratedDrawing, center: [f64; 2], radius: f64, segments: usize) {
    line(
        out,
        (0..=segments)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / segments as f64;
                [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
            })
            .collect(),
    );
}
