use async_std::task;
use async_fetch::{Request, Method, Status, Version};

#[async_std::test]
async fn performs_get_request() {
    let mut req = Request::parse_url("http://dummy.restapiexample.com/api/v1/employees").unwrap();

    let mut res = req.send().await.unwrap();
    assert_eq!(*res.status(), Status::Ok);
    assert_eq!(*res.version(), Version::Http1_1);

    let data = res.recv().await.unwrap();
    assert!(data.len() > 0);
}

#[async_std::test]
async fn performs_post_request() {
    let mut req = Request::parse_url("http://dummy.restapiexample.com/api/v1/create").unwrap();
    req.set_method(Method::Post);
    req.set_header("Content-Type", "application/json");

    let data = serde_json::json!({ "name": "John" });
    let mut res = req.send_json(&data).await.unwrap();
    assert_eq!(*res.status(), Status::Ok);
    assert_eq!(*res.version(), Version::Http1_1);

    let data = res.recv_json().await.unwrap();
    let name = data.get("data").unwrap().get("name").unwrap();
    let name: String = serde_json::from_str(&name.to_string()).unwrap();
    assert_eq!(name, "John");
}

#[async_std::test]
async fn respects_spawning() {
    task::spawn(async move {
        let mut req = Request::parse_url("https://jsonplaceholder.typicode.com/todos/1").unwrap();
        let res = req.send().await.unwrap();
        assert_eq!(*res.status(), Status::Ok);
    });   
}
