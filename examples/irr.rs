use std::{
    error::Error,
    fmt::{Debug, Display},
    str::FromStr,
};

use ip::{Ipv4, Ipv6, Prefix};
use irrc::{IrrClient, Query, ResponseItem};
use rpsl::expr::AsSetMember;
use simple_logger::SimpleLogger;

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init()?;
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    irr.pipeline()
        .push(Query::AsSetMembers("AS37271:AS-CUSTOMERS".parse()?))?
        .responses::<AsSetMember>()
        .for_each(hanndle_item_result);
    irr.pipeline()
        .push(Query::Ipv4Routes("AS37271".parse()?))?
        .responses::<Prefix<Ipv4>>()
        .for_each(hanndle_item_result);
    irr.pipeline()
        .push(Query::Ipv6Routes("AS37271".parse()?))?
        .responses::<Prefix<Ipv6>>()
        .for_each(hanndle_item_result);
    Ok(())
}

fn hanndle_item_result<T, E>(result: Result<ResponseItem<T>, E>)
where
    T: FromStr + Debug + Display,
    T::Err: Error + Send + Sync,
    E: Error + Send + Sync,
{
    match result {
        Ok(item) => println!("{}", item.content()),
        Err(err) => log::error!("{err}"),
    }
}
