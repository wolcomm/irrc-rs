use std::{error::Error, io::stderr};

use irrc::IrrClient;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(stderr)
        .try_init()?;
    let (host, port) = ("whois.radb.net", 43);
    println!(
        "connected to '{}', running '{}'",
        host,
        IrrClient::new(format!("{}:{}", host, port))
            .connect()?
            .version()?
    );
    Ok(())
}
