extern crate simple_logger;

use std::env::args;
use std::error::Error;
use std::panic;
use std::sync::mpsc;
use std::thread;

use prefixset::{IpPrefix, Ipv4Prefix, Ipv6Prefix, PrefixSet};
use simple_logger::SimpleLogger;

use irrc::{IrrClient, Query, QueryResult, ResponseItem};

struct CollectorSender<P>(mpsc::Sender<P>);

impl<P: IpPrefix> CollectorSender<P> {
    fn collect(&self, item: ResponseItem) {
        match item.content().parse() {
            Ok(prefix) => {
                if let Err(err) = self.0.send(prefix) {
                    log::warn!("failed to send prefix to collector: {}", err);
                }
            }
            Err(err) => log::warn!("failed to parse prefix: {}", err),
        }
    }
}

impl<P> From<mpsc::Sender<P>> for CollectorSender<P> {
    fn from(tx: mpsc::Sender<P>) -> Self {
        Self(tx)
    }
}

struct CollectorHandle<P: IpPrefix>(thread::JoinHandle<PrefixSet<P>>);

impl<P: IpPrefix> CollectorHandle<P> {
    fn print(self) {
        match self.0.join() {
            Ok(set) => set.ranges().for_each(|range| println!("{}", range)),
            Err(err) => log::error!("failed to join set builder thread: {:?}", err),
        }
    }
}

impl<P> From<mpsc::Receiver<P>> for Box<CollectorHandle<P>>
where
    P: 'static + IpPrefix + Send,
    P::Bits: Send,
{
    fn from(rx: mpsc::Receiver<P>) -> Self {
        Box::new(CollectorHandle(thread::spawn(move || {
            rx.iter()
                .inspect(|prefix| log::debug!("adding prefix {} to prefix set", prefix))
                .collect::<PrefixSet<_>>()
        })))
    }
}

struct Collector<P: IpPrefix>(CollectorSender<P>, Box<CollectorHandle<P>>);

impl<P> Collector<P>
where
    P: 'static + IpPrefix + Send,
    P::Bits: Send,
{
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        Self(tx.into(), rx.into())
    }
}

fn log_warning<E: Error>(err: E) -> E {
    log::warn!("failed to parse item: {}", err);
    err
}

fn into_routes_queries(item: ResponseItem) -> [Query; 2] {
    [
        Query::Ipv4Routes(item.content().to_string()),
        Query::Ipv6Routes(item.content().to_string()),
    ]
}

fn main() -> QueryResult<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Error)
        .init()
        .unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].clone();
    let (Collector(tx_ipv4, jh_ipv4), Collector(tx_ipv6, jh_ipv6)) = (
        Collector::<Ipv4Prefix>::spawn(),
        Collector::<Ipv6Prefix>::spawn(),
    );
    let query_thread = thread::spawn(move || -> QueryResult<()> {
        IrrClient::new(host)
            .connect()?
            .pipeline_from_initial(Query::AsSetMembersRecursive(object), |item| {
                item.map(into_routes_queries).map_err(log_warning).ok()
            })?
            .responses()
            .filter_map(|item| item.map_err(log_warning).ok())
            .for_each(|item| match item.query() {
                Query::Ipv4Routes(_) => tx_ipv4.collect(item),
                Query::Ipv6Routes(_) => tx_ipv6.collect(item),
                _ => unreachable!(),
            });
        Ok(())
    });
    match query_thread.join() {
        Ok(result) => result?,
        Err(err) => panic::resume_unwind(err),
    };
    jh_ipv4.print();
    jh_ipv6.print();
    Ok(())
}
