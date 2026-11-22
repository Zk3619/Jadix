use jadix_core::window::{WindowAttributes, WindowMode, WindowPosition, WindowSize, WindowState};
use serde_json::json;

#[test]
fn presentation_deltas_preserve_false_and_omitted_attributes() {
    let mut applied = WindowAttributes {
        mode: Some(WindowMode::Normal),
        visible: Some(true),
        title: Some("Editor".into()),
        ..Default::default()
    };
    let desired: WindowAttributes = serde_json::from_value(json!({
        "mode": "full_screen", "visible": false
    }))
    .unwrap();
    let delta = applied.diff(&desired).unwrap();
    assert_eq!(delta.mode, Some(WindowMode::FullScreen));
    assert_eq!(delta.visible, Some(false));
    assert_eq!(delta.title, None);
    applied.merge(delta);
    assert_eq!(applied.title.as_deref(), Some("Editor"));
    assert!(applied.diff(&desired).is_none());
    assert!(applied.diff(&WindowAttributes::default()).is_none());
    let round_trip: WindowAttributes =
        serde_json::from_value(serde_json::to_value(&applied).unwrap()).unwrap();
    assert_eq!(round_trip, applied);
}

#[test]
fn validates_partial_size_updates_against_acknowledged_limits() {
    let applied = WindowAttributes {
        min_content_size: Some(WindowSize {
            width: 200,
            height: 100,
        }),
        max_content_size: Some(WindowSize {
            width: 800,
            height: 600,
        }),
        ..Default::default()
    };
    let update = WindowAttributes {
        max_content_size: Some(WindowSize {
            width: 199,
            height: 600,
        }),
        ..Default::default()
    };
    assert!(update.validate().is_ok());
    assert!(applied.validate_update(&update).is_err());
    // A zero minimum removes a lower bound; zero content/max sizes are invalid.
    for field in ["content_size", "max_content_size"] {
        let invalid: WindowAttributes = serde_json::from_value(json!({
            field: {"width": 0, "height": 100}
        }))
        .unwrap();
        assert!(invalid.validate().is_err());
    }
    assert!(WindowAttributes {
        min_content_size: Some(WindowSize {
            width: 0,
            height: 0
        }),
        ..Default::default()
    }
    .validate()
    .is_ok());
    assert!(WindowAttributes {
        position: Some(WindowPosition {
            x: f64::NAN,
            y: 0.0
        }),
        ..Default::default()
    }
    .validate()
    .is_err());
}

#[test]
fn observed_state_round_trips_and_rejects_invalid_host_geometry() {
    let mut state: WindowState = serde_json::from_value(json!({
        "id": "editor", "frame": {"x": -1920, "y": 30, "width": 800, "height": 600},
        "content_width": 800, "content_height": 578, "backing_scale": 2,
        "display": null, "mode": "normal", "key": true, "visible": true, "occluded": false
    }))
    .unwrap();
    assert!(state.validate().is_ok());
    assert_eq!(state.normal_frame, None);
    assert_eq!(state.title_bar_insets.top, 0.0);
    assert_eq!(
        state,
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap()
    );
    state.frame.width = -1.0;
    assert!(state.validate().is_err());
    state.frame.width = 800.0;
    state.backing_scale = 0.0;
    assert!(state.validate().is_err());
}
