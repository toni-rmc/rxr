use std::sync::{Arc, Mutex};

use tokio::runtime::{Handle, TryCurrentError};

use crate::{
    subscribe::{SubscriptionHandle, UnsubscribeLogic},
    subscription::subscribe::Subscription,
    ObservableExt, Unsubscribeable,
};

enum SenderRaw<T> {
    OSSender(std::sync::mpsc::Sender<T>),
    TokioSender(tokio::sync::mpsc::Sender<T>),
}

pub(super) struct Sender<T> {
    tokio_handle: Result<Handle, TryCurrentError>,
    sender: SenderRaw<T>,
}

pub(super) enum Receiver<T> {
    OSReceiver(std::sync::mpsc::Receiver<T>),
    TokioReceiver(tokio::sync::mpsc::Receiver<T>),
}

impl Sender<bool> {
    pub(super) fn send_unsubscribe_signal(&self) {
        // Check which channel is used and use its sender to send unsubscribe signal.
        if let SenderRaw::TokioSender(s) = &self.sender {
            let s = s.clone();
            if let Ok(h) = &self.tokio_handle {
                h.spawn(async move {
                    s.send(true).await.unwrap();
                });
            }
        } else if let SenderRaw::OSSender(s) = &self.sender {
            let _ = s.send(true);
        }
    }

    pub(super) fn is_tokio_used(&self) -> bool {
        self.tokio_handle.is_ok()
    }
}

impl Receiver<bool> {
    pub(super) fn unsubscribe_background_emissions<T: 'static, O: ObservableExt<T>>(
        self,
        observable: &O,
        mut unsubscriber: Subscription,
    ) -> Subscription {
        let handle = unsubscriber.subscription_future;
        unsubscriber.subscription_future = SubscriptionHandle::Nil;

        if observable.is_subject() {
            // Skip opening a thread or a task if operator is called on one of the
            // `Subject` variants.
            return Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    unsubscriber.unsubscribe();
                })),
                handle,
            );
        };

        let mut is_future = false;
        if let UnsubscribeLogic::Future(_) = &unsubscriber.unsubscribe_logic {
            is_future = true;
        };
        let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
        let u_cloned = Arc::clone(&unsubscriber);

        // Check which channel is used and use its receiver to receive
        // unsubscribe signal.
        match self {
            Receiver::TokioReceiver(mut receiver) => {
                tokio::task::spawn(async move {
                    if receiver.recv().await.is_some() {
                        // println!("SIGNAL received");
                        if let Some(s) = unsubscriber.lock().unwrap().take() {
                            // println!("UNSUBSCRIBE called");
                            s.unsubscribe();
                        }
                    }
                });
            }
            Receiver::OSReceiver(receiver) => {
                std::thread::spawn(move || {
                    if receiver.recv().is_ok() {
                        // println!("---- SIGNAL received");
                        if let Some(s) = unsubscriber.lock().unwrap().take() {
                            // println!("UNSUBSCRIBE called");
                            s.unsubscribe();
                        }
                    }
                });
            }
        };

        if is_future {
            return Subscription::new(
                UnsubscribeLogic::Future(Box::pin(async move {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                handle,
            );
        }
        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                if let Some(s) = u_cloned.lock().unwrap().take() {
                    s.unsubscribe();
                }
            })),
            handle,
        )
    }
}

fn check_tokio_runtime() -> (Result<Handle, TryCurrentError>, bool) {
    let mut is_current = false;

    // Check if Tokio runtime is used.
    let tokio_handle = tokio::runtime::Handle::try_current();
    if let Ok(t) = &tokio_handle {
        // If it is check if runtime flavor is `current_thread`.
        if let tokio::runtime::RuntimeFlavor::CurrentThread = t.runtime_flavor() {
            is_current = true;
        }
    }
    (tokio_handle, is_current)
}

pub(super) fn setup_unsubscribe_channel() -> (Sender<bool>, Receiver<bool>) {
    let (tokio_handle, is_current) = check_tokio_runtime();
    let sender;
    let receiver;

    // Open non-blocking channel only if Tokio is used and runtime flavor is
    // not `current_thread.`
    if let (Ok(_), false) = (&tokio_handle, is_current) {
        let (tokio_sender, tokio_receiver) = tokio::sync::mpsc::channel(10);
        sender = SenderRaw::TokioSender(tokio_sender);
        receiver = Receiver::TokioReceiver(tokio_receiver);
    } else {
        // Tokio runtime flavor is `current_thread` or Tokio is not used at
        // all; open `std` sync channel.
        let (os_sender, os_receiver) = std::sync::mpsc::channel();
        sender = SenderRaw::OSSender(os_sender);
        receiver = Receiver::OSReceiver(os_receiver);
    }
    let sender = Sender {
        tokio_handle,
        sender,
    };
    (sender, receiver)
}
