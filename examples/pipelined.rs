use std::convert::{TryFrom, TryInto};
use std::env::args;
use std::error::Error;
use std::fmt;
use std::panic;
use std::sync::mpsc;
use std::thread;

use ipnet::IpNet;
use irrc::{IrrClient, Query, QueryResult, ResponseItem};
use prefixset::{IpPrefix, Ipv4Prefix, Ipv6Prefix, PrefixSet};
use rpsl::names::AutNum;
use simple_logger::SimpleLogger;

struct CollectorSender<P>(mpsc::Sender<P>);

impl<P> CollectorSender<P>
where
    P: IpPrefix + TryFrom<IpNet>,
    <P as TryFrom<IpNet>>::Error: fmt::Display,
{
    fn collect(&self, item: ResponseItem<IpNet>) {
        match item.into_content().try_into() {
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

fn into_routes_queries(item: ResponseItem<AutNum>) -> [Query; 2] {
    [
        Query::Ipv4Routes(*item.content()),
        Query::Ipv6Routes(*item.content()),
    ]
}

fn main() -> QueryResult<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].parse().unwrap();
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
