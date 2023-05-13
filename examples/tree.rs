use std::collections::HashMap;
use std::env::args;

use ip::{traits::PrefixSet as _, Any, Ipv4, Ipv6, Prefix, PrefixSet};
use irrc::{IrrClient, Query};
use rpsl::names::AutNum;
use simple_logger::SimpleLogger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::new().init().unwrap();
    let args: Vec<String> = args().collect();
    let host = format!("{}:43", args[1]);
    let object = args[2].parse().unwrap();
    let mut irr = IrrClient::new(host).connect()?;
    let mut pipeline = irr.pipeline();
    pipeline.push(Query::AsSetMembersRecursive(object))?;
    let mut autnums: HashMap<AutNum, _> = pipeline
        .pop()
        .unwrap()?
        .filter_map(|item| {
            item.map_err(|err| {
                log::warn!("{}", err);
                err
            })
            .map(|item| {
                (
                    item.into_content(),
                    (PrefixSet::<Ipv4>::default(), PrefixSet::<Ipv6>::default()),
                )
            })
            .ok()
        })
        .collect();
    pipeline.extend(
        autnums
            .keys()
            .flat_map(|k| [Query::Ipv4Routes(*k), Query::Ipv6Routes(*k)]),
    );
    while let Some(response_result) = pipeline.pop::<Prefix<Any>>() {
        match response_result {
            Ok(response) => match response.query() {
                Query::Ipv4Routes(autnum) => {
                    autnums.entry(*autnum).and_modify(|(set, _)| {
                        set.extend(response.filter_map::<Prefix<Ipv4>, _>(|item_result| {
                            item_result
                                .map(|item| item.into_content().try_into().unwrap())
                                .map_err(|err| log::warn!("error: {}", err))
                                .ok()
                        }))
                    });
                }
                Query::Ipv6Routes(autnum) => {
                    autnums.entry(*autnum).and_modify(|(_, set)| {
                        set.extend(response.filter_map::<Prefix<Ipv6>, _>(|item_result| {
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
