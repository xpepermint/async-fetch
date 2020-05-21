use std::collections::HashMap;
use std::io::{Error, ErrorKind};

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
                    false => Err(Error::new(ErrorKind::InvalidData, "The operation hit the limit of {} bytes while reading the HTTP body chunk data.")),
                },
                None => Ok(length),
            },
            Err(e) => Err(Error::new(ErrorKind::InvalidData, e.to_string())),
        },
        None => Err(Error::new(ErrorKind::InvalidData, "The header `Content-Length` cannot found.")),
    }
}
