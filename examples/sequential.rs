use std::error::Error;

use ip::{traits::PrefixSet as _, Any, Prefix, PrefixSet};
use irrc::{IrrClient, Query};
use simple_logger::SimpleLogger;

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()?;
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    let route_queries: Vec<_> = irr
        .pipeline()
        .push(Query::AsSetMembersRecursive(
            "AS37271:AS-CUSTOMERS".parse()?,
        ))?
        .responses()
        .filter_map(|result| match result {
            Ok(item) => Some([
                Query::Ipv4Routes(*item.content()),
                Query::Ipv6Routes(*item.content()),
            ]),
            Err(err) => {
                log::error!("{err}");
                None
            }
        })
        .flatten()
        .collect();
    irr.pipeline_from_iter(route_queries)
        .responses::<Prefix<Any>>()
        .filter_map(|result| match result {
            Ok(item) => Some(item.into_content()),
            Err(err) => {
                log::error!("{err}");
                None
            }
        })
        .collect::<PrefixSet<Any>>()
        .ranges()
        .for_each(|range| println!("{range}"));
    Ok(())
}
