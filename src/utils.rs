use std::collections::HashMap;
use crate::{Error};

pub fn read_transfer_encoding(headers: &HashMap<String, String>) -> &str {
    match headers.get("Transfer-Encoding") {
        Some(encoding) => encoding,
        None => "identity",
    }
}

pub fn read_content_length(headers: &HashMap<String, String>, limit: Option<usize>) -> Result<usize, Error> {
    match headers.get("Content-Length") {
        Some(length) => match length.parse::<usize>() {
            Ok(length) => match limit {
                Some(limit) => match limit >= length {
                    true => Ok(length),
                    false => Err(Error::LimitExceeded),
                },
                None => Ok(length),
            },
            Err(_) => Err(Error::InvalidInput),
        },
        None => Err(Error::InvalidHeader),
    }
}
