use std::error::Error;

use irrc::{IrrClient, Query};
use simple_logger::SimpleLogger;

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    IrrClient::new("whois.radb.net:43")
        .connect()?
        .pipeline()
        .push(Query::MntBy("WORKONLINE-MNT".parse()?))?
        .responses::<String>()
        .filter_map(|item_result| {
            item_result
                .map_err(|err| log::warn!("failed to parse item: {}", err))
                .ok()
        })
        .for_each(|item| {
            println!("## START ##\n{}\n## END ##", item.content());
        });
    Ok(())
}
