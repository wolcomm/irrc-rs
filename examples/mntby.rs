use std::{error::Error, io::stderr};

use irrc::{IrrClient, Query};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(stderr)
        .try_init()?;
    IrrClient::new("whois.radb.net:43")
        .connect()?
        .pipeline()
        .push(Query::MntBy("WORKONLINE-MNT".parse()?))?
        .responses::<String>()
        .filter_map(|item_result| {
            item_result
                .map_err(|err| tracing::warn!("failed to parse item: {}", err))
                .ok()
        })
        .for_each(|item| {
            println!("## START ##\n{}\n## END ##", item.content());
        });
    Ok(())
}
