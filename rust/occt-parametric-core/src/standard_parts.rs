//! Frozen standard-part catalog `occb_standard_parts_v1` (schema 180):
//! metric socket head cap screws (ISO 4762), hex nuts (ISO 4032 style 1),
//! plain washers (ISO 7089) and dowel pins (ISO 8734), modeled at their
//! maximum (nominal) dimensions.
//!
//! Source: the dimension tables of fasten.it, read 2026-10-20:
//! <https://fasten.it/norms/norm/din_en_iso_4762> (DIN EN ISO 4762:2004),
//! `din_en_iso_4032` (DIN EN ISO 4032:2013), `din_en_iso_7089`
//! (DIN EN ISO 7089:2000) and `din_en_iso_8734` (DIN EN ISO 8734:1998).
//! Sizes M3 to M20 are kept. Two entries in those tables are wrong and are not
//! used: the M2.5 screw's k min (2.66, above its k max of 2.5) and the M24
//! nut's m max (19, below M22's 19.4); the table tests check that each
//! dimension grows with size so such slips are caught.
use crate::error::ModelError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum StandardPartKind {
    /// Hexagon socket head cap screw; needs a length under the head.
    #[serde(rename = "socket_head_cap_screw_iso_4762")]
    SocketHeadCapScrewIso4762,
    /// Hexagon nut, style 1.
    #[serde(rename = "hex_nut_iso_4032")]
    HexNutIso4032,
    /// Plain washer, normal series, product grade A.
    #[serde(rename = "plain_washer_iso_7089")]
    PlainWasherIso7089,
    /// Hardened parallel dowel pin; needs a length.
    #[serde(rename = "dowel_pin_iso_8734")]
    DowelPinIso8734,
}

/// A catalog dimension, in millimeters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandardPartDimension {
    /// Screw head diameter dk max (plain head).
    HeadDiameter,
    /// Screw head height k max.
    HeadHeight,
    /// Screw socket across flats s.
    SocketAcrossFlats,
    /// Screw socket depth t min.
    SocketDepth,
    /// Screw reference thread length b.
    ThreadLength,
    /// Nut across flats s.
    AcrossFlats,
    /// Nut across corners e min.
    AcrossCorners,
    /// Nut height m max, or washer thickness.
    Height,
    /// Washer hole diameter.
    InnerDiameter,
    /// Washer outside diameter.
    OuterDiameter,
    /// Dowel pin end chamfer c.
    Chamfer,
}

/// ISO 4762 row: nominal d, dk max, k max, s, t min, b.
const SCREWS: [[f64; 6]; 10] = [
    [3.0, 5.5, 3.0, 2.5, 1.3, 18.0],
    [4.0, 7.0, 4.0, 3.0, 2.0, 20.0],
    [5.0, 8.5, 5.0, 4.0, 2.5, 22.0],
    [6.0, 10.0, 6.0, 5.0, 3.0, 24.0],
    [8.0, 13.0, 8.0, 6.0, 4.0, 28.0],
    [10.0, 16.0, 10.0, 8.0, 5.0, 32.0],
    [12.0, 18.0, 12.0, 10.0, 6.0, 36.0],
    [14.0, 21.0, 14.0, 12.0, 7.0, 40.0],
    [16.0, 24.0, 16.0, 14.0, 8.0, 44.0],
    [20.0, 30.0, 20.0, 17.0, 10.0, 52.0],
];
/// ISO 4032 row: nominal D, m max, s, e min.
const NUTS: [[f64; 4]; 10] = [
    [3.0, 2.4, 5.5, 6.01],
    [4.0, 3.2, 7.0, 7.66],
    [5.0, 4.7, 8.0, 8.79],
    [6.0, 5.0, 10.0, 11.05],
    [8.0, 6.8, 13.0, 14.38],
    [10.0, 8.4, 16.0, 17.77],
    [12.0, 10.8, 18.0, 20.03],
    [14.0, 12.8, 21.0, 23.36],
    [16.0, 14.8, 24.0, 26.75],
    [20.0, 18.0, 30.0, 32.95],
];
/// ISO 7089 row: thread size, hole d (nominal = min), outside (nominal =
/// max), thickness.
const WASHERS: [[f64; 4]; 10] = [
    [3.0, 3.2, 7.0, 0.5],
    [4.0, 4.3, 9.0, 0.8],
    [5.0, 5.3, 10.0, 1.0],
    [6.0, 6.4, 12.0, 1.6],
    [8.0, 8.4, 16.0, 1.6],
    [10.0, 10.5, 20.0, 2.0],
    [12.0, 13.0, 24.0, 2.5],
    [14.0, 15.0, 28.0, 2.5],
    [16.0, 17.0, 30.0, 3.0],
    [20.0, 21.0, 37.0, 3.0],
];
/// ISO 8734 row: nominal d, chamfer c.
const DOWELS: [[f64; 2]; 13] = [
    [1.0, 0.2],
    [1.5, 0.3],
    [2.0, 0.35],
    [2.5, 0.4],
    [3.0, 0.5],
    [4.0, 0.63],
    [5.0, 0.8],
    [6.0, 1.2],
    [8.0, 1.6],
    [10.0, 2.0],
    [12.0, 2.5],
    [16.0, 3.0],
    [20.0, 3.5],
];

