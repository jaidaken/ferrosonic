//! Property tests for `daemon::library::LruCache` invariants.

use ferrosonic::daemon::library::LruCache;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(String, i32),
    Get(String),
}

fn key_strategy() -> impl Strategy<Value = String> {
    (0u8..8).prop_map(|n| format!("k{}", n))
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (key_strategy(), any::<i32>()).prop_map(|(k, v)| Op::Insert(k, v)),
        key_strategy().prop_map(Op::Get),
    ]
}

#[test]
fn len_never_exceeds_cap_under_random_ops() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(1usize..=8, prop::collection::vec(op_strategy(), 0..64)),
            |(cap, ops)| {
                let mut cache: LruCache<i32> = LruCache::new();
                for op in ops {
                    match op {
                        Op::Insert(k, v) => cache.insert(k, v, cap),
                        Op::Get(k) => {
                            let _ = cache.get(&k);
                        }
                    }
                    prop_assert!(
                        cache.len() <= cap,
                        "len {} exceeded cap {}",
                        cache.len(),
                        cap
                    );
                }
                Ok(())
            },
        )
        .expect("LruCache cap invariant must hold");
}

#[test]
fn insert_then_get_returns_inserted_value() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(key_strategy(), any::<i32>(), 1usize..=8),
            |(k, v, cap)| {
                let mut cache: LruCache<i32> = LruCache::new();
                cache.insert(k.clone(), v, cap);
                prop_assert_eq!(cache.get(&k).copied(), Some(v));
                prop_assert_eq!(cache.len(), 1);
                Ok(())
            },
        )
        .expect("insert-then-get must return the inserted value");
}

#[test]
fn cap_two_first_inserted_evicted_when_third_arrives() {
    let mut runner = proptest::test_runner::TestRunner::default();
    let three_distinct = (0u8..8u8, 0u8..8u8, 0u8..8u8)
        .prop_filter("three distinct keys", |(a, b, c)| {
            a != b && b != c && a != c
        });
    runner
        .run(&three_distinct, |(a, b, c)| {
            let ka = format!("k{}", a);
            let kb = format!("k{}", b);
            let kc = format!("k{}", c);
            let mut cache: LruCache<i32> = LruCache::new();
            cache.insert(ka.clone(), 1, 2);
            cache.insert(kb.clone(), 2, 2);
            cache.insert(kc.clone(), 3, 2);
            prop_assert!(
                cache.get(&ka).is_none(),
                "oldest key {} should be evicted",
                ka
            );
            prop_assert_eq!(cache.get(&kb).copied(), Some(2));
            prop_assert_eq!(cache.get(&kc).copied(), Some(3));
            prop_assert_eq!(cache.len(), 2);
            Ok(())
        })
        .expect("LRU eviction order must drop the oldest untouched key");
}

#[test]
fn touched_key_survives_eviction_pressure() {
    let mut runner = proptest::test_runner::TestRunner::default();
    let three_distinct = (0u8..8u8, 0u8..8u8, 0u8..8u8)
        .prop_filter("three distinct keys", |(a, b, c)| {
            a != b && b != c && a != c
        });
    runner
        .run(&three_distinct, |(a, b, c)| {
            let ka = format!("k{}", a);
            let kb = format!("k{}", b);
            let kc = format!("k{}", c);
            let mut cache: LruCache<i32> = LruCache::new();
            cache.insert(ka.clone(), 1, 2);
            cache.insert(kb.clone(), 2, 2);
            let _ = cache.get(&ka);
            cache.insert(kc.clone(), 3, 2);
            prop_assert_eq!(
                cache.get(&ka).copied(),
                Some(1),
                "touched key {} should survive eviction",
                ka
            );
            prop_assert!(
                cache.get(&kb).is_none(),
                "untouched older key {} should be evicted",
                kb
            );
            prop_assert_eq!(cache.get(&kc).copied(), Some(3));
            Ok(())
        })
        .expect("touch via get() must promote to MRU and protect from eviction");
}
