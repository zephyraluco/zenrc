//! zenrc-cli — CycloneDDS 命令行工具。
//!
//! 参考 cyclonedds-python 实现的 ls/ps/typeof/subscribe/publish/performance 命令。

mod commands;
mod discovery;
mod topics;
mod type_schema;

use clap::{Parser, Subcommand};

use commands::performance::{PerfMode, PerfOptions};

/// CycloneDDS 命令行工具（zenrc-cli）
#[derive(Parser)]
#[command(
    name = "zenrc-cli",
    about = "CycloneDDS CLI — discover and interact with DDS networks",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 扫描并显示 DDS 网络中的实体（参与者、发布者、订阅者）
    Ls {
        /// DDS 域 ID（省略则使用默认域）
        #[arg(short = 'i', long = "domain-id")]
        domain_id: Option<u32>,
        /// 扫描持续时间（秒）
        #[arg(short = 'r', long = "runtime", default_value = "1.0")]
        runtime: f64,
        /// 按 topic 名称正则过滤（空字符串表示不过滤）
        #[arg(short = 't', long = "topic", default_value = "")]
        topic: String,
        /// 显示端点 GUID 信息
        #[arg(short = 'q', long = "qos", default_value = "false")]
        show_qos: bool,
    },

    /// 扫描并按参与者聚合 DDS 应用信息
    Ps {
        /// DDS 域 ID
        #[arg(short = 'i', long = "domain-id")]
        domain_id: Option<u32>,
        /// 扫描持续时间（秒）
        #[arg(short = 'r', long = "runtime", default_value = "1.0")]
        runtime: f64,
        /// 按 topic 名称正则过滤
        #[arg(short = 't', long = "topic", default_value = "")]
        topic: String,
    },

    /// 查询指定 topic 的类型名称
    Typeof {
        /// topic 名称
        topic: String,
        /// DDS 域 ID
        #[arg(short = 'i', long = "domain-id")]
        domain_id: Option<u32>,
        /// 扫描持续时间（秒）
        #[arg(short = 'r', long = "runtime", default_value = "2.0")]
        runtime: f64,
    },

    /// 订阅指定 topic 并输出收到的 CDR 数据（十六进制转储）
    Subscribe {
        /// topic 名称
        topic: String,
        /// DDS 域 ID
        #[arg(short = 'i', long = "domain-id")]
        domain_id: Option<u32>,
        /// 发现阶段扫描时间（秒）
        #[arg(short = 'r', long = "runtime", default_value = "2.0")]
        runtime: f64,
    },

    /// 向指定 topic 发布原始 CDR 数据（从 stdin 读取十六进制字节串）
    Publish {
        /// topic 名称
        topic: String,
        /// DDS 域 ID
        #[arg(short = 'i', long = "domain-id")]
        domain_id: Option<u32>,
        /// 发现阶段扫描时间（秒）
        #[arg(short = 'r', long = "runtime", default_value = "2.0")]
        runtime: f64,
    },

    /// 运行 DDS 性能测试（封装 ddsperf 工具）
    Performance {
        #[command(subcommand)]
        subcommand: PerfSubcommand,
    },
}

#[derive(Subcommand)]
enum PerfSubcommand {
    /// 延迟测试 — 发送 ping
    Ping {
        /// 发送速率（如 "100Hz"、"10000/s"）
        #[arg(short = 'r', long = "rate")]
        rate: Option<String>,
        /// 负载大小（字节，仅 KS topic 有效）
        #[arg(short = 's', long = "size")]
        size: Option<u64>,
        #[command(flatten)]
        opts: PerfArgs,
    },
    /// 延迟测试 — 响应 pong
    Pong {
        #[command(flatten)]
        opts: PerfArgs,
    },
    /// 吞吐量测试 — 发布端
    Pub {
        /// 发送速率
        #[arg(short = 'r', long = "rate")]
        rate: Option<String>,
        /// 负载大小（字节）
        #[arg(short = 's', long = "size")]
        size: Option<u64>,
        #[command(flatten)]
        opts: PerfArgs,
    },
    /// 吞吐量测试 — 订阅端
    Sub {
        #[command(flatten)]
        opts: PerfArgs,
    },
    /// 列出 ddsperf 支持的 topic 类型
    Topics,
}

#[derive(clap::Args, Debug, Default, Clone)]
struct PerfArgs {
    /// DDS 域 ID
    #[arg(short = 'i', long = "domain-id")]
    domain_id: Option<u32>,
    /// Topic 类型（KS/K32/K256/OU/UK16/UK1024/S16/S256/S4k/S32k）
    #[arg(short = 'T', long = "topic", default_value = "KS")]
    topic: Option<String>,
    /// key 值数量
    #[arg(short = 'n', long = "num-keys")]
    num_keys: Option<u32>,
    /// 使用 best-effort（不可靠）
    #[arg(short = 'u', long = "unreliable")]
    unreliable: bool,
    /// history keep（all 或 N）
    #[arg(short = 'k', long = "keep")]
    keep: Option<String>,
    /// 持续时间（秒）
    #[arg(short = 'D', long = "duration")]
    duration: Option<f64>,
    /// 允许进程内匹配
    #[arg(short = 'L', long = "local-matching")]
    local_matching: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ls { domain_id, runtime, topic, show_qos } => {
            commands::ls::run(domain_id, runtime, &topic, show_qos)
        }
        Commands::Ps { domain_id, runtime, topic } => {
            commands::ps::run(domain_id, runtime, &topic)
        }
        Commands::Typeof { topic, domain_id, runtime } => {
            commands::r#typeof::run(domain_id, runtime, &topic)
        }
        Commands::Subscribe { topic, domain_id, runtime } => {
            commands::subscribe::run(domain_id, runtime, &topic)
        }
        Commands::Publish { topic, domain_id, runtime } => {
            commands::publish::run(domain_id, runtime, &topic)
        }
        Commands::Performance { subcommand } => {
            match subcommand {
                PerfSubcommand::Ping { rate, size, opts } => {
                    commands::performance::run(
                        PerfMode::Ping { rate, size },
                        perf_opts(opts),
                    )
                }
                PerfSubcommand::Pong { opts } => {
                    commands::performance::run(PerfMode::Pong, perf_opts(opts))
                }
                PerfSubcommand::Pub { rate, size, opts } => {
                    commands::performance::run(
                        PerfMode::Publish { rate, size },
                        perf_opts(opts),
                    )
                }
                PerfSubcommand::Sub { opts } => {
                    commands::performance::run(PerfMode::Subscribe, perf_opts(opts))
                }
                PerfSubcommand::Topics => {
                    commands::performance::print_topics();
                    Ok(())
                }
            }
        }
    }
}

fn perf_opts(a: PerfArgs) -> PerfOptions {
    PerfOptions {
        domain_id: a.domain_id,
        topic: a.topic,
        num_keys: a.num_keys,
        unreliable: a.unreliable,
        keep: a.keep,
        duration: a.duration,
        local_matching: a.local_matching,
    }
}
