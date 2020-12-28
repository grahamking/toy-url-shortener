use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use http::response::Response;
use http::status::StatusCode;
use serde_derive::Deserialize;

type Db = Arc<Mutex<HashMap<String, String>>>;

use warp::Filter;
#[tokio::main]
async fn main() {
    let db = create_db();
    db.lock().unwrap().insert("Bob".into(), "Says yes".into());

    let get = warp::get()
        .and(with_db(db.clone()))
        .and(warp::path::param())
        .and_then(do_get);

    let post = warp::post()
        .and(warp::path("create"))
        .and(with_db(db.clone()))
        .and(warp::query::<CreateOpts>())
        .and_then(do_post);

    let routes = get.or(post);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn create_db() -> Arc<Mutex<HashMap<String, String>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

async fn do_get(db: Db, short_url: String) -> Result<impl warp::Reply, Infallible> {
    let val = {
        let lock = db.lock().unwrap();
        lock.get(&short_url).cloned()
    };
    let resp = match val {
        None => Response::builder().status(404).body("").unwrap(),
        Some(val) => Response::builder()
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

async fn do_post(db: Db, opts: CreateOpts) -> Result<impl warp::Reply, Infallible> {
    let url = match opts.url.strip_prefix("http://") {
        Some(u) => u,
        None => &opts.url,
    };
    let enc = base_62::encode(url.as_bytes());
    db.lock().unwrap().insert(enc.clone(), url.into());
    println!("Created: {} -> {}", url, enc);
    let resp = Response::builder()
        .status(StatusCode::CREATED)
        .body(enc)
        .unwrap();
    Ok(resp)
}

fn with_db(db: Db) -> impl Filter<Extract = (Db,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}
