use std::error::Error;

#[derive(Debug)]
pub struct CustomError;

impl std::fmt::Display for CustomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Customize the error message as needed
        write!(f, "Custom error occurred")
    }
}

impl Error for CustomError {}
