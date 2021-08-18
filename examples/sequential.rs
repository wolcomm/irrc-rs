extern crate simple_logger;

use simple_logger::SimpleLogger;

use irrc::{IrrClient, QueryResult};

fn main() -> QueryResult<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    irr.as_set_members("AS37271:AS-CUSTOMERS")?
        .into_iter()
        .filter_map(|item| {
            let autnum = item.content();
            match irr.ipv4_routes(autnum) {
                Ok(routes) => Some(routes),
                Err(err) => {
                    log::error!("getting ipv4 routes for {} failed: {}", autnum, err);
                    None
                }
            }
        })
        .flatten()
        .for_each(|item| println!("{}", item.content()));
    Ok(())
}
