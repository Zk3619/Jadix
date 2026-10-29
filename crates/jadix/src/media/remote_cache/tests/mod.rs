use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Barrier, Mutex,
};

use super::*;

const URL: &str = "https://assets.test/shared.png";

#[test]
fn concurrent_consumers_share_one_load_and_the_same_allocation() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let cache = CachedRemoteAssetFetcher::new(
        move |_url: &str| {
            observed.fetch_add(1, Ordering::AcqRel);
            Ok(vec![1, 2, 3])
        },
        NonZeroU32::new(1024).unwrap(),
        Duration::from_secs(60),
    );
    let barrier = Barrier::new(8);
    let values = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let cache = &cache;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    cache.fetch_shared(URL).unwrap()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(calls.load(Ordering::Acquire), 1);
    for value in &values {
        assert!(Arc::ptr_eq(&values[0], value));
    }
    assert!(Arc::ptr_eq(
        &values[0],
        &cache.clone().fetch_shared(URL).unwrap()
    ));
    assert_eq!(cache.fetch(URL).unwrap(), vec![1, 2, 3]);
}

#[test]
fn clearing_during_a_load_cannot_reinsert_old_bytes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (started_send, started_receive) = mpsc::channel();
    let (release_send, release_receive) = mpsc::channel();
    let release_receive = Mutex::new(release_receive);
    let cache = CachedRemoteAssetFetcher::new(
        move |_url: &str| {
            let call = observed.fetch_add(1, Ordering::AcqRel) + 1;
            if call == 1 {
                started_send.send(()).unwrap();
                release_receive
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
            Ok(vec![call as u8])
        },
        NonZeroU32::new(1024).unwrap(),
        Duration::from_secs(60),
    );
    let worker_cache = cache.clone();
    let worker = std::thread::spawn(move || worker_cache.fetch_shared(URL).unwrap());
    started_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    cache.clone().clear();
    let fresh = cache.fetch_shared(URL).unwrap();
    release_send.send(()).unwrap();
    assert_eq!(&*worker.join().unwrap(), &[1]);
    assert_eq!(&*fresh, &[2]);
    assert!(Arc::ptr_eq(&fresh, &cache.fetch_shared(URL).unwrap()));
    assert_eq!(calls.load(Ordering::Acquire), 2);
}

#[test]
fn a_slow_url_does_not_block_other_keys_or_duplicate_warm_reads() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (started_send, started_receive) = mpsc::channel();
    let (release_send, release_receive) = mpsc::channel();
    let release_receive = Mutex::new(release_receive);
    let cache = CachedRemoteAssetFetcher::new(
        move |url: &str| {
            observed.fetch_add(1, Ordering::AcqRel);
            if url == URL {
                started_send.send(()).unwrap();
                release_receive
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
            Ok(vec![1, 2, 3])
        },
        NonZeroU32::new(1024).unwrap(),
        Duration::from_secs(60),
    );
    let slow_cache = cache.clone();
    let slow = std::thread::spawn(move || slow_cache.fetch_shared(URL).unwrap());
    started_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let fast_cache = cache.clone();
    let (ready_send, ready_receive) = mpsc::channel();
    let fast = std::thread::spawn(move || {
        let bytes = fast_cache
            .fetch_shared("https://assets.test/fast.png")
            .unwrap();
        let warm = fast_cache
            .fetch_shared("  https://assets.test/fast.png  ")
            .unwrap();
        ready_send.send(Arc::ptr_eq(&bytes, &warm)).unwrap();
    });
    let independent = ready_receive.recv_timeout(Duration::from_secs(5));
    release_send.send(()).unwrap();
    slow.join().unwrap();
    fast.join().unwrap();
    assert!(independent.unwrap());
    assert_eq!(calls.load(Ordering::Acquire), 2);
}

#[test]
fn failures_and_oversized_responses_are_not_retained() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let cache = CachedRemoteAssetFetcher::new(
        move |_url: &str| match observed.fetch_add(1, Ordering::AcqRel) {
            0 => Err("temporary backend failure".into()),
            1 => Ok(vec![0; 5]),
            _ => Ok(vec![0; 4]),
        },
        NonZeroU32::new(4).unwrap(),
        Duration::from_secs(60),
    );
    assert_eq!(
        cache.fetch_shared(URL).unwrap_err(),
        "temporary backend failure"
    );
    assert!(cache.fetch_shared(URL).is_err());
    assert_eq!(cache.fetch_shared(URL).unwrap().len(), 4);
    assert_eq!(cache.fetch_shared(URL).unwrap().len(), 4);
    assert_eq!(calls.load(Ordering::Acquire), 3);
    assert!(cache.fetch_shared("file:///private").is_err());
    assert_eq!(calls.load(Ordering::Acquire), 3);
}

#[test]
fn ttl_and_session_instances_do_not_serve_stale_or_other_session_bytes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let cache = CachedRemoteAssetFetcher::new(
        move |_url: &str| Ok(vec![observed.fetch_add(1, Ordering::AcqRel) as u8]),
        NonZeroU32::new(1024).unwrap(),
        Duration::from_millis(10),
    );
    let previous = cache.fetch_shared(URL).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(&*cache.fetch_shared(URL).unwrap(), &[1]);
    assert_eq!(&*previous, &[0]);
    let other = CachedRemoteAssetFetcher::new(
        |_url: &str| Ok(vec![9]),
        NonZeroU32::new(1024).unwrap(),
        Duration::from_secs(60),
    );
    assert_eq!(&*other.fetch_shared(URL).unwrap(), &[9]);
}

#[test]
fn eviction_weighs_bytes_and_charges_empty_entries() {
    let cache = CachedRemoteAssetFetcher::new(
        |url: &str| {
            Ok(if url.ends_with("empty") {
                vec![]
            } else {
                vec![0; 4]
            })
        },
        NonZeroU32::new(8).unwrap(),
        Duration::from_secs(60),
    );
    for index in 0..20 {
        cache
            .fetch_shared(&format!("https://assets.test/{index}"))
            .unwrap();
        cache
            .fetch_shared(&format!("https://assets.test/{index}/empty"))
            .unwrap();
    }
    let inner = cache.cache.read().unwrap().clone();
    inner.run_pending_tasks();
    assert!(inner.weighted_size() <= 8);
    assert!(inner.entry_count() <= 8);
}
