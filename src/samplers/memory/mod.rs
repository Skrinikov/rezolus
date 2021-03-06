// Copyright 2019-2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use std::collections::HashMap;

use async_trait::async_trait;
use regex::Regex;
use rustcommon_metrics::*;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::common::*;
use crate::config::SamplerConfig;
use crate::samplers::Common;
use crate::Sampler;

mod config;
mod stat;

pub use config::*;
pub use stat::*;

use stat::MemoryStatistic as Stat;

#[allow(dead_code)]
pub struct Memory {
    common: Common,
}

#[async_trait]
impl Sampler for Memory {
    type Statistic = MemoryStatistic;

    fn new(common: Common) -> Result<Self, failure::Error> {
        Ok(Self { common })
    }

    fn spawn(common: Common) {
        if let Ok(mut sampler) = Self::new(common.clone()) {
            common.handle.spawn(async move {
                loop {
                    let _ = sampler.sample().await;
                }
            });
        } else if !common.config.fault_tolerant() {
            fatal!("failed to initialize memory sampler");
        } else {
            error!("failed to initialize memory sampler");
        }
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn common_mut(&mut self) -> &mut Common {
        &mut self.common
    }

    fn sampler_config(&self) -> &dyn SamplerConfig<Statistic = Self::Statistic> {
        self.common.config().samplers().memory()
    }

    async fn sample(&mut self) -> Result<(), std::io::Error> {
        if let Some(ref mut delay) = self.delay() {
            delay.tick().await;
        }

        if !self.sampler_config().enabled() {
            return Ok(());
        }

        debug!("sampling");
        self.register();

        self.map_result(self.sample_meminfo().await)?;
        self.map_result(self.sample_vmstat().await)?;

        Ok(())
    }

    fn summary(&self, _statistic: &Self::Statistic) -> Option<Summary> {
        Some(Summary::histogram(
            TEBIBYTE,
            3,
            Some(self.general_config().window()),
        ))
    }
}

impl Memory {
    async fn sample_meminfo(&self) -> Result<(), std::io::Error> {
        let file = File::open("/proc/meminfo").await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut result = HashMap::<MemoryStatistic, u64>::new();

        let re = Regex::new(r"(?P<stat>\w+):\s+(?P<value>\d+)").expect("failed to compile regex");

        while let Some(line) = lines.next_line().await? {
            if let Some(caps) = re.captures(&line) {
                if let Some(Ok(value)) = caps.name("value").map(|v| v.as_str().parse()) {
                    if let Some(Some(stat)) = caps.name("stat").map(|v| match v.as_str() {
                        "MemTotal" => Some(Stat::Total),
                        "MemFree" => Some(Stat::Free),
                        "MemAvailable" => Some(Stat::Available),
                        "Buffers" => Some(Stat::Buffers),
                        "Cached" => Some(Stat::Cached),
                        "SwapCached" => Some(Stat::SwapCached),
                        "Active" => Some(Stat::Active),
                        "Inactive" => Some(Stat::Inactive),
                        "Active(anon)" => Some(Stat::ActiveAnon),
                        "Inactive(anon)" => Some(Stat::InactiveAnon),
                        "Unevictable" => Some(Stat::Unevictable),
                        "Mlocked" => Some(Stat::Mlocked),
                        "SwapTotal" => Some(Stat::SwapTotal),
                        "SwapFree" => Some(Stat::SwapFree),
                        "Dirty" => Some(Stat::Dirty),
                        "Writeback" => Some(Stat::Writeback),
                        "AnonPages" => Some(Stat::AnonPages),
                        "Mapped" => Some(Stat::Mapped),
                        "Shmem" => Some(Stat::Shmem),
                        "Slab" => Some(Stat::SlabTotal),
                        "SReclaimable" => Some(Stat::SlabReclaimable),
                        "SUnreclaim" => Some(Stat::SlabUnreclaimable),
                        "KernelStack" => Some(Stat::KernelStack),
                        "PageTables" => Some(Stat::PageTables),
                        "NFS_Unstable" => Some(Stat::NFSUnstable),
                        "Bounce" => Some(Stat::Bounce),
                        "WritebackTmp" => Some(Stat::WritebackTmp),
                        "CommitLimit" => Some(Stat::CommitLimit),
                        "Committed_AS" => Some(Stat::CommittedAS),
                        "VmallocTotal" => Some(Stat::VmallocTotal),
                        "VmallocUsed" => Some(Stat::VmallocUsed),
                        "VmallocChunk" => Some(Stat::VmallocChunk),
                        "HardwareCorrupted" => Some(Stat::HardwareCorrupted),
                        "AnonHugePages" => Some(Stat::AnonHugePages),
                        "ShmemHugePages" => Some(Stat::ShmemHugePages),
                        "ShmemPmdMapped" => Some(Stat::ShmemPmdMapped),
                        "HugePages_Total" => Some(Stat::HugePagesTotal),
                        "HugePages_Free" => Some(Stat::HugePagesFree),
                        "HugePages_Rsvd" => Some(Stat::HugePagesRsvd),
                        "HugePages_Surp" => Some(Stat::HugePagesSurp),
                        "Hugepagesize" => Some(Stat::Hugepagesize),
                        "Hugetlb" => Some(Stat::Hugetlb),
                        "DirectMap4k" => Some(Stat::DirectMap4k),
                        "DirectMap2M" => Some(Stat::DirectMap2M),
                        "DirectMap1G" => Some(Stat::DirectMap1G),
                        _ => None,
                    }) {
                        result.insert(stat, value);
                    }
                }
            }
        }

        let time = time::precise_time_ns();
        for stat in self.sampler_config().statistics() {
            if let Some(value) = result.get(stat) {
                self.metrics()
                    .record_gauge(stat, time, *value * stat.multiplier());
            }
        }
        Ok(())
    }

