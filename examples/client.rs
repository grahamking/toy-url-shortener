#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();

    // create a URL
    let resp = client
        .post("http://localhost:3030/create?url=www.darkcoding.net")
        .send()
        .await?;
    let short_url = resp.text().await?;

    let mut url = String::from("http://localhost:3030/");
    url.push_str(&short_url);
    let body = reqwest::get(&url).await?.text().await?;
    println!("{}", &body[..306]);

    Ok(())
}
