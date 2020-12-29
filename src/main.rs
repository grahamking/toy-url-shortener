use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;

use http::response::Response;
use http::status::StatusCode;
use serde_derive::Deserialize;

type Db = Arc<RwLock<HashMap<String, String>>>;

use warp::Filter;
#[tokio::main]
async fn main() {
    let db = create_db();
    let with_db = warp::any().map(move || Arc::clone(&db));
    let get = warp::get()
        .and(with_db.clone())
        .and(warp::path::param())
        .and_then(do_get);

    let post = warp::post()
        .and(warp::path("create"))
        .and(warp::path::end())
        .and(with_db.clone())
        .and(warp::query::<CreateOpts>())
        .and_then(do_post);

    let routes = get.or(post);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn create_db() -> Arc<RwLock<HashMap<String, String>>> {
    Arc::new(RwLock::new(HashMap::new()))
}

async fn do_get(db: Db, short_url: String) -> Result<impl warp::Reply, Infallible> {
    let val = {
        let rlock = db.read().await;
        rlock.get(&short_url).cloned()
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
    let resp = Response::builder();
    let mut hs = DefaultHasher::new();
    url.hash(&mut hs);
    let mut enc = base62num::encode(hs.finish() as usize);
    match db.read().await.get(&enc) {
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
    db.write().await.insert(enc.clone(), url.into());
    println!("Created: {} -> {}", url, enc);
    let resp = resp.status(StatusCode::CREATED).body(enc).unwrap();
    Ok(resp)
}
