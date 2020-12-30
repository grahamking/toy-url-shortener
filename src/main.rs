use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::RwLock;

use http::response;
use http::status::StatusCode;
use serde_derive::Deserialize;

use remote::add_client::AddClient;
use remote::add_server::{Add, AddServer};
use remote::{AddUrlRequest, AddUrlResponse};
use tonic::{transport::Server, Request, Response, Status};

type Db = Arc<RwLock<Main>>;

use warp::Filter;
#[tokio::main]
async fn main() {
    // Args are out port then remote port
    let mut args = env::args().skip(1);
    let port: u16 = match args.next() {
        None => {
            eprintln!("missing port as first param");
            return;
        }
        Some(p) => p.parse().unwrap(),
    };
    let remote_port: u16 = match args.next() {
        None => {
            eprintln!("missing remote port as second param");
            return;
        }
        Some(p) => p.parse().unwrap(),
    };

    let main = Arc::new(RwLock::new(Main::new(remote_port)));
    let closure_main = Arc::clone(&main);
    let with_main = warp::any().map(move || Arc::clone(&closure_main));
    let get = warp::get()
        .and(with_main.clone())
        .and(warp::path::param())
        .and_then(do_get);

    let post = warp::post()
        .and(warp::path("create"))
        .and(warp::path::end())
        .and(with_main.clone())
        .and(warp::query::<CreateOpts>())
        .and_then(do_post);

    let routes = get.or(post);

    let rpc_main = Arc::clone(&main);
    let grpc_port = port + 1;
    tokio::spawn(async move {
        let remote = Remote { main: rpc_main };
        let addr = SocketAddr::from(([127, 0, 0, 1], grpc_port));
        Server::builder()
            .add_service(AddServer::new(remote))
            .serve(addr)
            .await
            .unwrap();
    });

    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

#[derive(Debug)]
struct Main {
    remote_addr: String,
    db: HashMap<String, String>,
}

impl Main {
    fn new(rp: u16) -> Self {
        let mut addr = String::from("http://127.0.0.1:");
        addr.push_str(&rp.to_string());
        Self {
            remote_addr: addr,
            db: HashMap::new(),
        }
    }

    async fn replicate(&self, hash: &str, url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = AddClient::connect(self.remote_addr.clone()).await?;
        let request = tonic::Request::new(AddUrlRequest {
            hash: hash.into(),
            url: url.into(),
        });
        let _ = client.add_url(request).await?; // TODO: confirm success response
        Ok(())
    }
}

async fn do_get(main: Db, short_url: String) -> Result<impl warp::Reply, Infallible> {
    let val = {
        let rlock = main.read().await;
        rlock.db.get(&short_url).cloned()
    };
    let resp = match val {
        None => response::Response::builder().status(404).body("").unwrap(),
        Some(val) => response::Response::builder()
            .status(301)
            .header("Location", String::from("http://") + &val)
            .body("")
            .unwrap(),
    };
    Ok(resp)
}

#[derive(Debug, Deserialize)]
struct CreateOpts {
    url: String,
}

async fn do_post(main: Db, opts: CreateOpts) -> Result<impl warp::Reply, Infallible> {
    let url = match opts.url.strip_prefix("http://") {
        Some(u) => u,
        None => &opts.url,
    };
    let resp = response::Response::builder();
    let mut hs = DefaultHasher::new();
    url.hash(&mut hs);
    let mut enc = base62num::encode(hs.finish() as usize);
    match main.read().await.db.get(&enc) {
        Some(current) => {
            if current == url {
                // already exists
                return Ok(resp.status(StatusCode::OK).body(enc).unwrap());
            } else {
                // conflict
                enc.push('X'); // TODO try until we find unused hash
            }
        }
        None => {}
    }
    main.write().await.db.insert(enc.clone(), url.into());

    main.read().await.replicate(&enc, &url).await.unwrap();

    println!("Created: {} -> {}", enc, url);
    let resp = resp.status(StatusCode::CREATED).body(enc).unwrap();
    Ok(resp)
}

#[derive(Debug)]
struct Remote {
    main: Db,
}

#[tonic::async_trait]
impl Add for Remote {
    async fn add_url(
        &self,
        request: Request<AddUrlRequest>,
    ) -> Result<Response<AddUrlResponse>, Status> {
        let req = request.into_inner();
        self.main
            .write()
            .await
            .db
            .insert(req.hash.clone(), req.url.clone());
        println!("rCreated: {} -> {}", req.hash, req.url);

        let reply = remote::AddUrlResponse {
            message: format!("Got {}:{}", req.hash, req.url),
        };
        Ok(Response::new(reply))
    }
}

// gRPC to/from remote server
pub mod remote {
    tonic::include_proto!("short");
}
