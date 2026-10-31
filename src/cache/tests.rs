//! Cache corruption is recoverable; private-spool and deadline failures are not.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Temporary, Cache, Module) {
    let directory = std::env::temp_dir().join(format!(
        "bifoil-cache-input-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let module = crate::compiler::compile(
        crate::parser::parse(
            "export fun read() -> Int = 1\nfun other() -> Int = 2\n",
            "src/helpers.bfl",
        )
        .unwrap(),
        "helpers",
        "src/helpers.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    let cache = Cache {
        directory: directory.clone(),
        compiler: "test-compiler".into(),
        observed: BTreeMap::new(),
        compiled: 0,
        reused: 0,
    };
    (Temporary(directory), cache, module)
}
fn record(cache: &Cache, module: &Module) -> String {
    serde_json::to_string(&RecordRef {
        compiler: &cache.compiler,
        source: "source",
        module,
    })
    .unwrap()
}

#[test]
fn streamed_cache_input_preserves_order_independence_checksums_and_partial_failure_isolation() {
    let (_temporary, mut cache, module) = fixture();
    let path = cache.path("helpers");
    let text = format!(
        "{{\"module\":{},\"source\":\"source\",\"compiler\":\"test-compiler\"}}",
        serde_json::to_string_pretty(&module).unwrap()
    );
    fs::write(&path, &text).unwrap();
    let mut pending = crate::code_store::CodeStore::default();
    let loaded = cache
        .load("helpers", "source", &mut pending, Instant::now())
        .unwrap()
        .unwrap();
    assert_eq!(loaded.digest, module.digest);
    assert!(loaded.functions.values().all(|function| function.code.is_empty()));
    assert!(pending.instructions > 0);
    let checksum = crate::input::hash(&path, MAX_ARTIFACT, None, Instant::now()).unwrap();
    assert_eq!(cache.observed("helpers"), Some(checksum.as_str()));

    let mut corrupted = serde_json::from_str::<serde_json::Value>(&record(&cache, &module)).unwrap();
    // The earlier helpers.other entry has already been spooled when this fails.
    corrupted["module"]["functions"]["helpers.read"]["code"][0]["op"] = "unknown-op".into();
    let duplicate = record(&cache, &module).replacen("\"compiler\":", "\"compiler\":\"other\",\"compiler\":", 1);
    for bad in [serde_json::to_string(&corrupted).unwrap(), duplicate] {
        fs::write(&path, bad).unwrap();
        let mut pending = crate::code_store::CodeStore::default();
        assert!(
            cache
                .load("helpers", "source", &mut pending, Instant::now())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            pending.instructions, 0,
            "invalid private input must not change shared pending code"
        );
    }
}

#[test]
fn an_expired_incremental_deadline_is_not_a_disposable_cache_miss() {
    let (_temporary, mut cache, module) = fixture();
    let text = record(&cache, &module);
    fs::write(cache.path("helpers"), &text).unwrap();
    struct Deadline(bool);
    impl Drop for Deadline {
        fn drop(&mut self) {
            crate::INCREMENTAL_DEADLINE.set(self.0);
        }
    }
    let _reset = Deadline(crate::INCREMENTAL_DEADLINE.get());
    crate::INCREMENTAL_DEADLINE.set(true);
    for preloaded in [false, true] {
        let mut pending = crate::code_store::CodeStore::default();
        let started = Instant::now() - Duration::from_secs(20);
        let result = if preloaded {
            cache.load_bytes("helpers", "source", text.as_bytes(), &mut pending, started)
        } else {
            cache.load("helpers", "source", &mut pending, started)
        };
        let error = result.expect_err("deadline failures must stop compilation");
        assert!(error.contains("budget"), "{error}");
        assert_eq!(pending.instructions, 0);
    }
}

