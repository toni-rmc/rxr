use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, Observer, Subscribeable,
};

#[test]
fn unchained_observable() {
    let value = 100;
    let o = Subscriber::new(
        move |v| {
            assert_eq!(
                v, value,
                "expected integer value {} but {} is emitted",
                value, v
            );
        },
        |_observable_error| {},
        move || {},
    );

    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        o.next(value);
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    s.subscribe(o);
}
