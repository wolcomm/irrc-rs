use irrc::{Error, IrrClient, Query};
use simple_logger::SimpleLogger;

fn main() -> Result<(), Error> {
    SimpleLogger::new().init().unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    let autnum = "AS37271".parse().unwrap();
    let mut pipeline = irr.pipeline();
    pipeline
        .push(Query::Ipv4Routes(autnum))?
        .push(Query::Ipv6Routes(autnum))?;
    while let Some(resp_result) = pipeline.pop::<String>() {
        if let Ok(mut resp) = resp_result {
            dbg!(resp.next());
        }
    }
    Ok(())
}
