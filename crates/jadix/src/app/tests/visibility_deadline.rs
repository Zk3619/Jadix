use super::retain_future_instants;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[test]
fn expired_deadlines_are_removed_once() {
    let now = Instant::now();
    let mut map = HashMap::new();
    map.insert("past".into(), now - Duration::from_millis(1));
    map.insert("future".into(), now + Duration::from_secs(1));
    assert!(retain_future_instants(&mut map, now));
    assert_eq!(map.len(), 1);
    assert!(map.contains_key("future"));
    assert!(!retain_future_instants(&mut map, now));
    assert_eq!(map.len(), 1);
}
