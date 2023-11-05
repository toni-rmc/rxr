//! The `subjects` module provides various types of subjects for handling and observing
//! data streams. Subjects serve both as observers and observables, allowing multiple
//! observers to concurrently subscribe to a single source and receive updates.
//!
//! Unlike in `RxJS`, `rxr` subjects are split into emitter and receiver using the
//! `emitter_receiver` function.
//!
//! The `Subject` emitter behaves as an `Observer`, enabling `next()`, `error()` and
//! `complete()` calls. This also allows the `Subject` emitter to be passed as a
//! parameter to the `subscribe` method of another `Observable`.
//!
//! The `Subject` receiver functions as an `Observable`, enabling you to use the
//! `subscribe` and `unsubscribe` methods on it.
//!
//! There are four specialized varieties of `Subject`, each tailored for particular use
//! cases: `ReplaySubject`, `BehaviorSubject`, `AsyncSubject` and the basic `Subject`.
//! These varieties provide specific functionalities like caching previous values,
//! emitting the most recent or last value, or serving as a simple subject for direct
//! value pushing.

mod async_subject;
mod behavior_subject;
mod replay_subject;
mod subject;

pub use async_subject::*;
pub use behavior_subject::*;
pub use replay_subject::*;
pub use subject::*;

use std::hash::Hasher;

fn random_seed() -> u64 {
    std::hash::BuildHasher::build_hasher(&std::collections::hash_map::RandomState::new()).finish()
}

// Pseudorandom number generator from the "Xorshift RNGs" paper by George Marsaglia.
//
// https://github.com/rust-lang/rust/blob/1.55.0/library/core/src/slice/sort.rs#L559-L573
fn gen_key() -> impl Iterator<Item = u64> {
    let mut random: u64 = random_seed();
    std::iter::repeat_with(move || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    })
}
