//! Pinning tests for `OAuthTokenManager` concurrency + correctness contracts.
//!
//! ## Scope and structural finding
//!
//! The brief that drove these tests asked us to pin a "refresh-race"
//! deduplication contract on the OAuth token manager. After surveying the
//! file (`crates/at-bridge/src/oauth_token_manager.rs`, ~698 LOC), we
//! determined that **`OAuthTokenManager` does not actually perform OAuth
//! HTTP refresh calls**. It is a *storage* primitive: it encrypts tokens
//! at rest with ChaCha20-Poly1305, tracks expiry timestamps, and exposes
//! `store_token` / `get_token` / `get_refresh_token` / `is_expired` /
//! `should_refresh` / `clear_token`. There is no `reqwest::Client`, no
//! token endpoint URL, no `get_valid_token()` that could race, no
//! single-flight mechanism (`Notify`, `oneshot`, or held async `Mutex`).
//!
//! The actual HTTP refresh is performed in
//! `crates/at-integrations/src/github/oauth.rs` (and the GitLab equivalent),
//! and is invoked from the axum handler `github_oauth_refresh` in
//! `crates/at-bridge/src/http_api/github.rs`. That handler holds a
//! coarse-grained `tokio::sync::RwLock<OAuthTokenManager>` write lock for
//! the duration of the HTTP refresh — there is no per-token single-flight
//! deduplication. Concurrent refresh callers will serialize on the lock
//! and each will issue its own outbound HTTP request, opening exactly the
//! race the brief warned about.
//!
//! That correctness gap is **out of scope for this test-coverage PR**
//! (the brief explicitly forbids fixing it here, and forbids modifying any
//! crate other than `at-bridge`). It is recorded as finding #5 in
//! `docs/test-coverage-followups.md`.
//!
//! What this file pins:
//!
//! * Group A — basic correctness contracts of `get_token` / `is_expired`.
//! * Group B — concurrency contracts on the encrypted storage primitive
//!   (concurrent readers, racing writer + readers, racing clear + readers).
//!   These are the contracts that the higher-level refresh-race fix would
//!   eventually rely on.
//! * Group D — clock/expiry edge cases. Clock is `chrono::Utc::now()` and
//!   not injectable, so we use `expires_in: 0` (boundary) and very short
//!   sub-second sleeps where unavoidable.
//!
//! Group C (auth-class HTTP error variants — 401/429/500) is not
//! applicable to this primitive — there is no HTTP code in this file.
//! Documented as a gap in finding #5.

use at_bridge::oauth_token_manager::{OAuthTokenManager, TokenManagerError};
use std::sync::Arc;
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// Group A — basic correctness
// ---------------------------------------------------------------------------

/// A1: token at exact expiry boundary surfaces `TokenExpired` from
/// `get_token` and `is_expired() == true`. Pins the `Utc::now() >=
/// expires_at` boundary in `is_expired`.
#[tokio::test]
async fn token_at_exact_expiry_is_treated_as_expired() {
    let manager = OAuthTokenManager::new();
    // expires_in: 0 -> expires_at == stored_at == Utc::now() at the call
    // site. By the time we observe it, Utc::now() has advanced, so the
    // `>=` branch fires.
    manager.store_token("ghp_at_boundary", Some(0), None).await;

    assert!(
        manager.is_expired().await,
        "token with expires_in=0 must report as expired at boundary"
    );

    let result = manager.get_token().await;
    assert!(
        matches!(result, Err(TokenManagerError::TokenExpired)),
        "expected TokenExpired, got {:?}",
        result
    );
}

/// A2: a fresh token (large `expires_in`) returns its plaintext through
/// `get_token` with no error. Mirror of the "no refresh needed" path.
#[tokio::test]
async fn fresh_token_returns_value_no_error() {
    let manager = OAuthTokenManager::new();
    manager
        .store_token("ghp_fresh_value_12345", Some(3600), None)
        .await;

    assert!(
        !manager.is_expired().await,
        "freshly stored 1h token must not be expired"
    );
    let token = manager
        .get_token()
        .await
        .expect("fresh token retrieval should succeed");
    assert_eq!(token, "ghp_fresh_value_12345");
}

// ---------------------------------------------------------------------------
// Group B — concurrency / race deduplication contracts on the storage
// primitive. The "refresh-race" the brief asks about lives one layer up
// (HTTP refresh handler), but these tests pin the contracts the higher
// layer relies on: that the encrypted storage never returns torn data,
// never panics under concurrent access, and that overlapping reads of a
// stored token all observe the same plaintext (no per-call key
// regeneration, no nonce reuse path that would corrupt one reader).
// ---------------------------------------------------------------------------

