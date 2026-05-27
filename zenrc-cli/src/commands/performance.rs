//! `zenrc-cli performance` — 封装 `ddsperf` 工具运行性能测试。
//!
//! 子命令: ping, pong, pub (publish), sub (subscribe)

use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

/// 性能测试模式
#[derive(Debug, Clone)]
pub enum PerfMode {
    Ping {
        rate: Option<String>,
        size: Option<u64>,
    },
    Pong,
    Publish {
        rate: Option<String>,
        size: Option<u64>,
    },
    Subscribe,
}

/// 全局性能参数
#[derive(Debug, Clone, Default)]
pub struct PerfOptions {
    pub domain_id: Option<u32>,
    pub topic: Option<String>,
    pub num_keys: Option<u32>,
    pub unreliable: bool,
    pub keep: Option<String>,
    pub duration: Option<f64>,
    pub local_matching: bool,
}

/// 运行 ddsperf 子命令
pub fn run(mode: PerfMode, opts: PerfOptions) -> Result<()> {
    // 查找 ddsperf 可执行文件
    let ddsperf = find_ddsperf()?;

    let mut cmd = Command::new(&ddsperf);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // 全局选项
    if let Some(id) = opts.domain_id {
        cmd.args(["-i", &id.to_string()]);
    }
    if let Some(ref t) = opts.topic {
        cmd.args(["-T", t]);
    }
    if let Some(n) = opts.num_keys {
        cmd.args(["-n", &n.to_string()]);
    }
    if opts.unreliable {
        cmd.arg("-u");
    }
    if let Some(ref k) = opts.keep {
        cmd.args(["-k", k]);
    }
    if let Some(d) = opts.duration {
        cmd.args(["-D", &format!("{}", d as u64)]);
    }
    if opts.local_matching {
        cmd.arg("-L");
    }

    // 子命令
    match mode {
        PerfMode::Ping { rate, size } => {
            cmd.arg("ping");
            if let Some(r) = rate {
                cmd.args(["ping", &r]);
                // 子命令前已添加 ping，这里放速率参数
                // 实际 ddsperf 的 ping 参数格式: ping [<rate>] [<size>]
            }
            if let Some(s) = size {
                cmd.arg(s.to_string());
            }
        }
        PerfMode::Pong => {
            cmd.arg("pong");
        }
        PerfMode::Publish { rate, size } => {
            cmd.arg("pub");
            if let Some(r) = rate {
                cmd.arg(r);
            }
            if let Some(s) = size {
                cmd.arg(format!("{}b", s));
            }
        }
        PerfMode::Subscribe => {
            cmd.arg("sub");
        }
    }

    eprintln!("Running: {} {:?}", ddsperf, cmd.get_args().collect::<Vec<_>>());

    let exit_status = cmd.status().context("Failed to launch ddsperf")?;
    if !exit_status.success() {
        return Err(anyhow!("ddsperf exited with: {}", exit_status));
    }
    Ok(())
}

/// 打印已知的性能测试 topic 及其格式
pub fn print_topics() {
    println!("ddsperf topic types:");
    println!("  KS       - Key+sequence payload (default)");
    println!("  K32      - Key + 32-byte payload");
    println!("  K256     - Key + 256-byte payload");
    println!("  OU       - Only unreliable (no key)");
    println!("  UK16     - No key, 16-byte payload");
    println!("  UK1024   - No key, 1024-byte payload");
    println!("  S16      - Seq, 16 bytes");
    println!("  S256     - Seq, 256 bytes");
    println!("  S4k      - Seq, 4096 bytes");
    println!("  S32k     - Seq, 32768 bytes");
}

fn find_ddsperf() -> Result<String> {
    // 优先使用 PATH 中的 ddsperf
    for candidate in &["ddsperf", "/usr/local/bin/ddsperf", "/opt/ros/jazzy/bin/ddsperf"] {
        if Command::new(candidate).arg("--help").output().is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Err(anyhow!(
        "ddsperf not found. Install CycloneDDS and ensure ddsperf is in PATH."
    ))
}
