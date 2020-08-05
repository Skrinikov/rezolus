// Copyright 2019-2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

// A simple tool for tracking various scheduler perf events.

#include <linux/ptrace.h>
#include <uapi/linux/bpf_perf_event.h>

// Currently supported events:
// > scheduler/cpu_migrations - cpu migrations count

// Change key type if you need more granular counters.
#define KEY u8
#define KEY_DEFAULT_INIT 0

// Update later with the key values you need.
static inline __attribute__((always_inline)) void get_key(KEY *key) {
    *key = 0;
}

// Add more as needed.
BPF_HASH(cpu_migrations, KEY); 
int f_cpu_migrations(struct bpf_perf_event_data *ctx) { 
    KEY key = KEY_DEFAULT_INIT; 
    get_key(&key); 
    cpu_migrations.increment(key); return 0; 
}