use std::collections::HashMap;
use std::convert::TryInto;
use std::env::args;

use ipnet::IpNet;
use irrc::{IrrClient, Query, QueryResult};
use prefixset::{Ipv4Prefix, Ipv6Prefix, PrefixSet};
use rpsl::names::AutNum;
use simple_logger::SimpleLogger;

fn main() -> QueryResult<()> {
    SimpleLogger::new().init().unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].parse().unwrap();
    let mut irr = IrrClient::new(host).connect()?;
    let mut pipeline = irr.pipeline();
    pipeline.push(Query::AsSetMembersRecursive(object))?;
    let mut autnums: HashMap<AutNum, (PrefixSet<Ipv4Prefix>, PrefixSet<Ipv6Prefix>)> = pipeline
        .pop()
        .unwrap()?
        .filter_map(|item| {
            item.map_err(|err| {
                log::warn!("{}", err);
                err
            })
            .map(|item| (item.into_content(), (PrefixSet::new(), PrefixSet::new())))
            .ok()
        })
        .collect();
    pipeline.extend(
        autnums
            .keys()
            .map(|k| [Query::Ipv4Routes(*k), Query::Ipv6Routes(*k)])
            .flatten(),
    );
    while let Some(response_result) = pipeline.pop::<IpNet>() {
        match response_result {
            Ok(response) => match response.query() {
                Query::Ipv4Routes(autnum) => {
                    autnums.entry(*autnum).and_modify(|(set, _)| {
                        set.extend(response.filter_map::<Ipv4Prefix, _>(|item_result| {
                            item_result
                                .map(|item| item.into_content().try_into().unwrap())
                                .map_err(|err| log::warn!("error: {}", err))
                                .ok()
                        }))
                    });
                }
                Query::Ipv6Routes(autnum) => {
                    autnums.entry(*autnum).and_modify(|(_, set)| {
                        set.extend(response.filter_map::<Ipv6Prefix, _>(|item_result| {
                            item_result
                                .map(|item| item.into_content().try_into().unwrap())
                                .map_err(|err| log::warn!("error: {}", err))
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
