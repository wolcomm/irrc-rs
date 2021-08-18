use simple_logger::SimpleLogger;

use irrc::{IrrClient, Query, QueryResult};

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    let mut irr = IrrClient::new("whois.radb.net:43").connect()?;
    let mut pipeline = irr.pipeline();
    pipeline
        .push(Query::Ipv4Routes("AS37271".to_string()))?
        .push(Query::Ipv6Routes("AS37271".to_string()))?;
    while let Some(resp_result) = pipeline.pop() {
        if let Ok(mut resp) = resp_result {
            dbg!(resp.next());
        }
    }
    Ok(())
}
