use async_httplib::{Error as HttpError};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Error {
    InvalidUrl,
    InvalidMethod,
    InvalidVersion,
    InvalidStatus,
    InvalidInput,
    InvalidHeader,
    InvalidData,
    UnableToConnect,
    UnableToRead,
    UnableToWrite,
    LimitExceeded,
}

impl<'a> std::convert::TryFrom<HttpError> for Error {
    type Error = crate::Error;

    fn try_from(err: HttpError) -> Result<Self, Self::Error> {
        match err {
            HttpError::InvalidMethod => Ok(Error::InvalidMethod),
            HttpError::InvalidVersion => Ok(Error::InvalidVersion),
            HttpError::InvalidStatus => Ok(Error::InvalidStatus),
            HttpError::InvalidInput => Ok(Error::InvalidInput),
            HttpError::UnableToRead => Ok(Error::UnableToRead),
            HttpError::UnableToWrite => Ok(Error::UnableToWrite),
            HttpError::LimitExceeded => Ok(Error::LimitExceeded),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;
    
    #[async_std::test]
    async fn implements_try_from() {
        assert_eq!(Error::try_from(HttpError::InvalidInput).unwrap(), Error::InvalidInput);
    }
}
