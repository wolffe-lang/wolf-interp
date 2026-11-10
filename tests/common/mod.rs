//! What the corpus-walking test binaries share (is85): one way to spread a
//! walk over a few threads. Each binary that walks the whole pinned corpus
//! used to do it one file at a time on one thread, and the corpus is a few
//! programs of tens of thousands of container calls; on the slowest hosted
//! windows machines that made single tests run for an hour or more. The
//! observations are the same ones, taken in a different order and gathered
//! back into the walk's order; no assertion moves.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The most threads one walk takes: the hosted runners' core count (four;
/// three on macOS, which `available_parallelism` reports). Two walks may run
/// at once in one binary, and a heavy corpus program holds a large list, so
/// a bigger host does not get more.
pub const WALKERS: usize = 4;

/// `work(i)` for every `i` in `0..n`, on up to [`WALKERS`] threads, answered
/// in index order. A panic stays its own index's `Err`, carrying the panic's
/// message, so the caller can report the file it belongs to in walk order.
pub fn each<T: Send>(n: usize, work: impl Fn(usize) -> T + Sync) -> Vec<Result<T, String>> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .clamp(1, WALKERS)
        .min(n.max(1));
    let mut results: Vec<Option<Result<T, String>>> = (0..n).map(|_| None).collect();
    std::thread::scope(|scope| {
        let (next, work) = (&next, &work);
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break done;
                        }
                        let result =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(i)))
                                .map_err(|payload| {
                                    payload
                                        .downcast_ref::<&str>()
                                        .map(|s| (*s).to_owned())
                                        .or_else(|| payload.downcast_ref::<String>().cloned())
                                        .unwrap_or_else(|| "a non-string panic".to_owned())
                                });
                        done.push((i, result));
                    }
                })
            })
            .collect();
        for handle in handles {
            for (i, result) in handle.join().expect("a walker catches its own panics") {
                results[i] = Some(result);
            }
        }
    });
    results
        .into_iter()
        .map(|result| result.expect("every index was taken by one walker"))
        .collect()
}
