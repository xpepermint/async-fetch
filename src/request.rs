use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::convert::TryFrom;
use std::str::FromStr;
use url::{Url, Position};
use async_std::io::{Read, Write};
use async_uninet::{SocketAddr, Stream};
use async_httplib::{read_first_line, parse_version, parse_status, read_header,
    write_slice, write_all, write_exact, write_chunks, flush_write};
use crate::{Method, Version, Response, Error, read_content_length};

#[derive(Debug)]
pub struct Request {
    url: Url,
    method: Method,
    version: Version,
    headers: HashMap<String, String>,
    relay: Option<String>,
    body_limit: Option<usize>,
}

impl Request {

    pub fn default() -> Self {
        Self {
            url: Url::parse("http://localhost").unwrap(),
            method: Method::Get,
            version: Version::Http1_1,
            headers: HashMap::with_hasher(RandomState::new()),
            relay: None,
            body_limit: None,
        }
    }

    pub fn parse_url<U>(url: U) -> Result<Self, Error>
        where
        U: Into<String>,
    {
        let mut req = Request::default();
        req.set_url_str(url.into())?;
        Ok(req)
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    fn scheme(&self) -> &str {
        self.url.scheme()
    }

    fn host(&self) -> &str {
        match self.url.host_str() {
            Some(host) => host,
            None => "localhost",
        }
    }

    fn port(&self) -> u16 {
        match self.url.port_or_known_default() {
            Some(port) => port,
            None => 80,
        }
    }

    fn host_with_port(&self) -> String {
        format!("{}:{}", self.host(), self.port())
    }

    fn socket_address(&self) -> String {
        match &self.relay {
            Some(relay) => relay.to_string(),
            None => self.host_with_port(),
        }
    }

    fn uri(&self) -> &str {
        &self.url[Position::BeforePath..]
    }

    pub fn method(&self) -> &Method {
        &self.method
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

    pub fn relay(&self) -> &Option<String> {
        &self.relay
    }

    pub fn body_limit(&self) -> &Option<usize> {
        &self.body_limit
    }

    pub fn headers_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.headers
    }

    pub fn has_method(&self, value: Method) -> bool {
        self.method == value
    }

    pub fn has_version(&self, value: Version) -> bool {
        self.version == value
    }

    pub fn has_header<N: Into<String>>(&self, name: N) -> bool {
        self.headers.contains_key(&name.into())
    }

    pub fn has_body_limit(&self) -> bool {
        self.body_limit.is_some()
    }

    pub fn set_url(&mut self, value: Url) {
        self.url = value;
    }

    pub fn set_url_str<V: Into<String>>(&mut self, value: V) -> Result<(), Error> {
        self.url = match Url::parse(&value.into()) {
            Ok(url) => url,
            Err(_) => return Err(Error::InvalidUrl),
        };
        Ok(())
    }

    pub fn set_method(&mut self, value: Method) {
        self.method = value;
    }

    pub fn set_method_str(&mut self, value: &str) -> Result<(), Error> {
        self.method = match Method::from_str(value) {
            Ok(method) => method,
            Err(e) => return Err(Error::try_from(e).unwrap()),
        };
        Ok(())
    }

    pub fn set_version(&mut self, value: Version) {
        self.version = value;
    }

    pub fn set_version_str(&mut self, value: &str) -> Result<(), Error> {
        self.version = match Version::from_str(value) {
            Ok(version) => version,
            Err(e) => return Err(Error::try_from(e).unwrap()),
        };
        Ok(())
    }

    pub fn set_header<N: Into<String>, V: Into<String>>(&mut self, name: N, value: V) {
        self.headers.insert(name.into(), value.into());
    }

    pub fn set_relay<V: Into<String>>(&mut self, value: V) {
        self.relay = Some(value.into());
    }

    pub fn set_body_limit(&mut self, length: usize) {
        self.body_limit = Some(length);
    }

    pub fn remove_header<N: Into<String>>(&mut self, name: N) {
        self.headers.remove(&name.into());
    }

    pub fn remove_relay(&mut self) {
        self.relay = None;
    }

    pub fn clear_headers(&mut self) {
        self.headers.clear();
    }

    pub fn to_proto_string(&self) -> String {
        let mut output = String::new();

        match self.version {
            Version::Http0_9 => {
                output.push_str(&format!("GET {}\r\n", self.uri()));
            },
            _ => {
                output.push_str(&format!("{} {} {}\r\n", self.method(), self.uri(), self.version()));
                for (name, value) in self.headers.iter() {
                    output.push_str(&format!("{}: {}\r\n", name, value));
                }
                output.push_str("\r\n");
            },
        };

        output
    }

    pub async fn send<'a>(&mut self) -> Result<Response<'a>, Error> {
        self.update_host_header();

        match self.scheme() {
            "http" => self.send_http(&mut "".as_bytes()).await,
            "https" => self.send_https(&mut "".as_bytes()).await,
            _ => Err(Error::InvalidUrl),
        }
    }

    pub async fn send_stream<'a, R>(&mut self, body: &mut R) -> Result<Response<'a>, Error>
        where
        R: Read + Unpin,
    {
        self.update_host_header();
        self.update_body_headers();
        
        match self.scheme() {
            "http" => self.send_http(body).await,
            "https" => self.send_https(body).await,
            _ => Err(Error::InvalidUrl),
        }
    }

    pub async fn send_slice<'a>(&mut self, body: &[u8]) -> Result<Response<'a>, Error> {
        self.set_header("Content-Length", body.len().to_string());
        self.send_stream(&mut body.clone()).await
    }

    pub async fn send_str<'a>(&mut self, body: &str) -> Result<Response<'a>, Error> {
        self.set_header("Content-Length", body.len().to_string());
        self.send_stream(&mut body.as_bytes()).await
    }

    #[cfg(feature = "json")]
    pub async fn send_json<'a>(&mut self, body: &serde_json::Value) -> Result<Response<'a>, Error> {
        let body = body.to_string();
        self.set_header("Content-Length", body.len().to_string());
        self.send_stream(&mut body.as_bytes()).await
    }

    pub async fn send_http<'a, R>(&mut self, body: &mut R) -> Result<Response<'a>, Error>
        where
        R: Read + Unpin
    {
        let mut stream = self.build_conn().await?;
        self.write_request(&mut stream, body).await?;
        self.build_response(stream).await
    }

    pub async fn send_https<'a, R>(&mut self, body: &mut R) -> Result<Response<'a>, Error>
        where
        R: Read + Unpin
    {
        let stream = self.build_conn().await?;
        let mut stream = match async_native_tls::connect(self.host(), stream).await {
            Ok(stream) => stream,
            Err(_) => return Err(Error::UnableToConnect),
        };
        self.write_request(&mut stream, body).await?;
        self.build_response(stream).await
    }

    fn update_host_header(&mut self) {
        if self.version >= Version::Http1_1 && !self.has_header("Host") {
            self.set_header("Host", self.host_with_port());
        }
    }

    fn update_body_headers(&mut self) {
        if self.version >= Version::Http0_9 && self.method.has_body() && !self.has_header("Content-Length") {
            self.set_header("Transfer-Encoding", "chunked");
        }
    }

    async fn write_request<S, R>(&self, stream: &mut S, body: &mut R) -> Result<(), Error>
        where
        S: Write + Unpin,
        R: Read + Unpin,
    {
        self.write_proto(stream).await?;
        self.write_body(stream, body).await
    }

    async fn write_proto<S>(&self, stream: &mut S) -> Result<(), Error>
        where
        S: Write + Unpin,
    {
        match write_slice(stream, self.to_string().as_bytes()).await {
            Ok(_) => (),
            Err(e) => return Err(Error::try_from(e).unwrap()),
        };
        match flush_write(stream).await {
            Ok(_) => (),
            Err(e) => return Err(Error::try_from(e).unwrap()),
        };

        Ok(())
    }

    async fn write_body<S, R>(&self, stream: &mut S, body: &mut R) -> Result<(), Error>
        where
        S: Write + Unpin,
        R: Read + Unpin,
    {
        if self.has_version(Version::Http0_9) {
            match write_all(stream, body, self.body_limit).await {
                Ok(_) => (),
                Err(e) => return Err(Error::try_from(e).unwrap()),
            };
        } else if self.has_header("Content-Length") { // exact
            match write_exact(stream, body, read_content_length(&self.headers, self.body_limit)?).await {
                Ok(_) => (),
                Err(e) => return Err(Error::try_from(e).unwrap()),
            };
        } else { // chunked
            match write_chunks(stream, body, (Some(1024), self.body_limit)).await {
                Ok(_) => (),
                Err(e) => return Err(Error::try_from(e).unwrap()),
            };
        }
        match flush_write(stream).await {
            Ok(_) => Ok(()),
            Err(e) => return Err(Error::try_from(e).unwrap()),
        }
    }

    async fn build_conn(&mut self) -> Result<Stream, Error> {
        let addr = match SocketAddr::from_str(self.socket_address()).await {
            Ok(addr) => addr,
            Err(_) => return Err(Error::UnableToConnect),
        };
        let stream = match Stream::connect(&addr).await {
            Ok(stream) => stream,
            Err(_) => return Err(Error::UnableToConnect),
        };
        Ok(stream)
    }

    async fn build_response<'a, S>(&mut self, mut stream: S) -> Result<Response<'a>, Error>
        where
        S: Read + Unpin + 'a,
    {
        let mut res: Response<'a> = Response::default();

        let (mut version, mut status, mut message) = (vec![], vec![], vec![]);
        match read_first_line(&mut stream, (&mut version, &mut status, &mut message), None).await {
            Ok(_) => (),
            Err(e) => return Err(Error::try_from(e).unwrap()),
        };
        res.set_version(match parse_version(version) {
            Ok(version) => version,
            Err(e) => return Err(Error::try_from(e).unwrap()),
        });
        res.set_status(match parse_status(status) {
            Ok(status) => status,
            Err(e) => return Err(Error::try_from(e).unwrap()),
        });
    
        loop {
            let (mut name, mut value) = (vec![], vec![]);
            match read_header(&mut stream, (&mut name, &mut value), None).await {
                Ok(_) => match !name.is_empty() {
                    true => {
                        res.set_header(
                            match String::from_utf8(name) {
                                Ok(name) => name,
                                Err(_) => return Err(Error::InvalidHeader),
                            },
                            match String::from_utf8(value) {
                                Ok(value) => value,
                                Err(_) => return Err(Error::InvalidHeader),
                            },
                        );
                    },
                    false => break,
                },
                Err(e) => return Err(Error::try_from(e).unwrap()),
            };
        }

        res.set_reader(stream);

        Ok(res)
    }
}

impl fmt::Display for Request {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.to_proto_string())
    }
}

impl From<Request> for String {
    fn from(item: Request) -> String {
        item.to_string()
    }
}
