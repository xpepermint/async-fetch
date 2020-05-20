use async_fetch::{Request, Method, Status, Version};

#[async_std::test]
async fn performs_get_request() {
    let mut req = Request::parse_url("https://jsonplaceholder.typicode.com/users/1").unwrap();

    let mut res = req.send().await.unwrap();
    assert_eq!(*res.status(), Status::Ok);
    assert_eq!(*res.version(), Version::Http1_1);

    let data = res.recv().await.unwrap();
    assert_eq!(data.len(), 509);
}

#[async_std::test]
async fn performs_post_request() {
    let mut req = Request::parse_url("https://jsonplaceholder.typicode.com/users").unwrap();
    req.set_method(Method::Post);

    let mut res = req.send_data(&mut "".as_bytes()).await.unwrap();
    assert_eq!(*res.status(), Status::Created);
    assert_eq!(*res.version(), Version::Http1_1);

    let data = res.recv().await.unwrap();
    assert_eq!(data.len(), 14);
}
