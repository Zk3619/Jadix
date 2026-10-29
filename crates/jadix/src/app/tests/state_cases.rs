#[cfg(test)]
#[test]
fn page_stack_text_cache_keeps_recent_hosts_on_reentry() {
    let mut keys: Vec<u64> = (1..=64).collect();
    let mut renderers: Vec<u64> = keys.iter().map(|k| k * 100).collect();
    keys.push(65);
    renderers.push(6500);
    promote_page_stack_text_cache_slot(&mut keys, &mut renderers, 0, 64);
    keys.truncate(64);
    renderers.truncate(64);
    assert_eq!(
        &keys[..3],
        &[65, 1, 2],
        "the host just left must survive a full cache"
    );
    assert!(!keys.contains(&64), "the least recent host should leave");
    promote_page_stack_text_cache_slot(&mut keys, &mut renderers, 0, 1);
    assert_eq!(&keys[..3], &[1, 65, 2]);
    assert_eq!(
        &renderers[..3],
        &[100, 6500, 200],
        "reentry must retain the same renderer"
    );
    // A second live stacking host must not displace the first host or its history.
    keys.push(66);
    renderers.push(6600);
    promote_page_stack_text_cache_slot(&mut keys, &mut renderers, 1, 64);
    keys.truncate(64);
    renderers.truncate(64);
    assert_eq!(&keys[..4], &[1, 66, 65, 2]);
    assert_eq!(&renderers[..4], &[100, 6600, 6500, 200]);
}
