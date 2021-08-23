use irrc::{IrrClient, QueryResult};
use simple_logger::SimpleLogger;

fn main() -> QueryResult<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    irr.as_set_members("AS37271:AS-CUSTOMERS".parse().unwrap())?
        .into_iter()
        .filter_map(|item| {
            let autnum = match item.content().parse() {
                Ok(autnum) => autnum,
                Err(err) => {
                    log::error!("failed to parse aut-num: {}", err);
                    return None;
                }
            };
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
