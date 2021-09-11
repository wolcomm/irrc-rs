use std::error::Error;

use irrc::{IrrClient, Query, QueryResult, ResponseItem};
use simple_logger::SimpleLogger;

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    IrrClient::new("whois.radb.net:43")
        .connect()?
        .pipeline()
        .push(Query::MntBy("WORKONLINE-MNT".parse()?))?
        .responses()
        .filter_map(|item_result: QueryResult<ResponseItem<String>>| {
            item_result
                .map_err(|err| log::warn!("failed to parse item: {}", err))
                .ok()
        })
        .for_each(|item| {
            println!("## START ##\n{}\n## END ##", item.content());
        });
    Ok(())
}
