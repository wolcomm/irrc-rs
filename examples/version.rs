use simple_logger::SimpleLogger;

use irrc::{IrrClient, Query, QueryResult};

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    println!(
        "{:?}",
        IrrClient::new("whois.radb.net:43")
            .connect()?
            .pipeline()
            .push(Query::Version)?
            .pop()
            .unwrap()?
            .next()
    );
    Ok(())
}
