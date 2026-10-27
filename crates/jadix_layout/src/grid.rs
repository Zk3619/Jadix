//! Grid 模板辅助：等分列、多 `fr` 轨等（布局计算仍由 Taffy 完成）。

use taffy::prelude::*;
use taffy::style::GridTemplateComponent;

/// Taffy 0.13 templates are generic over named-line idents; Jadix does not use names.
pub type GridTrack = GridTemplateComponent<String>;

/// Tailwind `grid-cols-{n}` 风格：N 列等分 `minmax(0, 1fr)`。
#[inline]
pub fn cols_equal_fr(n: u16) -> Vec<GridTrack> {
    evenly_sized_tracks(n)
}

/// 多列（或行）按给定 `fr` 权重分配。
#[inline]
pub fn tracks_fr(weights: &[f32]) -> Vec<GridTrack> {
    weights
        .iter()
        .copied()
        .map(flex::<f32, GridTrack>)
        .collect()
}

/// `minmax(0, Nfr)` — Taffy `flex(n)`；比裸 `Nfr`（`minmax(auto, Nfr)`）更利于 `0fr` 收起。
#[inline]
pub fn track_fr(n: f32) -> GridTrack {
    flex::<f32, GridTrack>(n.max(0.0))
}

/// 单行或单列：`1fr` 占满剩余空间。
#[inline]
pub fn track_fr_1() -> GridTrack {
    track_fr(1.0)
}

/// First template track's `fr` max, if it is a single `fr` / `minmax(_, Nfr)` track.
pub fn first_track_fr(tracks: &[GridTrack]) -> Option<f32> {
    match tracks.first()? {
        GridTemplateComponent::Single(sizing) => {
            let max = sizing.max_sizing_function();
            if max.is_fr() {
                Some(max.into_raw().value().max(0.0))
            } else {
                None
            }
        }
        GridTemplateComponent::Repeat(_) => None,
    }
}

/// Replace (or create) the first track as `minmax(0, Nfr)`. Later tracks are left intact.
pub fn set_first_track_fr(tracks: &mut Vec<GridTrack>, fr: f32) {
    let track = track_fr(fr);
    if tracks.is_empty() {
        tracks.push(track);
    } else {
        tracks[0] = track;
    }
}

/// Every template track's `fr` max, if **all** tracks are single `fr` / `minmax(_, Nfr)`.
///
/// CSS interpolates `grid-template-rows` / `columns` only when the lists are the same
/// length and each item is interpolable. Mixed `px`/`auto`/`repeat()` lists stay discrete.
pub fn tracks_fr_list(tracks: &[GridTrack]) -> Option<Vec<f32>> {
    if tracks.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(tracks.len());
    for t in tracks {
        match t {
            GridTemplateComponent::Single(sizing) if sizing.max_sizing_function().is_fr() => {
                out.push(sizing.max_sizing_function().into_raw().value().max(0.0));
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Replace the whole template with `minmax(0, Nfr)` tracks (equal-length list lerp).
pub fn set_tracks_fr_list(tracks: &mut Vec<GridTrack>, frs: &[f32]) {
    *tracks = frs.iter().copied().map(|n| track_fr(n.max(0.0))).collect();
}

/// Pack up to 8 `fr` values for the transition `[f32; 8]` slot.
pub fn pack_tracks_fr(tracks: &[GridTrack]) -> ([f32; 8], u8) {
    match tracks_fr_list(tracks) {
        Some(list) if !list.is_empty() => {
            let n = list.len().min(8);
            let mut buf = [0.0; 8];
            buf[..n].copy_from_slice(&list[..n]);
            (buf, n as u8)
        }
        _ => ([0.0; 8], 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_track_fr_roundtrips_flex_tracks() {
        let mut rows = vec![track_fr(0.0)];
        assert_eq!(first_track_fr(&rows), Some(0.0));
        set_first_track_fr(&mut rows, 1.0);
        assert!((first_track_fr(&rows).unwrap() - 1.0).abs() < 1e-6);
        set_first_track_fr(&mut rows, 0.5);
        assert!((first_track_fr(&rows).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn tracks_fr_list_rejects_mixed_and_lerps_equal_length() {
        let two = vec![track_fr(0.0), track_fr(1.0)];
        assert_eq!(tracks_fr_list(&two), Some(vec![0.0, 1.0]));
        let mut rows = two.clone();
        set_tracks_fr_list(&mut rows, &[0.25, 0.75]);
        assert_eq!(tracks_fr_list(&rows), Some(vec![0.25, 0.75]));
        let mixed = vec![track_fr(1.0), GridTemplateComponent::Single(length(40.0))];
        assert!(tracks_fr_list(&mixed).is_none());
    }
}
