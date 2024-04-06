//! Module for handling observables with multicast capabilities.
//!
//! This module provides functionality for creating and working with observables that
//! can multicast emissions to multiple subscribers. It includes the `Connectable`
//! observable type, which allows subscribers to connect to an underlying source and
//! receive emissions through a shared subscription.
//!
//! The `Connectable` type enables efficient and controlled multicasting of
//! observable emissions, allowing multiple subscribers to receive the same emissions
//! without duplicating the source work or emissions.

use std::sync::{Arc, Mutex};

use crate::{
    subjects::{SubjectEmitter, SubjectReceiver},
    subscribe::{Fuse, Subscription, SubscriptionHandle, UnsubscribeLogic},
    ObservableExt, Subject, Subscribeable, Unsubscribeable,
};

/// Multicasting observable with a `connect()` method for creating subscriptions to
/// an underlying source, enabling multiple consumers to connect and receive emitted
/// values concurrently.
///
/// `Connectable` observable is a type of observable that does not emit values until
/// the `connect()` method is called. It allows for multiple subscribers to be
/// registered before any values are emitted, enabling subscribers to receive the
/// same set of values concurrently. Once connected, a `Connectable` observable
/// behaves like a regular observable, emitting values to all registered subscribers.
#[derive(Clone)]
pub struct Connectable<T> {
    source: Arc<Mutex<dyn ObservableExt<T> + Send + Sync>>,
    state_subject: (SubjectEmitter<T>, SubjectReceiver<T>),
    fused: bool,
    defused: bool,
    subject: bool,
    connected: Arc<Mutex<bool>>,
    connected_subscription: Arc<Mutex<Option<Subscription>>>,
}

impl<T: Clone + 'static> Connectable<T> {
    /// Creates a new instance of a `Connectable` observable.
    ///
    /// This method constructs a new instance of a `Connectable` observable.
    /// Typically, you will not directly use this method to create a `Connectable`
    /// observable instance. Instead, the [`connectable()`] operator is used to create
    /// instances of `Connectable` observables more conveniently.
    ///
    /// [`connectable()`]: ../trait.ObservableExt.html#method.connectable
    pub fn new(source: Arc<Mutex<dyn ObservableExt<T> + Send + Sync>>) -> Self {
        Connectable {
            source,
            state_subject: Subject::emitter_receiver(),
            fused: false,
            defused: false,
            subject: false,
            connected: Arc::new(Mutex::new(false)),
            connected_subscription: Arc::new(Mutex::new(None)),
        }
    }

    /// Connects the `Connectable` observable, allowing it to emit values to
    /// its subscribers.
    ///
    /// This method triggers the `Connectable` observable to start emitting values to
    /// its subscribers. Until this method is called, the `Connectable` observable
    /// will not emit any values, even if subscribers are present. After calling
    /// `connect()`, the `Connectable` observable will start emitting values to its
    /// subscribers, allowing them to receive and process the emitted data. It
    /// returns a subscription handle which can be used to unsubscribe from the
    /// `Connectable` observable when no longer needed or to await for the connected
    /// source to finish emitting.
    #[must_use]
    pub fn connect(self) -> Subscription {
        // Mark `Connectable` as connected.
        *self.connected.lock().unwrap() = true;

        // Turn `SubjectEmitter` into `Subscriber` and subscribe to it. This will
        // subscribe all sinked `Subscriber`'s in `Subject` to the source observable.
        let mut subscription = self
            .source
            .lock()
            .unwrap()
            .subscribe(self.state_subject.0.into());

        // Extract `subscription_future` so it can be returnrd within new `Subscription`.
        let subscription_future = subscription.subscription_future;
        subscription.subscription_future = SubscriptionHandle::Nil;

        // Store subscription into `connected_subscription` so connected source can
        // be unsubscribed.
        *self.connected_subscription.lock().unwrap() = Some(subscription);

        // Get connected subscription memory location.
        let cs = Arc::clone(&self.connected_subscription);

        // Return `Subscription` that can be used to await connected source to finish
        // emitting or can be used to disconnect the source from connector subject
        // and stopping notifications to all subscribers.
        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                let connectable_subscription = cs.lock().unwrap().take();
                if let Some(connectable_subscription) = connectable_subscription {
                    connectable_subscription.unsubscribe();
                }
            })),
            subscription_future,
        )
    }
}

impl<T> Fuse for Connectable<T> {
    fn set_fused(&mut self, fused: bool, defused: bool) {
        self.fused = fused;
        self.defused = defused;
    }

    fn get_fused(&self) -> (bool, bool) {
        (self.fused, self.defused)
    }
}

impl<T: 'static> Subscribeable for Connectable<T> {
    type ObsType = T;

    fn subscribe(
        &mut self,
        mut s: crate::subscribe::Subscriber<Self::ObsType>,
    ) -> crate::subscribe::Subscription {
        let (fused, defused) = s.get_fused();

        if defused || (fused && !self.fused) {
            self.defused = s.defused;
            self.fused = s.fused;
        } else {
            s.set_fused(self.fused, self.defused);
        }
        // Sink all `Subscriber`'s into `SubjectReceiver`.
        let subject_subscription = self.state_subject.1.subscribe(s);

        // Get connected subscription memory location.
        let cs = Arc::clone(&self.connected_subscription);

        // Get connected indicator memory location.
        let connected = Arc::clone(&self.connected);
        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                // If `unsubscribe()` is called individually on one or more subscriptions
                // before connecting `Connectable` with `connect()`, this flag will be
                // `false` and each subscription can be unsubscribed individually so
                // that it does not emit when `Connectable` is connected.
                if !*connected.lock().unwrap() {
                    subject_subscription.unsubscribe();
                }

                let connectable_subscription = cs.lock().unwrap().take();

                // The unsubscribe logic may be invoked multiple times if the
                // `Connectable` observable has more than one subscriber. Upon the
                // initial invocation, it will contain a valid `Subscription` that
                // can be unsubscribed from. Subsequent invocations will result in
                // `None`. Unsubscribing the `Connectable` observable once is
                // sufficient, regardless of the number of subscribers it has.
                if let Some(connectable_subscription) = connectable_subscription {
                    connectable_subscription.unsubscribe();
                }
            })),
            SubscriptionHandle::Nil,
        )
    }

    fn is_subject(&self) -> bool {
        self.subject
    }

    fn set_subject_indicator(&mut self, s: bool) {
        self.subject = s;
    }
}
