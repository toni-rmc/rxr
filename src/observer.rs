//! The `observer` module defines the `Observer` trait, which is implemented by
//! types responsible for observing and handling events emitted by observables.
//! While optional for users to implement, this trait is mainly utilized by the `rxr`
//! library internally.
use std::{error::Error, sync::Arc};

/// The `Observer` trait defines the methods that observers must implement to handle
/// events emitted by an observable stream.
///
/// Observers are responsible for reacting to emitted values, signals of successful
/// completion, and signals of error occurrences.
/// Although users have the flexibility to implement this trait, it's worth noting
/// that the `rxr` library predominantly employs it for internal functionality.
pub trait Observer {
    /// The type of values emitted by the observable stream.
    type NextFnType;

    /// Handles an emitted value from the observable stream.
    ///
    /// This method is called by the observable when a new value is emitted.
    /// Observers can use this method to perform actions based on the emitted value.
    fn next(&mut self, _: Self::NextFnType);

    /// Handles the successful completion of the observable stream.
    ///
    /// This method is called by the observable when it completes successfully.
    /// Observers can use this method to perform cleanup or finalization actions.
    fn complete(&mut self);

    /// Handles an error occurrence during emission from the observable stream.
    ///
    /// This method is called by the observable when an error occurs during emission.
    /// Observers can use this method to react to the error and potentially take
    /// recovery or reporting actions.
    fn error(&mut self, _: Arc<dyn Error + Send + Sync>);
}
