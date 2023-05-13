use irrc::{Error, IrrClient};
use simple_logger::SimpleLogger;

fn main() -> Result<(), Error> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();
    let (host, port) = ("whois.radb.net", 43);
    println!(
        "connected to '{}', running '{}'",
        host,
        IrrClient::new(format!("{}:{}", host, port))
            .connect()?
            .version()?
    );
    Ok(())
}
