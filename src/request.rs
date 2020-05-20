use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::convert::TryFrom;
use url::{Url, Position};
use async_std::io::{Read, Write};
use async_uninet::{SocketAddr, Stream};
use async_httplib::{read_first_line, parse_version, parse_status, read_header,
    write_all, flush_write, write_exact, write_chunks};
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
        req.set_url(match Url::parse(&url.into()) {
            Ok(url) => url,
            Err(_) => return Err(Error::InvalidUrl),
        });
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

    pub fn has_method(&mut self, value: Method) -> bool {
        self.method == value
    }

    pub fn has_version(&mut self, value: Version) -> bool {
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
    
    pub fn set_method(&mut self, value: Method) {
        self.method = value;
    }

    pub fn set_version(&mut self, value: Version) {
        self.version = value;
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
        match self.scheme() {
            "http" => self.send_http(&mut "".as_bytes()).await,
            "https" => self.send_https(&mut "".as_bytes()).await,
            _ => Err(Error::InvalidUrl),
        }
    }

    pub async fn send_data<'a, R>(&mut self, body: &mut R) -> Result<Response<'a>, Error>
        where
        R: Read + Unpin,
    {
        match self.scheme() {
            "http" => self.send_http(body).await,
            "https" => self.send_https(body).await,
            _ => Err(Error::InvalidUrl),
        }
    }

    pub async fn send_http<'a, R>(&mut self, body: &mut R) -> Result<Response<'a>, Error>
        where
        R: Read + Unpin
    {
        let mut stream = self.build_conn().await?;
        self.update_headers();
        self.write_headers(&mut stream).await?;
        self.write_body(&mut stream, body).await?;
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
        self.update_headers();
        self.write_headers(&mut stream).await?;
        self.write_body(&mut stream, body).await?;
        self.build_response(stream).await
    }

    fn update_headers(&mut self) {
        if self.version == Version::Http0_9 {
            self.clear_headers();
        } else if self.version >= Version::Http1_1 && !self.has_header("Host") {
            self.set_header("Host", self.host_with_port());
        } else if self.method.has_body() && !self.has_header("Content-Length") {
            self.set_header("Transfer-Encoding", "chunked");
        }
    }

    async fn write_headers<S>(&self, stream: &mut S) -> Result<(), Error>
        where
        S: Write + Unpin,
    {
        match write_all(stream, self.to_string().as_bytes()).await {
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
        if self.has_header("Content-Length") {
            match write_exact(stream, body, read_content_length(&self.headers, self.body_limit)?).await {
                Ok(_) => (),
                Err(e) => return Err(Error::try_from(e).unwrap()),
            };
        } else {
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
