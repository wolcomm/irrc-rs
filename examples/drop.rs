use std::{error::Error, io::stderr};

use irrc::{IrrClient, Query};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(stderr)
        .try_init()?;
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    let autnum = "AS37271".parse().unwrap();
    let mut pipeline = irr.pipeline();
    pipeline
        .push(Query::Ipv4Routes(autnum))?
        .push(Query::Ipv6Routes(autnum))?;
    while let Some(resp_result) = pipeline.pop::<String>() {
        if let Ok(mut resp) = resp_result {
            dbg!(resp.next());
        }
    }
    Ok(())
}
