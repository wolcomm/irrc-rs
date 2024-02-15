use std::error::Error;

use ip::{traits::PrefixSet as _, Any, Prefix, PrefixSet};
use irrc::{IrrClient, Query};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init()?;
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
                tracing::error!("{err}");
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
                tracing::error!("{err}");
                None
            }
        })
        .collect::<PrefixSet<Any>>()
        .ranges()
        .for_each(|range| println!("{range}"));
    Ok(())
}
