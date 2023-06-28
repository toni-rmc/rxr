use std::error::Error;
use std::fmt;

type Result<T> = std::result::Result<T, ObservableError>;

#[derive(Debug)]
pub enum ObservableError {
    InfoRoot {
        name: &'static str,
        source: Box<dyn Error>,
    },
    Root(Box<dyn Error>),
    Info(String),
    NoInfo,
    
}

impl ObservableError {
    pub fn new(name: &str, s: impl Error) {}
}

impl fmt::Display for ObservableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InfoRoot { name, .. } => write!(f, "{} observable emitted an error", name),
            Self::Info(name) => write!(f, "{} observable emitted an error", name),
            _ => write!(f, "observable emitted an error")
        }
        
    }
}

impl Error for ObservableError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::InfoRoot { source: ref s, .. } | Self::Root(ref s) => {
                return Some(s.as_ref());
            },
            _ => return None,
        }
    }
}