    async fn sample_vmstat(&self) -> Result<(), std::io::Error> {
        let file = File::open("/proc/vmstat").await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut result = HashMap::<MemoryStatistic, u64>::new();

        let re = Regex::new(r"(?P<stat>\w+)\s+(?P<value>\d+)").expect("failed to compile regex");

        while let Some(line) = lines.next_line().await? {
            if let Some(caps) = re.captures(&line) {
                if let Some(Ok(value)) = caps.name("value").map(|v| v.as_str().parse()) {
                    if let Some(Some(stat)) = caps.name("stat").map(|v| match v.as_str() {
                        "numa_hit" => Some(Stat::NumaHit),
                        "numa_miss" => Some(Stat::NumaMiss),
                        "numa_foreign" => Some(Stat::NumaForeign),
                        "numa_interleave" => Some(Stat::NumaInterleave),
                        "numa_local" => Some(Stat::NumaLocal),
                        "numa_other" => Some(Stat::NumaOther),
                        "thp_fault_alloc" => Some(Stat::ThpFaultAlloc),
                        "thp_fault_fallback" => Some(Stat::ThpFaultFallback),
                        "thp_collapse_alloc" => Some(Stat::ThpCollapseAlloc),
                        "thp_collapse_alloc_failed" => Some(Stat::ThpCollapseAllocFailed),
                        "thp_split_page" => Some(Stat::ThpSplitPage),
                        "thp_split_page_failed" => Some(Stat::ThpSplitPageFailed),
                        "thp_deferred_split_page" => Some(Stat::ThpDeferredSplitPage),
                        "compact_migrate_scanned" => Some(Stat::CompactMigrateScanned),
                        "compact_free_scanned" => Some(Stat::CompactFreeScanned),
                        "compact_isolated" => Some(Stat::CompactIsolated),
                        "compact_stall" => Some(Stat::CompactStall),
                        "compact_fail" => Some(Stat::CompactFail),
                        "compact_success" => Some(Stat::CompactSuccess),
                        "compact_daemon_wake" => Some(Stat::CompactDaemonWake),
                        "compact_daemon_migrate_scanned" => Some(Stat::CompactDaemonMigrateScanned),
                        "compact_daemon_free_scanned" => Some(Stat::CompactDaemonFreeScanned),
                        _ => None,
                    }) {
                        result.insert(stat, value);
                    }
                }
            }
        }

        let time = time::precise_time_ns();
        for stat in self.sampler_config().statistics() {
            if let Some(value) = result.get(stat) {
                if stat.source() == Source::Counter {
                    self.metrics()
                        .record_counter(stat, time, *value * stat.multiplier());
                } else {
                    self.metrics()
                        .record_gauge(stat, time, *value * stat.multiplier());
                }
            }
        }
        Ok(())
    }
}
