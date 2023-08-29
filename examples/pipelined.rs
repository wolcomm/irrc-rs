use std::env::args;
use std::sync::mpsc;
use std::thread;

use ip::{traits::PrefixSet as _, Any, Prefix, PrefixSet};
use irrc::{Error, IrrClient, Query, ResponseItem};
use rpsl::names::{AsSet, AutNum};
use simple_logger::SimpleLogger;

struct Collector {
    sender: Option<mpsc::Sender<Prefix<Any>>>,
    join_handle: thread::JoinHandle<PrefixSet<Any>>,
}

impl Collector {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let sender = Some(tx);
        log::debug!("starting collector thread");
        let join_handle = thread::spawn(move || {
            rx.iter()
                .inspect(|prefix| log::trace!("adding prefix {} to prefix set", prefix))
                .collect::<PrefixSet<Any>>()
        });
        Self {
            sender,
            join_handle,
        }
    }

    fn sender(&mut self) -> Option<Sender> {
        self.sender.take().map(Sender)
    }

    fn print(self) {
        log::debug!("trying to join collector thread");
        match self.join_handle.join() {
            Ok(set) => set.ranges().for_each(|range| println!("{}", range)),
            Err(err) => log::error!("failed to join set builder thread: {:?}", err),
        }
    }
}

#[derive(Clone)]
struct Sender(mpsc::Sender<Prefix<Any>>);

impl Sender {
    fn collect(&self, item: ResponseItem<Prefix<Any>>) {
        let prefix = item.into_content();
        log::trace!("sending prefix {prefix} to collector");
        if let Err(err) = self.0.send(prefix) {
            log::warn!("failed to send prefix to collector: {}", err);
        }
    }
}

struct QueryThread(thread::JoinHandle<Result<(), Error>>);

impl QueryThread {
    fn spawn(host: String, object: AsSet, collector: &mut Collector) -> Self {
        let sender = collector
            .sender()
            .expect("failed to take collector send handle");
        let join_handle = thread::spawn(move || -> Result<(), Error> {
            IrrClient::new(host)
                .connect()?
                .pipeline_from_initial(Query::AsSetMembersRecursive(object), |item| {
                    item.map(into_routes_queries).map_err(log_warning).ok()
                })?
                .responses()
                .filter_map(|item| item.map_err(log_warning).ok())
                .for_each(|item| sender.collect(item));
            Ok(())
        });
        Self(join_handle)
    }

    fn join(self) -> Result<(), Error> {
        self.0.join().expect("failed to join query thread")
    }
}

fn log_warning<E: std::error::Error>(err: E) -> E {
    log::warn!("failed to parse item: {}", err);
    err
}

fn into_routes_queries(item: ResponseItem<AutNum>) -> [Query; 2] {
    let autnum = item.into_content();
    [Query::Ipv4Routes(autnum), Query::Ipv6Routes(autnum)]
}

fn main() -> Result<(), Error> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].parse().unwrap();
    let mut collector = Collector::spawn();
    let query_thread = QueryThread::spawn(host, object, &mut collector);
    query_thread.join()?;
    collector.print();
    Ok(())
}
