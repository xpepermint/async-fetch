> Asynchronous HTTP client.

This crate provides a simple HTTP client and is built on top of [async-std](https://github.com/async-rs/async-std).

* Fully streaming requests and responses
* TLS/SSL enabled by default
* Relay through HTTP proxy or UNIX socket

**Example:**

```rs
async_fetch::Request;

let mut req = Request::parse_url("https://domain.com").unwrap();
let mut res = req.send().await?;
let data = res.recv().await?;
```
