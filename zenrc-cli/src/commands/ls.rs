//! `zenrc-cli ls` — 扫描并显示 DDS 网络中的实体。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;

use crate::discovery::{fmt_guid, scan_network};

pub fn run(domain_id: Option<u32>, runtime_secs: f64, topic: &str, show_qos: bool) -> Result<()> {
    let duration = Duration::from_secs_f64(runtime_secs);
    eprintln!("Scanning DDS network for {:.1}s...", runtime_secs);

    let result = scan_network(domain_id, duration, topic)?;

    if result.participants.is_empty() && result.publications.is_empty() && result.subscriptions.is_empty() {
        println!("No DDS entities found.");
        return Ok(());
    }

    // 按参与者聚合端点
    let mut pubs_by_participant: HashMap<String, Vec<_>> = HashMap::new();
    let mut subs_by_participant: HashMap<String, Vec<_>> = HashMap::new();

    for ep in &result.publications {
        let key = fmt_guid(&ep.participant_guid);
        pubs_by_participant.entry(key).or_default().push(ep);
    }
    for ep in &result.subscriptions {
        let key = fmt_guid(&ep.participant_guid);
        subs_by_participant.entry(key).or_default().push(ep);
    }

    // 按参与者输出
    let mut printed = std::collections::HashSet::new();

    for p in &result.participants {
        let guid_str = fmt_guid(&p.guid);
        printed.insert(guid_str.clone());

        println!("┌─ Participant: {}", guid_str);

        if let Some(pubs) = pubs_by_participant.get(&guid_str) {
            println!("│  Publishers ({}):", pubs.len());
            for ep in pubs {
                println!("│    topic: {}  type: {}", ep.topic_name, ep.type_name);
                if show_qos {
                    println!("│      guid: {}", fmt_guid(&ep.guid));
                }
            }
        }

        if let Some(subs) = subs_by_participant.get(&guid_str) {
            println!("│  Subscribers ({}):", subs.len());
            for ep in subs {
                println!("│    topic: {}  type: {}", ep.topic_name, ep.type_name);
                if show_qos {
                    println!("│      guid: {}", fmt_guid(&ep.guid));
                }
            }
        }

        println!("└──");
        println!();
    }

    // 输出没有对应参与者记录的端点（可能参与者样本尚未到达）
    let orphan_pub_keys: Vec<_> = pubs_by_participant
        .keys()
        .filter(|k| !printed.contains(*k))
        .cloned()
        .collect();
    let orphan_sub_keys: Vec<_> = subs_by_participant
        .keys()
        .filter(|k| !printed.contains(*k))
        .cloned()
        .collect();

    for key in &orphan_pub_keys {
        println!("┌─ Participant (unknown): {}", key);
        if let Some(pubs) = pubs_by_participant.get(key) {
            println!("│  Publishers ({}):", pubs.len());
            for ep in pubs {
                println!("│    topic: {}  type: {}", ep.topic_name, ep.type_name);
            }
        }
        println!("└──");
        println!();
    }

    for key in &orphan_sub_keys {
        if orphan_pub_keys.contains(key) {
            continue; // 已在上面输出
        }
        println!("┌─ Participant (unknown): {}", key);
        if let Some(subs) = subs_by_participant.get(key) {
            println!("│  Subscribers ({}):", subs.len());
            for ep in subs {
                println!("│    topic: {}  type: {}", ep.topic_name, ep.type_name);
            }
        }
        println!("└──");
        println!();
    }

    let np = result.participants.len();
    let nw = result.publications.len();
    let nr = result.subscriptions.len();
    println!("Summary: {} participant(s), {} writer(s), {} reader(s)", np, nw, nr);

    Ok(())
}
