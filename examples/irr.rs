extern crate simple_logger;

use simple_logger::SimpleLogger;

use irrc::{IrrClient, QueryResult};

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    irr.as_set_members("AS37271:AS-CUSTOMERS")?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    irr.ipv4_routes("AS37271")?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    irr.ipv6_routes("AS37271")?
        .into_iter()
        .for_each(|item| println!("{}", item.content()));
    Ok(())
}