/// B1: N concurrent `get_token` callers must all observe the same
/// plaintext. Pins: the `RwLock<Option<Vec<u8>>>` allows shared reads;
/// the encryption key is shared (not per-call); decryption is
/// deterministic for the same ciphertext.
#[tokio::test]
async fn concurrent_get_token_callers_all_see_same_value() {
    const N: usize = 16;
    let manager = Arc::new(OAuthTokenManager::new());
    manager
        .store_token("ghp_shared_plaintext_xyz", Some(3600), None)
        .await;

    let mut set = JoinSet::new();
    for _ in 0..N {
        let m = Arc::clone(&manager);
        set.spawn(async move { m.get_token().await });
    }

    let mut results = Vec::with_capacity(N);
    while let Some(joined) = set.join_next().await {
        let r = joined.expect("task should not panic");
        results.push(r.expect("get_token under concurrent load must succeed"));
    }
    assert_eq!(results.len(), N);
    for tok in &results {
        assert_eq!(tok, "ghp_shared_plaintext_xyz");
    }
}

/// B2: `clear_token` racing N concurrent readers must yield only one of
/// two outcomes per reader: the original plaintext or `NoToken`. We must
/// never see a `Crypto` (decryption) error, which would indicate a torn
/// state (e.g. metadata cleared but ciphertext still present, or key
/// rotated mid-decrypt).
#[tokio::test]
async fn clear_during_concurrent_get_returns_typed_error_not_torn_state() {
    const N: usize = 32;
    let manager = Arc::new(OAuthTokenManager::new());
    manager
        .store_token("ghp_to_be_cleared_under_load", Some(3600), None)
        .await;

    let mut set = JoinSet::new();
    for _ in 0..N {
        let m = Arc::clone(&manager);
        set.spawn(async move { m.get_token().await });
    }
    // Race a clear against the readers. Because tokio executes tasks
    // cooperatively, the clear will land somewhere in the middle.
    let clearer = {
        let m = Arc::clone(&manager);
        tokio::spawn(async move { m.clear_token().await })
    };

    let mut ok_count = 0usize;
    let mut no_token_count = 0usize;
    while let Some(joined) = set.join_next().await {
        match joined.expect("reader task should not panic") {
            Ok(t) => {
                assert_eq!(t, "ghp_to_be_cleared_under_load");
                ok_count += 1;
            }
            Err(TokenManagerError::NoToken) => {
                no_token_count += 1;
            }
            Err(other) => panic!(
                "unexpected error variant under clear-race: {:?} \
                 (only Ok or NoToken are valid)",
                other
            ),
        }
    }
    clearer.await.expect("clear task should not panic");

    assert_eq!(
        ok_count + no_token_count,
        N,
        "every reader must complete with Ok or NoToken"
    );
    // After the clear lands, the manager must report no valid token.
    assert!(
        !manager.has_valid_token().await,
        "post-clear: has_valid_token must be false"
    );
}

/// B3: two racing `store_token` writers leave the manager in a coherent
/// state: `get_token` returns exactly one of the two written values, never
/// a torn frankenstein, never a `Crypto` error. Pins atomicity of
/// `store_token` w.r.t. the `(encrypted_token, metadata)` pair.
#[tokio::test]
async fn concurrent_writers_overwrite_atomically() {
    let manager = Arc::new(OAuthTokenManager::new());

    let m1 = Arc::clone(&manager);
    let w1 = tokio::spawn(async move {
        m1.store_token("ghp_writer_one_AAA", Some(3600), Some("ghr_w1"))
            .await
    });
    let m2 = Arc::clone(&manager);
    let w2 = tokio::spawn(async move {
        m2.store_token("ghp_writer_two_BBB", Some(7200), Some("ghr_w2"))
            .await
    });
    w1.await.expect("writer 1 should not panic");
    w2.await.expect("writer 2 should not panic");

    let access = manager
        .get_token()
        .await
        .expect("post-race get_token must succeed");
    let refresh = manager
        .get_refresh_token()
        .await
        .expect("post-race get_refresh_token must succeed");

    // The (access, refresh) pair must come from the SAME writer — never
    // mixed. This pins atomicity of the `TokenData` blob under the
    // encryption envelope, which is the property the higher-level refresh
    // logic relies on to avoid persisting half-updated tokens.
    let coherent = (access == "ghp_writer_one_AAA" && refresh == "ghr_w1")
        || (access == "ghp_writer_two_BBB" && refresh == "ghr_w2");
    assert!(
        coherent,
        "torn write detected: access={access:?} refresh={refresh:?}"
    );
}

