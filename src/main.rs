mod cache;
mod date;
mod models;
mod nasa;
mod server;
mod service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let address = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("NASA Asteroid Tracker is available at http://{address}");
    axum::serve(listener, server::router()).await?;
    Ok(())
}
