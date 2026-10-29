//! Handle table: the `long` the JVM holds is a key, not a pointer.
//!
//! A raw `Box` pointer cannot be validated: a stale or forged `long` would be undefined
//! behaviour. Keys are looked up in a table instead, so a null, freed or made-up handle is
//! simply "not found" ([`ERR_NULL`]), and a `nativeFree` racing a feed on another thread
//! cannot free memory in use (the `Arc` keeps it alive until the feed returns). Keys are
//! never reused. The lookup costs one short mutex hold per call, noise next to a frame.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::layout::{ERR_NULL, ERR_PANIC};
use crate::scanner::Scanner;

type Entry = Arc<Mutex<Scanner>>;

struct Registry {
    next: i64,
    scanners: BTreeMap<i64, Entry>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    next: 1,
    scanners: BTreeMap::new(),
});

fn table() -> MutexGuard<'static, Registry> {
    // The table is only ever mutated by single insert/remove calls, so a poisoned lock
    // still holds a consistent map.
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Create a scanner and return its handle (always positive), or 0 if the table is full.
pub(crate) fn create() -> i64 {
    let scanner = Arc::new(Mutex::new(Scanner::new()));
    let mut t = table();
    let Some(next) = t.next.checked_add(1) else {
        return 0;
    };
    let handle = t.next;
    t.next = next;
    t.scanners.insert(handle, scanner);
    handle
}

/// Drop the scanner behind `handle`. Unknown handles are ignored. Returns whether one existed.
pub(crate) fn free(handle: i64) -> bool {
    let removed = table().scanners.remove(&handle);
    removed.is_some()
}

/// Run `f` on the scanner behind `handle`.
///
/// `Err(ERR_NULL)` for a handle that is zero, negative, freed or unknown; `Err(ERR_PANIC)` if
/// an earlier call panicked while holding the scanner (its state is then untrustworthy and
/// the caller should free it). Calls on one handle are serialized.
pub(crate) fn with<T>(handle: i64, f: impl FnOnce(&mut Scanner) -> T) -> Result<T, i32> {
    let entry = table().scanners.get(&handle).cloned().ok_or(ERR_NULL)?;
    let mut scanner = entry.lock().map_err(|_| ERR_PANIC)?;
    Ok(f(&mut scanner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_handles_are_errors() {
        for h in [0, -1, i64::MIN, i64::MAX] {
            assert_eq!(with(h, |_| ()), Err(ERR_NULL), "{h}");
            assert!(!free(h));
        }
    }

    #[test]
    fn handles_are_unique_and_not_reused() {
        let a = create();
        let b = create();
        assert!(a > 0 && b > a);
        assert_eq!(with(a, |s| s.session_id()), Ok(None));
        assert!(free(a));
        assert_eq!(with(a, |_| ()), Err(ERR_NULL));
        assert!(!free(a), "double free is a no-op");
        let c = create();
        assert!(c > b);
        assert!(with(b, |_| ()).is_ok());
        assert!(free(b) && free(c));
    }

    #[test]
    fn a_panic_poisons_only_that_scanner() {
        let a = create();
        let b = create();
        let r = std::panic::catch_unwind(|| with(a, |_| panic!("boom")));
        assert!(r.is_err());
        assert_eq!(with(a, |_| ()), Err(ERR_PANIC));
        assert!(with(b, |_| ()).is_ok());
        assert!(free(a) && free(b));
    }
}