/// B4: concurrent callers racing against an EXPIRED token all receive
/// `TokenExpired` (no caller silently gets stale plaintext through). This
/// is the storage-layer mirror of the "all callers see the same refresh
/// outcome" contract that a future single-flight refresh would need.
#[tokio::test]
async fn concurrent_callers_on_expired_token_all_see_typed_error() {
    const N: usize = 16;
    let manager = Arc::new(OAuthTokenManager::new());
    // Boundary expiry — every concurrent caller must observe expired.
    manager.store_token("ghp_already_dead", Some(0), None).await;

    let mut set = JoinSet::new();
    for _ in 0..N {
        let m = Arc::clone(&manager);
        set.spawn(async move { m.get_token().await });
    }
    let mut errs = 0usize;
    while let Some(joined) = set.join_next().await {
        let r = joined.expect("reader task should not panic");
        assert!(
            matches!(r, Err(TokenManagerError::TokenExpired)),
            "every concurrent caller must see TokenExpired, got {:?}",
            r
        );
        errs += 1;
    }
    assert_eq!(errs, N);
}

/// B5: a subsequent `store_token` AFTER an expired-token failure makes
/// the manager usable again. Pins "errors are not cached" — there is no
/// sticky failure state on the storage primitive, so an external retry
/// flow that observes `TokenExpired` and then writes a new token will
/// see the new token on the very next `get_token`.
#[tokio::test]
async fn store_after_expired_failure_recovers() {
    let manager = OAuthTokenManager::new();
    manager.store_token("ghp_old_dead", Some(0), None).await;

    // First read sees expired.
    assert!(matches!(
        manager.get_token().await,
        Err(TokenManagerError::TokenExpired)
    ));

    // External "refresh" writes a new token.
    manager
        .store_token("ghp_brand_new_after_refresh", Some(3600), None)
        .await;

    // Next read sees the new token, no leftover error state.
    let t = manager
        .get_token()
        .await
        .expect("post-store get_token must succeed");
    assert_eq!(t, "ghp_brand_new_after_refresh");
    assert!(!manager.is_expired().await);
}

// ---------------------------------------------------------------------------
// Group D — clock / expiry edge
// ---------------------------------------------------------------------------

/// D1: `should_refresh()` flips at the 5-minute pre-expiry skew window.
/// Pins the hardcoded `Duration::minutes(5)` threshold in
/// `OAuthTokenManager::should_refresh` (oauth_token_manager.rs:363).
/// Clock is `chrono::Utc::now()` and not injectable, so we test on the
/// two safe sides: an `expires_in` well inside the window (60s) must
/// trigger refresh; one well outside (10 min) must not.
#[tokio::test]
async fn should_refresh_pins_five_minute_skew_window() {
    let inside = OAuthTokenManager::new();
    inside.store_token("ghp_inside_skew", Some(60), None).await;
    assert!(
        inside.should_refresh().await,
        "token expiring in 60s must trigger should_refresh (5-minute skew)"
    );

    let outside = OAuthTokenManager::new();
    outside
        .store_token("ghp_outside_skew", Some(600), None)
        .await;
    assert!(
        !outside.should_refresh().await,
        "token expiring in 600s (>5 min) must not trigger should_refresh"
    );
}

/// D2: `is_expired()` and `should_refresh()` are independent of whether a
/// token has been retrieved before — calling `get_token` does not "cache"
/// or "freeze" the expiry decision. Pins that expiry checks always read
/// fresh `Utc::now()`.
#[tokio::test]
async fn expiry_check_is_not_cached_by_prior_get_token() {
    let manager = OAuthTokenManager::new();
    manager
        .store_token("ghp_expiry_independence", Some(3600), None)
        .await;

    // Prime the path: a successful get_token.
    let _ = manager.get_token().await.unwrap();
    assert!(!manager.is_expired().await);

    // Overwrite with an expired token. is_expired must reflect the new
    // metadata immediately — there must be no stale cached "not expired"
    // decision from the prior get_token call.
    manager
        .store_token("ghp_now_expired", Some(0), None)
        .await;
    assert!(
        manager.is_expired().await,
        "is_expired must re-read metadata; prior get_token must not cache"
    );
}
