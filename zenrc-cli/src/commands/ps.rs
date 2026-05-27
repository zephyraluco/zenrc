//! `zenrc-cli ps` — 扫描并按应用维度聚合 DDS 参与者。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;

use crate::discovery::{fmt_guid, scan_network};

pub fn run(domain_id: Option<u32>, runtime_secs: f64, topic: &str) -> Result<()> {
    let duration = Duration::from_secs_f64(runtime_secs);
    eprintln!("Scanning DDS applications for {:.1}s...", runtime_secs);

    let result = scan_network(domain_id, duration, topic)?;

    if result.participants.is_empty() {
        println!("No DDS participants found.");
        return Ok(());
    }

    // 统计每个参与者的 writers/readers
    let mut writers_count: HashMap<String, usize> = HashMap::new();
    let mut readers_count: HashMap<String, usize> = HashMap::new();
    let mut topics_map: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    for ep in &result.publications {
        let key = fmt_guid(&ep.participant_guid);
        *writers_count.entry(key.clone()).or_default() += 1;
        topics_map.entry(key).or_default().insert(ep.topic_name.clone());
    }
    for ep in &result.subscriptions {
        let key = fmt_guid(&ep.participant_guid);
        *readers_count.entry(key.clone()).or_default() += 1;
        topics_map.entry(key).or_default().insert(ep.topic_name.clone());
    }

    println!("{:<40}  {:>8}  {:>8}  Topics", "Participant GUID", "Writers", "Readers");
    println!("{}", "-".repeat(80));

    for p in &result.participants {
        let guid = fmt_guid(&p.guid);
        let nw = writers_count.get(&guid).copied().unwrap_or(0);
        let nr = readers_count.get(&guid).copied().unwrap_or(0);
        let topics: Vec<_> = topics_map
            .get(&guid)
            .map(|s| {
                let mut v: Vec<_> = s.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default();

        println!("{:<40}  {:>8}  {:>8}  {}", guid, nw, nr, topics.join(", "));
    }

    println!();
    println!("Total: {} participant(s)", result.participants.len());

    Ok(())
}
