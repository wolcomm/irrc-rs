use irrc::{IrrClient, QueryResult};
use simple_logger::SimpleLogger;

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    irr.as_set_members("AS37271:AS-CUSTOMERS".parse().unwrap())?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    irr.ipv4_routes("AS37271".parse().unwrap())?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    irr.ipv6_routes("AS37271".parse().unwrap())?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    Ok(())
}