#[test]
fn preloaded_cache_bytes_keep_checksum_validation_and_failure_isolation() {
    let (_temporary, mut cache, module) = fixture();
    let text = record(&cache, &module);
    let mut pending = crate::code_store::CodeStore::default();
    let loaded = cache
        .load_bytes("helpers", "source", text.as_bytes(), &mut pending, Instant::now())
        .unwrap()
        .unwrap();
    assert_eq!(loaded.digest, module.digest);
    assert!(pending.instructions > 0);
    assert_eq!(cache.observed("helpers"), Some(Cache::source_hash(&text).as_str()));
    let mut corrupted = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    corrupted["module"]["functions"]["helpers.read"]["code"][0]["op"] = "unknown-op".into();
    for bad in [serde_json::to_vec(&corrupted).unwrap(), vec![255]] {
        let mut pending = crate::code_store::CodeStore::default();
        assert!(
            cache
                .load_bytes("helpers", "source", &bad, &mut pending, Instant::now())
                .unwrap()
                .is_none()
        );
        assert_eq!(pending.instructions, 0);
    }
}

#[test]
fn stale_identity_aborts_before_decoding_the_irrelevant_module_body() {
    use serde::de::DeserializeSeed;
    let (_temporary, mut cache, _module) = fixture();
    for (compiler, source, diagnostic) in [
        ("old-compiler", "source", "cache compiler identity changed"),
        ("test-compiler", "old-source", "cache source identity changed"),
    ] {
        // Even JSON parsing of the irrelevant body must not be reached.
        let text = format!("{{\"compiler\":\"{compiler}\",\"source\":\"{source}\",\"module\":invalid JSON");
        let seed = crate::module_input::streamed::Seed::new(Instant::now());
        let error = input::RecordSeed::new(seed.clone(), "test-compiler", "source")
            .deserialize(&mut serde_json::Deserializer::from_str(&text))
            .err()
            .unwrap();
        assert!(error.to_string().contains(diagnostic), "{error}");
        assert!(seed.take_failure().is_none());
        fs::write(cache.path("helpers"), &text).unwrap();
        for preloaded in [false, true] {
            let mut pending = crate::code_store::CodeStore::default();
            let result = if preloaded {
                cache.load_bytes("helpers", "source", text.as_bytes(), &mut pending, Instant::now())
            } else {
                cache.load("helpers", "source", &mut pending, Instant::now())
            };
            assert!(result.unwrap().is_none());
            assert_eq!(pending.instructions, 0);
            assert!(cache.observed("helpers").is_none());
        }
    }
}

#[test]
fn identity_checks_after_module_fields_drop_the_private_body_without_publishing_it() {
    use serde::de::DeserializeSeed;
    let (_temporary, mut cache, module) = fixture();
    let text = format!(
        "{{\"module\":{},\"source\":\"old-source\",\"compiler\":\"test-compiler\"}}",
        serde_json::to_string(&module).unwrap()
    );
    let seed = crate::module_input::streamed::Seed::new(Instant::now());
    let error = input::RecordSeed::new(seed.clone(), "test-compiler", "source")
        .deserialize(&mut serde_json::Deserializer::from_str(&text))
        .err()
        .unwrap();
    assert!(error.to_string().contains("cache source identity changed"));
    assert!(seed.take_failure().is_none());
    let mut pending = crate::code_store::CodeStore::default();
    assert!(
        cache
            .load_bytes("helpers", "source", text.as_bytes(), &mut pending, Instant::now())
            .unwrap()
            .is_none()
    );
    assert_eq!(pending.instructions, 0);
    assert!(cache.observed("helpers").is_none());
}

#[test]
fn warm_markers_use_constant_bounded_reads() {
    let expected = "a".repeat(64);
    assert!(marker_matches(expected.as_bytes(), expected.as_bytes()));
    for invalid in [vec![], vec![b'a'; 63], vec![b'a'; 65], vec![255; 64]] {
        assert!(!marker_matches(invalid.as_slice(), expected.as_bytes()));
    }
    struct Growing {
        read: usize,
    }
    impl Read for Growing {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            assert!(self.read + bytes.len() <= 65);
            bytes.fill(b'a');
            self.read += bytes.len();
            Ok(bytes.len())
        }
    }
    assert!(!marker_matches(Growing { read: 0 }, expected.as_bytes()));
    let (_directory, mut cache, _) = fixture();
    cache.compiler = expected;
    assert!(!cache.is_warm());
    let marker = cache.directory.join("build.sha256");
    fs::write(&marker, &cache.compiler).unwrap();
    assert!(cache.is_warm());
    fs::OpenOptions::new()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_len(300_000_000)
        .unwrap();
    assert!(!cache.is_warm());
}
