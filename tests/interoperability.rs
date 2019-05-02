use hawk::{Credentials, Header, Key, PayloadHasher, RequestBuilder, ResponseBuilder, SHA256};
use hyper;
use hyper::rt::{self, Future, Stream};
use hyper::{header, Body, Client, Request, StatusCode};
//use hyper_hawk::{HawkScheme, ServerAuthorization};
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command};
use std::str::FromStr;
use url::Url;

fn start_node_server() -> (Child, u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", 0)).unwrap();
    let callback_port = listener.local_addr().unwrap().port();

    // check for `node_modules' first
    let path = Path::new("tests/node/node_modules");
    if !path.is_dir() {
        panic!(
            "Run `yarn` or `npm install` in tests/node, or test with --features \
             no-interoperability"
        );
    }

    let child = Command::new("node")
        .arg("serve-one.js")
        .arg(format!("{}", callback_port))
        .current_dir("tests/node")
        .spawn()
        .expect("node command failed to start");

    // wait until the process is ready, signalled by a connect to the callback port, and then
    // return the port it provides. We know this will only get one connection, but iteration
    // is easier anyway
    #[cfg_attr(feature = "cargo-clippy", allow(never_loop))]
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();

        let mut data: Vec<u8> = vec![];
        stream.read_to_end(&mut data).unwrap();
        let port = u16::from_str(std::str::from_utf8(&data).unwrap()).unwrap();

        drop(stream);
        return (child, port);
    }
    unreachable!();
}

fn make_credentials() -> Credentials {
    Credentials {
        id: "dh37fgj492je".to_string(),
        key: Key::new("werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn", &SHA256),
    }
}

#[cfg_attr(feature = "no-interoperability", ignore)]
#[test]
fn client_with_header() {
    let (mut child, port) = start_node_server();
    let mut test_passed = false;

    // NOTE: this closure is run in another thread, so assert! and friends are right out.
    // consider using tokio current_thread runtime with block_on
    rt::run(rt::lazy(move || {
        let credentials = make_credentials();
        let url = Url::parse(&format!("http://localhost:{}/resource", port)).unwrap();
        let body = "foo=bar";

        // build a hawk::Request
        let payload_hash = PayloadHasher::hash(b"text/plain", &SHA256, body.as_bytes());
        let hawk_req = RequestBuilder::from_url("POST", &url)
            .unwrap()
            .hash(&payload_hash[..])
            .ext("ext-content")
            .request();

        // build a hyper::Request
        let req_header = hawk_req.make_header(&credentials).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri(url.as_str())
            // TODO: implement something to convert this to HeaderValue?
            .header(header::AUTHORIZATION, format!("Hawk {}", req_header))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(body))
            .unwrap();
        println!("Request: {:?}", req);

        let client = Client::new();
        client
            .request(req)
            .and_then(move |res| {
                println!("Response: {}", res.status());
                println!("Headers: {:#?}", res.headers());
                // TODO: check StatusCode
                let sa_header = res
                    .headers()
                    .get("server-authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                let hasher = PayloadHasher::new(b"text/plain", &SHA256);
                res.into_body()
                    .fold(
                        hasher,
                        |mut hasher, chunk| -> Result<PayloadHasher, hyper::Error> {
                            hasher.update(&chunk);
                            Ok(hasher)
                        },
                    )
                    .map(move |hasher| {
                        let hash = hasher.finish();
                        println!("hash: {:?}", hash);
                        println!("s-a header: {}", sa_header);
                        // TODO: check body content too?
                        // TODO: hm, can't access request here
                        let response = ResponseBuilder::from_request_header(
                            &req_header,
                            "POST",
                            "localhostXXXXXXXXXXXXXXX",
                            port,
                            "/resource",
                        )
                        .hash(&hash[..])
                        .response();
                        let server_header = Header::from_str(&sa_header[5..]).unwrap();
                        test_passed = response.validate_header(&server_header, &credentials.key);
                    })
            })
            .map_err(|err| {
                println!("Error {}", err);
            })
    }));

    child.wait().expect("Failure waiting for child");

    assert!(test_passed, "test should have passed");
}

/*
#[cfg_attr(feature = "no-interoperability", ignore)]
#[test]
fn client_with_bewit() {
    let (mut child, port) = start_node_server();

    let credentials = make_credentials();
    let url = Url::parse(&format!("http://localhost:{}/resource", port)).unwrap();
    let hawk_req = RequestBuilder::from_url("GET", &url)
        .unwrap()
        .ext("ext-content")
        .request();

    let bewit = hawk_req
        .make_bewit(&credentials, SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    let mut url = url.clone();
    url.set_query(Some(&format!("bewit={}", bewit.to_str())));

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let client = Client::new(&handle);
    let work = client.get(url.as_str().parse().unwrap()).and_then(|res| {
        assert_eq!(res.status(), hyper::Ok);

        res.body().concat2().map(|body| {
            assert_eq!(body.as_ref(), b"Hello Steve ext-content");
        })
    });

    core.run(work).unwrap();

    // drop everything to allow the client connection to close and thus the Node server
    // to exit.  Curiously, just dropping client is not sufficient - the core holds the
    // socket open.
    drop(client);
    drop(handle);
    drop(core);

    child.wait().expect("Failure waiting for child");
}
*/
