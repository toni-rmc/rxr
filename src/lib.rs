//! # rxr
//!
//! `rxr` is an implementation of the reactive extensions for the Rust
//! programming language, inspired by the popular `RxJS` library in JavaScript.
//! Currently, `rxr` implements a smaller subset of operators compared to `RxJS`.
//!
//! ## Design
//!
//! `rxr` supports `Observables` and `Subjects`. You define your own `Observables` with
//! functionality you want. For examples on how to define your `Observables` see the
//! [Creating `Observables`]. Your `Observables` can be synchronous or asynchronous. For
//! asynchronous `Observables`, you can utilize OS threads or `Tokio` tasks.
//!
//! [Creating `Observables`]: observable/struct.Observable.html

pub mod observable;
pub mod observer;
pub mod subjects;
pub mod subscription;

// pub use subscribe::*;
pub use observable::timestamp::TimestampedEmit;
pub use observable::*;
pub use observer::Observer;
pub use subjects::Subject;
pub use subscribe::{Subscribeable, Unsubscribeable};
pub use subscription::*;
