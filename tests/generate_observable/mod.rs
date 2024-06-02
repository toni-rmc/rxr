use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, Observer,
};

pub(crate) fn generate_u32_observable(
    end: u32,
    last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
) -> Observable<u32> {
    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(i) = rx.recv() {
                *done_c.lock().unwrap() = i;
            }
        });

        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = std::thread::spawn(move || {
            let mut last_emit = 0;

            for i in 0..=end {
                if *done.lock().unwrap() == true {
                    break;
                }
                last_emit = i;
                o.next(i);
                // Important. Put an await point after each emit.
                std::thread::sleep(Duration::from_millis(1));
            }
            o.complete();
            last_emit_assert.lock().unwrap()(last_emit);
        });

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                if let Err(_) = tx.send(true) {
                    eprintln!("receiver dropped");
                }
            })),
            SubscriptionHandle::JoinThread(jh),
        )
    })
}

#[allow(dead_code)]
pub(crate) async fn generate_u32_observable_async(
    end: u32,
    last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
) -> Observable<u32> {
    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        tokio::task::spawn(async move {
            if let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }
        });

        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = tokio::task::spawn(async move {
            let mut last_emit = 0;

            for i in 0..=end {
                if *done.lock().unwrap() == true {
                    break;
                }
                last_emit = i;
                o.next(i);
                // Important. Put an await point after each emit.
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            o.complete();
            last_emit_assert.lock().unwrap()(last_emit);
        });

        Subscription::new(
            UnsubscribeLogic::Future(Box::pin(async move {
                if let Err(_) = tx.send(true).await {
                    eprintln!("receiver dropped");
                }
            })),
            SubscriptionHandle::JoinTask(jh),
        )
    })
}

#[allow(dead_code)]
pub(crate) fn generate_delayed_observable(
    end: u32,
    time_delay_emit_ms: u64,
    last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
) -> Observable<u32> {
    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        tokio::task::spawn(async move {
            if let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }
        });

        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = tokio::task::spawn(async move {
            let mut last_emit = 0;

            for i in 0..end {
                if *done.lock().unwrap() == true {
                    break;
                }
                // Delay before emitting.
                tokio::time::sleep(Duration::from_millis(time_delay_emit_ms)).await;
                last_emit = i;
                o.next(i);
                // Important. Put an await point after each emit.
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            last_emit_assert.lock().unwrap()(last_emit);
            o.complete();
        });

        Subscription::new(
            UnsubscribeLogic::Future(Box::pin(async move {
                if let Err(_) = tx.send(true).await {
                    eprintln!("receiver dropped");
                }
            })),
            SubscriptionHandle::JoinTask(jh),
        )
    })
}
