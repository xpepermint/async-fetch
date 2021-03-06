use std::fmt;
use std::pin::Pin;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::io::{Error, ErrorKind};
use std::str::FromStr;
use async_std::io::{Read};
use async_httplib::{Status, Version, read_exact, read_chunks};
use crate::{read_content_length, read_transfer_encoding};

pub struct Response<'a> {
    status: Status,
    version: Version,
    headers: HashMap<String, String>,
    reader: Pin<Box<dyn Read + Send + Unpin + 'a>>,
    chunkline_limit: Option<usize>,
    body_limit: Option<usize>,
}

impl<'a> Response<'a> {

    pub fn default() -> Self {
        Self {
            status: Status::Ok,
            version: Version::Http1_1,
            headers: HashMap::with_hasher(RandomState::new()),
            reader: Box::pin("".as_bytes()),
            chunkline_limit: None,
            body_limit: None,
        }
    }

    pub fn with_reader<R>(reader: R) -> Self
        where
        R: Read + Send + Unpin + 'a,
    {
        let mut res = Self::default();
        res.set_reader(reader);
        res
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn header<N: Into<String>>(&self, name: N) -> Option<&String> {
        self.headers.get(&name.into())
    }

    pub fn reader(&self) -> &Pin<Box<dyn Read + Send + Unpin + 'a>> {
        &self.reader
    }

    pub fn chunkline_limit(&self) -> &Option<usize> {
        &self.chunkline_limit
    }

    pub fn body_limit(&self) -> &Option<usize> {
        &self.body_limit
    }

    pub fn has_status(&self, value: Status) -> bool {
        self.status == value
    }

    pub fn has_version(&self, value: Version) -> bool {
        self.version == value
    }

    pub fn has_headers(&self) -> bool {
        !self.headers.is_empty()
    }

    pub fn has_header<N: Into<String>>(&self, name: N) -> bool {
        self.headers.contains_key(&name.into())
    }

    pub fn has_chunkline_limit(&self) -> bool {
        self.chunkline_limit.is_some()
    }

    pub fn has_body_limit(&self) -> bool {
        self.body_limit.is_some()
    }

    pub fn set_status(&mut self, value: Status) {
        self.status = value;
    }

    pub fn set_status_str(&mut self, value: &str) -> Result<(), Error> {
        self.status = Status::from_str(value)?;
        Ok(())
    }

    pub fn set_version(&mut self, value: Version) {
        self.version = value;
    }

    pub fn set_version_str(&mut self, value: &str) -> Result<(), Error> {
        self.version = Version::from_str(value)?;
        Ok(())
    }

    pub fn set_header<N: Into<String>, V: Into<String>>(&mut self, name: N, value: V) {
        self.headers.insert(name.into(), value.into());
    }

    pub fn set_reader<R>(&mut self, reader: R)
        where
        R: Read + Send + Unpin + 'a,
    {
        self.reader = Box::pin(reader);
    }

    pub fn set_chunkline_limit(&mut self, length: usize) {
        self.chunkline_limit = Some(length);
    }

    pub fn set_body_limit(&mut self, length: usize) {
        self.body_limit = Some(length);
    }

    pub fn remove_header<N: Into<String>>(&mut self, name: N) {
        self.headers.remove(&name.into());
    }

    pub fn clear_headers(&mut self) {
        self.headers.clear();
    }

    pub fn to_proto_string(&self) -> String {
        let mut output = String::new();
        if !self.has_version(Version::Http0_9) {
            output.push_str(&format!("{} {} {}\r\n", self.version, self.status, self.status.reason()));

            for (name, value) in self.headers.iter() {
                output.push_str(&format!("{}: {}\r\n", name, value));
            }

            output.push_str("\r\n");
        }
        output
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();

        if read_transfer_encoding(&self.headers) == "chunked" {
            read_chunks(&mut self.reader, &mut data, (self.chunkline_limit, self.body_limit)).await?;
        } else if self.has_header("Content-Length") {
            let length = read_content_length(&self.headers, self.body_limit)?;
            read_exact(&mut self.reader, &mut data, length).await?;
        }

        Ok(data)
    }

    pub async fn recv_string(&mut self) -> Result<String, Error> {
        let data = self.recv().await?;
        let txt = match String::from_utf8(data) {
            Ok(txt) => txt,
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e.to_string())),
        };
        Ok(txt)
    }

    #[cfg(feature = "json")]
    pub async fn recv_json(&mut self) -> Result<serde_json::Value, Error> {
        let mut data = self.recv().await?;
        if data.is_empty() {
            data = "{}".as_bytes().to_vec();
        }
        let json: serde_json::Value = match serde_json::from_slice(&data) {
            Ok(json) => json,
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e.to_string())),
        };
        Ok(json)
    }
}

impl fmt::Display for Response<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.to_proto_string())
    }
}

impl From<Response<'_>> for String {
    fn from(item: Response) -> String {
        item.to_proto_string()
    }
}
