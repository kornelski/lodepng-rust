use std;
use std::error;
use std::fmt;
use std::io;
use ffi::Error;
use ffi;

impl Error {
    /// Returns an English description of the numerical error code.
    pub fn as_str(&self) -> &'static str {
        unsafe {
            let cstr = std::ffi::CStr::from_ptr(ffi::lodepng_error_text(self.0) as *const _);
            std::str::from_utf8(cstr.to_bytes()).unwrap()
        }
    }

    /// Helper function for the library
    pub fn to_result(self) -> Result<(), Error> {
        match self {
            Error(0) => Ok(()),
            err => Err(err),
        }
    }
}

impl From<Error> for Result<(), Error> {
    fn from(err: Error) -> Self {
        err.to_result()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.as_str(), self.0)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        self.as_str()
    }
}

#[doc(hidden)]
impl std::convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        match err.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => Error(78),
            _ => Error(79),
        }
    }
}
