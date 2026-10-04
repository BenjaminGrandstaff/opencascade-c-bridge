//! Native viewer export: the exact B-rep parts of a generated graph and a
//! script that opens them, named and colored, in OCCT's DRAW viewer.

use super::*;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A DRAW variable name: letters, digits, and underscores, not starting with a
/// digit, made unique with a numeric suffix.
fn draw_name(instance: &str, used: &mut HashSet<String>) -> String {
    let mut name = instance
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert_str(0, "part_");
    }
    let mut unique = name.clone();
    let mut suffix = 2;
    while !used.insert(unique.clone()) {
        unique = format!("{name}_{suffix}");
        suffix += 1;
    }
    unique
}

/// `#RRGGBB` in sRGB from a linear appearance color.
fn hex(appearance: &MaterialAppearance) -> String {
    let channel = |linear: f64| {
        let srgb = if linear <= 0.003_130_8 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let [r, g, b, _] = appearance.base_color;
    format!("#{:02X}{:02X}{:02X}", channel(r), channel(g), channel(b))
}

impl InstanceGraph<'_> {
    /// Writes `model.brep` (the generated `outputs` as one compound, placed)
    /// and `view.tcl` into `directory`, creating it if needed. Run
    /// `DRAWEXE -i -f view.tcl` there to open every part shaded, named after
    /// its instance, and colored from its material appearance; the script
    /// prints which DRAW name belongs to which instance. Returns the script
    /// path. O(outputs) plus one BREP write.
    pub fn export_draw_view(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        directory: impl AsRef<Path>,
        outputs: &OutputSet,
    ) -> Result<PathBuf, ModelError> {
        let outputs = self.generated_outputs(generation, outputs)?;
        let mut shapes = Vec::with_capacity(outputs.len());
        for output in &outputs {
            shapes.push(
                generation
                    .result(&output.instance)
                    .and_then(|result| result.shape(&output.output))
                    .ok_or_else(|| {
                        ModelError::new(format!(
                            "missing generated output '{}:{}'",
                            output.instance, output.output
                        ))
                    })?,
            );
        }
        let directory = directory.as_ref();
        std::fs::create_dir_all(directory)
            .map_err(|error| ModelError::new(format!("cannot create view directory: {error}")))?;
        let compound = session.create_compound(&shapes)?;
        let saved = session.save_brep(&compound, directory.join("model.brep"));
        session.remove(compound)?;
        saved?;

        let mut used = HashSet::new();
        let mut script = String::from(
            "# Opens model.brep in OCCT's DRAW viewer: DRAWEXE -i -f view.tcl\n\
             # (-i is required: with -f alone DRAW draws off screen and exits).\n\
             pload MODELING VISUALIZATION\n\
             set here [file dirname [file normalize [info script]]]\n\
             restore [file join $here model.brep] model\n\
             explode model\n\
             vinit name=Model w=1280 h=800\n",
        );
        let mut legend = String::new();
        for (index, output) in outputs.iter().enumerate() {
            let name = draw_name(&output.instance, &mut used);
            let _ = writeln!(script, "copy model_{} {name}", index + 1);
            let _ = writeln!(script, "vdisplay -dispMode 1 {name}");
            let appearance = self
                .material_of(&output.instance)?
                .and_then(|material| self.assembly.material_appearances.get(&material.id));
            if let Some(appearance) = appearance {
                let _ = writeln!(script, "vsetcolor {name} {}", hex(appearance));
            }
            let _ = writeln!(legend, "{name} = {}:{}", output.instance, output.output);
        }
        script.push_str("vaxo\nvfit\n");
        for line in legend.lines() {
            // Braces keep instance ids literal in Tcl; ids lose their own braces.
            let line = line.replace('{', "(").replace('}', ")");
            let _ = writeln!(script, "puts {{{line}}}");
        }
        let path = directory.join("view.tcl");
        std::fs::write(&path, script)
            .map_err(|error| ModelError::new(format!("cannot write view script: {error}")))?;
        Ok(path)
    }
}

#[cfg(test)]
pub(crate) fn draw_name_for_tests(instance: &str, used: &mut HashSet<String>) -> String {
    draw_name(instance, used)
}