/// A part's frozen dimensions at one nominal size, in millimeters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StandardPartData {
    Screw {
        diameter: f64,
        head_diameter: f64,
        head_height: f64,
        socket_across_flats: f64,
        socket_depth: f64,
        thread_length: f64,
    },
    Nut {
        diameter: f64,
        height: f64,
        across_flats: f64,
        across_corners: f64,
    },
    Washer {
        inner_diameter: f64,
        outer_diameter: f64,
        thickness: f64,
    },
    Dowel {
        diameter: f64,
        chamfer: f64,
    },
}

fn row<const N: usize>(table: &[[f64; N]], nominal: f64) -> Option<[f64; N]> {
    table
        .iter()
        .find(|row| (row[0] - nominal).abs() < 1e-9)
        .copied()
}

impl StandardPartKind {
    pub fn name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Nominal sizes in the catalog, in millimeters.
    pub fn sizes(self) -> Vec<f64> {
        match self {
            Self::SocketHeadCapScrewIso4762 => SCREWS.iter().map(|r| r[0]).collect(),
            Self::HexNutIso4032 => NUTS.iter().map(|r| r[0]).collect(),
            Self::PlainWasherIso7089 => WASHERS.iter().map(|r| r[0]).collect(),
            Self::DowelPinIso8734 => DOWELS.iter().map(|r| r[0]).collect(),
        }
    }

    /// Whether the part takes a length (screws under the head, pins overall).
    pub fn takes_length(self) -> bool {
        matches!(
            self,
            Self::SocketHeadCapScrewIso4762 | Self::DowelPinIso8734
        )
    }

    /// The part's dimensions at `nominal` millimeters, or an error listing
    /// the catalog sizes.
    pub fn data(self, nominal: f64) -> Result<StandardPartData, ModelError> {
        let found = match self {
            Self::SocketHeadCapScrewIso4762 => {
                row(&SCREWS, nominal).map(|r| StandardPartData::Screw {
                    diameter: r[0],
                    head_diameter: r[1],
                    head_height: r[2],
                    socket_across_flats: r[3],
                    socket_depth: r[4],
                    thread_length: r[5],
                })
            }
            Self::HexNutIso4032 => row(&NUTS, nominal).map(|r| StandardPartData::Nut {
                diameter: r[0],
                height: r[1],
                across_flats: r[2],
                across_corners: r[3],
            }),
            Self::PlainWasherIso7089 => row(&WASHERS, nominal).map(|r| StandardPartData::Washer {
                inner_diameter: r[1],
                outer_diameter: r[2],
                thickness: r[3],
            }),
            Self::DowelPinIso8734 => row(&DOWELS, nominal).map(|r| StandardPartData::Dowel {
                diameter: r[0],
                chamfer: r[1],
            }),
        };
        found.ok_or_else(|| {
            let sizes = self
                .sizes()
                .iter()
                .map(|s| format!("{s}"))
                .collect::<Vec<_>>()
                .join(", ");
            ModelError::new(format!(
                "{} has no {nominal} mm size; catalog sizes are {sizes} mm",
                self.name()
            ))
        })
    }

