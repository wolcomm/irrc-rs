use std::collections::HashMap;
use std::env::args;

use simple_logger::SimpleLogger;

use irrc::{IrrClient, Query, QueryResult};
use prefixset::{Ipv4Prefix, Ipv6Prefix, PrefixSet};

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].clone();
    let mut irr = IrrClient::new(host).connect()?;
    let mut pipeline = irr.pipeline();
    pipeline.push(Query::AsSetMembersRecursive(object))?;
    let mut autnums: HashMap<String, (PrefixSet<Ipv4Prefix>, PrefixSet<Ipv6Prefix>)> = pipeline
        .pop()
        .unwrap()?
        .filter_map(|item| {
            item.map_err(|err| {
                log::warn!("{}", err);
                err
            })
            .map(|item| {
                (
                    item.content().to_string(),
                    (PrefixSet::new(), PrefixSet::new()),
                )
            })
            .ok()
        })
        .collect();
    pipeline.extend(
        autnums
            .keys()
            .map(|k| {
                [
                    Query::Ipv4Routes(k.to_owned()),
                    Query::Ipv6Routes(k.to_owned()),
                ]
            })
            .flatten(),
    );
    while let Some(response_result) = pipeline.pop() {
        match response_result {
            Ok(response) => match response.query() {
                Query::Ipv4Routes(autnum) => {
                    autnums.entry(autnum.to_string()).and_modify(|(set, _)| {
                        set.extend(response.filter_map(|item| {
                            item.map_err(|err| {
                                log::warn!("error: {}", err);
                                err
                            })
                            .map(|item| item.content().parse::<Ipv4Prefix>().expect("should parse"))
                            .ok()
                        }))
                    });
                }
                Query::Ipv6Routes(autnum) => {
                    autnums.entry(autnum.to_string()).and_modify(|(_, set)| {
                        set.extend(response.filter_map(|item| {
                            item.map_err(|err| {
                                log::warn!("error: {}", err);
                                err
                            })
                            .map(|item| item.content().parse::<Ipv6Prefix>().expect("should parse"))
                            .ok()
                        }))
                    });
                }
                _ => unreachable!(),
            },
            Err(err) => {
                log::warn!("query failed: {}", err);
            }
        }
    }
    autnums
        .into_iter()
        .for_each(|(autnum, (ipv4_set, ipv6_set))| {
            println!("{}:", autnum);
            println!("  ipv4 prefixes: {}", ipv4_set.prefixes().count());
            println!("  ipv6 prefixes: {}", ipv6_set.prefixes().count());
        });
    Ok(())
}
