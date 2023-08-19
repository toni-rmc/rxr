use std::{error::Error, sync::Arc};

pub trait Observer {
    type NextFnType;

    fn next(&mut self, _: Self::NextFnType);
    fn complete(&mut self);
    fn error(&mut self, _: Arc<dyn Error + Send + Sync>);
}
