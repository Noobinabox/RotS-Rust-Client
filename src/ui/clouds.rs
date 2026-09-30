//! Seeded cloud silhouettes with independently drifting depth layers.
//!
//! The caller supplies the animation phase. Irregular spacing and different
//! drift rates produce changing overlaps without randomness or clocks during
//! rendering. The scene repeats with the weather phase cycle; these are visual
//! intervals, not independently scheduled wall-clock events.

use crate::animation::weather_playback::SCENE_PHASES;

const CLOUD: [&str; 3] = ["   .--.    ", " .(    ).  ", "(___.__)__)"];
const ROW_PERIOD: i64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CloudCell {
    pub symbol: &'static str,
    /// Larger values are closer to the viewer.
    pub layer: u8,
}

struct CloudLayer {
    spacing: i64,
    row_offset: i64,
    phase_per_step: i64,
    seed: u64,
}

const LAYERS: [CloudLayer; 3] = [
    CloudLayer {
        spacing: 37,
        row_offset: 0,
        phase_per_step: 16,
        seed: 0x9e37_79b9,
    },
    CloudLayer {
        spacing: 31,
        row_offset: 1,
        phase_per_step: 12,
        seed: 0x85eb_ca6b,
    },
    CloudLayer {
        spacing: 29,
        row_offset: 3,
        phase_per_step: 8,
        seed: 0xc2b2_ae35,
    },
];

/// Select the foremost opaque cloud cell, including spaces inside its outline.
/// Work is bounded by three layers regardless of position or viewport size.
pub(super) fn cloud_cover(x: i32, y: i32, phase: i32) -> Option<CloudCell> {
    let phase = i64::from(phase).rem_euclid(SCENE_PHASES as i64);
    LAYERS.iter().enumerate().rev().find_map(|(index, layer)| {
        layer_cover(layer, i64::from(x), i64::from(y), phase).map(|symbol| CloudCell {
            symbol,
            layer: index as u8,
        })
    })
}

fn layer_cover(layer: &CloudLayer, x: i64, y: i64, phase: i64) -> Option<&'static str> {
    let shifted_y = y - layer.row_offset;
    let row = shifted_y.rem_euclid(ROW_PERIOD) as usize;
    let shape = *CLOUD.get(row)?;
    let shifted_x = x - phase / layer.phase_per_step;
    let tile = shifted_x.div_euclid(layer.spacing);
    let band = shifted_y.div_euclid(ROW_PERIOD);
    let offset =
        seeded_offset(tile, band, layer.seed) % (layer.spacing as u64 - CLOUD[0].len() as u64 + 1);
    let column = shifted_x.rem_euclid(layer.spacing) - offset as i64;
    let column = usize::try_from(column).ok()?;
    let first = shape.find(|c| c != ' ')?;
    let last = shape.rfind(|c| c != ' ')?;
    if !(first..=last).contains(&column) {
        return None;
    }
    // Shapes are ASCII; retaining interior blanks makes silhouettes opaque.
    shape.get(column..column + 1)
}

fn seeded_offset(tile: i64, band: i64, seed: u64) -> u64 {
    let mut value = (tile as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (band as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ seed;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_is_deterministic_and_cycles() {
        for phase in 0..SCENE_PHASES as i32 {
            for x in -8..80 {
                assert_eq!(cloud_cover(x, 2, phase), cloud_cover(x, 2, phase));
                assert_eq!(
                    cloud_cover(x, 2, phase),
                    cloud_cover(x, 2, phase + SCENE_PHASES as i32)
                );
            }
        }
    }

    #[test]
    fn foreground_interior_spaces_occlude_back_layers() {
        let mut saw_opaque_overlap = false;
        for phase in 0..SCENE_PHASES as i64 {
            for y in 0..12 {
                for x in 0..100 {
                    let covers: Vec<_> = LAYERS
                        .iter()
                        .enumerate()
                        .filter_map(|(layer, spec)| {
                            layer_cover(spec, x, y, phase).map(|symbol| CloudCell {
                                layer: layer as u8,
                                symbol,
                            })
                        })
                        .collect();
                    if covers.len() > 1 && covers.last().unwrap().symbol == " " {
                        saw_opaque_overlap = true;
                        assert_eq!(
                            cloud_cover(x as i32, y as i32, phase as i32),
                            covers.last().copied()
                        );
                    }
                }
            }
        }
        assert!(saw_opaque_overlap);
    }

    #[test]
    fn overlapping_clouds_change_at_irregular_intervals() {
        let overlap_counts: Vec<_> = (0..SCENE_PHASES as i64)
            .map(|phase| {
                (0..12)
                    .flat_map(|y| (0..100).map(move |x| (x, y)))
                    .filter(|&(x, y)| {
                        LAYERS
                            .iter()
                            .filter(|layer| layer_cover(layer, x, y, phase).is_some())
                            .count()
                            > 1
                    })
                    .count()
            })
            .collect();
        assert!(overlap_counts.iter().any(|&count| count > 0));
        let changes: Vec<_> = overlap_counts
            .windows(2)
            .enumerate()
            .filter_map(|(phase, counts)| (counts[0] != counts[1]).then_some(phase + 1))
            .collect();
        let intervals: Vec<_> = changes.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(intervals.len() > 2);
        assert!(intervals.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn extreme_and_negative_coordinates_are_safe() {
        for x in [i32::MIN, -1, 0, 1, i32::MAX] {
            for y in [i32::MIN, -1, 0, 1, i32::MAX] {
                for phase in [i32::MIN, -1, 0, 1, i32::MAX] {
                    if let Some(cell) = cloud_cover(x, y, phase) {
                        assert!(cell.layer < LAYERS.len() as u8);
                        assert_eq!(cell.symbol.len(), 1);
                    }
                }
            }
        }
    }
}