    /// One dimension in millimeters, or an error when the part has none.
    pub fn dimension(
        self,
        nominal: f64,
        dimension: StandardPartDimension,
    ) -> Result<f64, ModelError> {
        use StandardPartDimension as D;
        let value = match (self.data(nominal)?, dimension) {
            (StandardPartData::Screw { head_diameter, .. }, D::HeadDiameter) => head_diameter,
            (StandardPartData::Screw { head_height, .. }, D::HeadHeight) => head_height,
            (
                StandardPartData::Screw {
                    socket_across_flats,
                    ..
                },
                D::SocketAcrossFlats,
            ) => socket_across_flats,
            (StandardPartData::Screw { socket_depth, .. }, D::SocketDepth) => socket_depth,
            (StandardPartData::Screw { thread_length, .. }, D::ThreadLength) => thread_length,
            (StandardPartData::Nut { across_flats, .. }, D::AcrossFlats) => across_flats,
            (StandardPartData::Nut { across_corners, .. }, D::AcrossCorners) => across_corners,
            (StandardPartData::Nut { height, .. }, D::Height) => height,
            (StandardPartData::Washer { thickness, .. }, D::Height) => thickness,
            (StandardPartData::Washer { inner_diameter, .. }, D::InnerDiameter) => inner_diameter,
            (StandardPartData::Washer { outer_diameter, .. }, D::OuterDiameter) => outer_diameter,
            (StandardPartData::Dowel { chamfer, .. }, D::Chamfer) => chamfer,
            _ => {
                return Err(ModelError::new(format!(
                    "{} has no {} dimension",
                    self.name(),
                    serde_json::to_value(dimension)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default()
                )));
            }
        };
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each column grows with size, which catches transcription slips like
    /// the source's M24 nut height.
    fn increasing<const N: usize>(table: &[[f64; N]]) {
        for pair in table.windows(2) {
            for column in 0..N {
                assert!(
                    pair[1][column] >= pair[0][column] && pair[1][column] > 0.0,
                    "{:?} -> {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn tables_are_ordered_and_consistent() {
        increasing(&SCREWS);
        increasing(&NUTS);
        increasing(&WASHERS);
        increasing(&DOWELS);
        for r in SCREWS {
            // Head wider than the shank, socket inside the head.
            assert!(r[1] > r[0] && r[3] < r[1] && r[4] < r[2]);
        }
        for r in NUTS {
            // Corners of a hexagon: e = s / cos 30° at most.
            assert!(r[3] > r[2] && r[3] <= r[2] / (30f64).to_radians().cos() + 1e-9);
        }
        for r in WASHERS {
            assert!(r[1] > r[0] && r[2] > r[1]);
        }
    }

    #[test]
    fn lookups_return_dimensions_or_name_the_sizes() {
        let screw = StandardPartKind::SocketHeadCapScrewIso4762;
        assert_eq!(
            screw
                .dimension(6.0, StandardPartDimension::HeadDiameter)
                .unwrap(),
            10.0
        );
        assert_eq!(
            StandardPartKind::HexNutIso4032
                .dimension(8.0, StandardPartDimension::AcrossCorners)
                .unwrap(),
            14.38
        );
        assert_eq!(
            StandardPartKind::PlainWasherIso7089
                .dimension(10.0, StandardPartDimension::Height)
                .unwrap(),
            2.0
        );
        let error = screw.data(7.0).unwrap_err().message;
        assert!(
            error.contains("no 7 mm size") && error.contains("3, 4, 5, 6, 8"),
            "{error}"
        );
        let error = screw
            .dimension(6.0, StandardPartDimension::Chamfer)
            .unwrap_err()
            .message;
        assert!(error.contains("has no chamfer dimension"), "{error}");
    }
}
