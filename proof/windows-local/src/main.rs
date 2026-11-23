//! Windows `Local` conversion proof harness.
//!
//! `profile` times the raw Windows timezone calls next to the end-to-end conversion.
//! `bench <label>` times end-to-end workloads through the public API.
//! `checksum <label>` hashes conversion results so two builds can be compared.

use std::hint::black_box;
use std::sync::Barrier;
use std::time::Instant;

use chrono::{DateTime, Local, MappedLocalTime, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};

const SAMPLES: usize = 15;
const BATCH: usize = 50_000;
const SET_LEN: usize = 4096;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn year_start(year: i32) -> i64 {
    NaiveDate::from_ymd_opt(year, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
}

fn repeated_year() -> Vec<DateTime<Utc>> {
    let start = year_start(2024);
    let span = year_start(2025) - start;
    (0..SET_LEN as i64)
        .map(|i| DateTime::from_timestamp(start + i * span / SET_LEN as i64 + 37 * i, 0).unwrap())
        .collect()
}

fn mixed_years() -> Vec<DateTime<Utc>> {
    let mut rng = Lcg(0x5eed);
    (0..SET_LEN)
        .map(|_| {
            let year = 1970 + (rng.next() % 90) as i32;
            let offset = (rng.next() % (365 * 86_400)) as i64;
            DateTime::from_timestamp(year_start(year) + offset, 0).unwrap()
        })
        .collect()
}

fn alternating_years() -> Vec<DateTime<Utc>> {
    let mut rng = Lcg(0xa17e);
    (0..SET_LEN)
        .map(|i| {
            let year = if i % 2 == 0 { 2023 } else { 2024 };
            let offset = (rng.next() % (365 * 86_400)) as i64;
            DateTime::from_timestamp(year_start(year) + offset, 0).unwrap()
        })
        .collect()
}

fn local_repeated_year() -> Vec<NaiveDateTime> {
    repeated_year().iter().map(|dt| dt.naive_utc()).collect()
}

fn stats(mut v: Vec<f64>) -> String {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    format!(
        "\"median_ns\":{:.2},\"p25_ns\":{:.2},\"p75_ns\":{:.2},\"min_ns\":{:.2},\"max_ns\":{:.2},\"samples\":{}",
        q(0.5),
        q(0.25),
        q(0.75),
        v[0],
        v[v.len() - 1],
        v.len()
    )
}

/// Runs `op(i)` `BATCH` times on each of `threads` threads and returns, for each sample, the time
/// from the first thread starting to the last thread finishing, divided by `BATCH`, in nanoseconds.
fn measure(threads: usize, op: &(dyn Fn(usize) + Sync)) -> Vec<f64> {
    let run = |batch: usize| {
        let barrier = Barrier::new(threads);
        let spans: Vec<(Instant, Instant)> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let barrier = &barrier;
                    s.spawn(move || {
                        barrier.wait();
                        let start = Instant::now();
                        for i in 0..batch {
                            op(i + t * 7);
                        }
                        (start, Instant::now())
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let start = spans.iter().map(|s| s.0).min().unwrap();
        let end = spans.iter().map(|s| s.1).max().unwrap();
        end - start
    };
    run(BATCH / 10);
    (0..SAMPLES).map(|_| run(BATCH).as_nanos() as f64 / BATCH as f64).collect()
}

/// Time of the first `Local::now()` on a fresh thread, in nanoseconds, for each sample.
fn measure_fresh_thread() -> Vec<f64> {
    const THREADS: u32 = 2000;
    (0..SAMPLES)
        .map(|_| {
            let total: std::time::Duration = (0..THREADS)
                .map(|_| {
                    std::thread::spawn(|| {
                        let start = Instant::now();
                        black_box(Local::now());
                        start.elapsed()
                    })
                    .join()
                    .unwrap()
                })
                .sum();
            total.as_nanos() as f64 / THREADS as f64
        })
        .collect()
}

fn wide_years() -> Vec<DateTime<Utc>> {
    let mut rng = Lcg(0x31de);
    (0..SET_LEN)
        .map(|_| {
            let year = 1601 + (rng.next() % 2400) as i32;
            let offset = (rng.next() % (365 * 86_400)) as i64;
            DateTime::from_timestamp(year_start(year) + offset, 0).unwrap()
        })
        .collect()
}

fn report(kind: &str, label: &str, name: &str, threads: usize, samples: Vec<f64>) {
    println!(
        "{{\"kind\":\"{kind}\",\"label\":\"{label}\",\"workload\":\"{name}\",\"threads\":{threads},{}}}",
        stats(samples)
    );
}

fn bench(label: &str) {
    let repeated = repeated_year();
    let mixed = mixed_years();
    let alternating = alternating_years();
    let local = local_repeated_year();
    let wide = wide_years();
    report("bench", label, "local_now_first_on_fresh_thread", 1, measure_fresh_thread());
    for threads in [1, 4] {
        report("bench", label, "utc_to_local_repeated_year", threads, measure(threads, &|i| {
            black_box(black_box(&repeated[i % SET_LEN]).with_timezone(&Local));
        }));
        report("bench", label, "utc_to_local_mixed_years", threads, measure(threads, &|i| {
            black_box(black_box(&mixed[i % SET_LEN]).with_timezone(&Local));
        }));
        report("bench", label, "utc_to_local_alternating_years", threads, measure(threads, &|i| {
            black_box(black_box(&alternating[i % SET_LEN]).with_timezone(&Local));
        }));
        report("bench", label, "utc_to_local_wide_years_cold", threads, measure(threads, &|i| {
            black_box(black_box(&wide[i % SET_LEN]).with_timezone(&Local));
        }));
        report("bench", label, "local_now", threads, measure(threads, &|_| {
            black_box(Local::now());
        }));
        report("bench", label, "local_to_utc_repeated_year", threads, measure(threads, &|i| {
            black_box(Local.from_local_datetime(black_box(&local[i % SET_LEN])));
        }));
        report("bench", label, "utc_now_control", threads, measure(threads, &|_| {
            black_box(Utc::now());
        }));
    }
}

struct Fnv(u64);

impl Fnv {
    fn write(&mut self, v: i64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

fn checksum(label: &str) {
    let mut utc_hash = Fnv(0xcbf29ce484222325);
    let mut local_hash = Fnv(0xcbf29ce484222325);
    let mut count = 0u64;
    let start = NaiveDate::from_ymd_opt(1975, 1, 1).unwrap().and_hms_opt(0, 30, 0).unwrap();
    let end = NaiveDate::from_ymd_opt(2078, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
    let mut dt = start;
    while dt < end {
        utc_hash.write(dt.and_utc().with_timezone(&Local).offset().local_minus_utc() as i64);
        match Local.from_local_datetime(&dt) {
            MappedLocalTime::Single(d) => local_hash.write(d.offset().local_minus_utc() as i64),
            MappedLocalTime::Ambiguous(a, b) => {
                local_hash.write(1 << 40);
                local_hash.write(a.offset().local_minus_utc() as i64);
                local_hash.write(b.offset().local_minus_utc() as i64);
            }
            MappedLocalTime::None => local_hash.write(2 << 40),
        }
        count += 1;
        dt += TimeDelta::minutes(30);
    }
    let mut mixed_hash = Fnv(0xcbf29ce484222325);
    let mut rng = Lcg(0xc0ffee);
    for _ in 0..200_000 {
        let ts = (rng.next() as i64 % (year_start(2100) - year_start(1900))) + year_start(1900);
        let dt = DateTime::from_timestamp(ts, 0).unwrap();
        mixed_hash.write(dt.with_timezone(&Local).offset().local_minus_utc() as i64);
    }
    println!(
        "{{\"kind\":\"checksum\",\"label\":\"{label}\",\"count\":{count},\"utc\":\"{:016x}\",\"local\":\"{:016x}\",\"mixed\":\"{:016x}\"}}",
        utc_hash.0, local_hash.0, mixed_hash.0
    );
}

#[cfg(windows)]
mod ffi {
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SystemTime {
        pub fields: [u16; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct TimeZoneInformation {
        pub bias: i32,
        pub standard_name: [u16; 32],
        pub standard_date: SystemTime,
        pub standard_bias: i32,
        pub daylight_name: [u16; 32],
        pub daylight_date: SystemTime,
        pub daylight_bias: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct DynamicTimeZoneInformation {
        pub tzi: TimeZoneInformation,
        pub key_name: [u16; 128],
        pub dynamic_daylight_time_disabled: u8,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetTimeZoneInformationForYear(
            year: u16,
            dtzi: *const DynamicTimeZoneInformation,
            tzi: *mut TimeZoneInformation,
        ) -> i32;
        pub fn GetDynamicTimeZoneInformation(dtzi: *mut DynamicTimeZoneInformation) -> u32;
        pub fn GetTimeZoneInformation(tzi: *mut TimeZoneInformation) -> u32;
    }
}

#[cfg(windows)]
fn profile() {
    use ffi::*;
    use std::mem::MaybeUninit;

    let label = "base";
    let repeated = repeated_year();
    let mut dtzi = MaybeUninit::<DynamicTimeZoneInformation>::zeroed();
    unsafe { GetDynamicTimeZoneInformation(dtzi.as_mut_ptr()) };
    let dtzi = unsafe { dtzi.assume_init() };
    let key: String = String::from_utf16_lossy(&dtzi.key_name).trim_end_matches('\0').to_owned();
    println!("{{\"kind\":\"timezone\",\"key\":\"{key}\",\"bias\":{}}}", dtzi.tzi.bias);

    for threads in [1, 4] {
        report("profile", label, "raw_GetTimeZoneInformationForYear_null_2024", threads, measure(threads, &|_| {
            let mut tzi = MaybeUninit::<TimeZoneInformation>::uninit();
            black_box(unsafe { GetTimeZoneInformationForYear(black_box(2024), std::ptr::null(), tzi.as_mut_ptr()) });
            black_box(tzi);
        }));
        report("profile", label, "raw_GetTimeZoneInformationForYear_null_mixed", threads, measure(threads, &|i| {
            let mut tzi = MaybeUninit::<TimeZoneInformation>::uninit();
            let year = 1970 + (i * 7919 % 90) as u16;
            black_box(unsafe { GetTimeZoneInformationForYear(black_box(year), std::ptr::null(), tzi.as_mut_ptr()) });
            black_box(tzi);
        }));
        report("profile", label, "raw_GetTimeZoneInformationForYear_dtzi_2024", threads, measure(threads, &|_| {
            let mut tzi = MaybeUninit::<TimeZoneInformation>::uninit();
            black_box(unsafe { GetTimeZoneInformationForYear(black_box(2024), &dtzi, tzi.as_mut_ptr()) });
            black_box(tzi);
        }));
        report("profile", label, "raw_GetDynamicTimeZoneInformation", threads, measure(threads, &|_| {
            let mut out = MaybeUninit::<DynamicTimeZoneInformation>::uninit();
            black_box(unsafe { GetDynamicTimeZoneInformation(out.as_mut_ptr()) });
            black_box(out);
        }));
        report("profile", label, "raw_GetTimeZoneInformation", threads, measure(threads, &|_| {
            let mut out = MaybeUninit::<TimeZoneInformation>::uninit();
            black_box(unsafe { GetTimeZoneInformation(out.as_mut_ptr()) });
            black_box(out);
        }));
        report("profile", label, "std_SystemTime_now", threads, measure(threads, &|_| {
            black_box(std::time::SystemTime::now());
        }));
        report("profile", label, "e2e_utc_to_local_repeated_year", threads, measure(threads, &|i| {
            black_box(black_box(&repeated[i % SET_LEN]).with_timezone(&Local));
        }));
    }
}

#[cfg(not(windows))]
fn profile() {
    panic!("profile mode needs Windows");
}

/// Switches the system timezone with `tzutil` while conversions run on two warmed threads, and
/// checks that each thread sees the new rules on its next conversion.
fn reconfig(label: &str) {
    let summer = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap().and_hms_opt(12, 0, 0).unwrap().and_utc();
    let winter = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap().and_hms_opt(12, 0, 0).unwrap().and_utc();
    let summer_local = summer.naive_utc();
    let cases: [(&str, i32, i32); 6] = [
        ("Pacific Standard Time", -7 * 3600, -8 * 3600),
        ("W. Europe Standard Time", 2 * 3600, 3600),
        ("Pacific Standard Time_dstoff", -8 * 3600, -8 * 3600),
        ("AUS Eastern Standard Time", 10 * 3600, 11 * 3600),
        ("UTC", 0, 0),
        ("Pacific Standard Time", -7 * 3600, -8 * 3600),
    ];
    let observe = || {
        let s = summer.with_timezone(&Local).offset().local_minus_utc();
        let w = winter.with_timezone(&Local).offset().local_minus_utc();
        let l = Local.from_local_datetime(&summer_local).earliest().map_or(i32::MIN, |d| d.offset().local_minus_utc());
        let n = Local::now().offset().local_minus_utc();
        (s, w, l, n)
    };
    let warm = || {
        for _ in 0..10_000 {
            black_box(summer.with_timezone(&Local));
            black_box(Local::now());
            black_box(Local.from_local_datetime(&summer_local));
        }
    };
    let mut failures = 0;
    let (to_worker, from_main) = std::sync::mpsc::channel::<()>();
    let (to_main, from_worker) = std::sync::mpsc::channel::<(i32, i32, i32, i32)>();
    std::thread::scope(|s| {
        s.spawn(move || {
            while from_main.recv().is_ok() {
                warm();
                to_main.send(observe()).unwrap();
                from_main.recv().unwrap();
                to_main.send(observe()).unwrap();
            }
        });
        for (zone, want_summer, want_winter) in cases {
            to_worker.send(()).unwrap();
            from_worker.recv().unwrap();
            warm();
            let status = std::process::Command::new("tzutil").args(["/s", zone]).status().unwrap();
            assert!(status.success(), "tzutil /s {zone} failed");
            let main_seen = observe();
            to_worker.send(()).unwrap();
            let worker_seen = from_worker.recv().unwrap();
            for (thread, (s, w, l, _now)) in [("main", main_seen), ("worker", worker_seen)] {
                let ok = s == want_summer && w == want_winter && l == want_summer;
                if !ok {
                    failures += 1;
                }
                println!(
                    "{{\"kind\":\"reconfig\",\"label\":\"{label}\",\"zone\":\"{zone}\",\"thread\":\"{thread}\",\"summer\":{s},\"winter\":{w},\"local\":{l},\"ok\":{ok}}}"
                );
            }
        }
        drop(to_worker);
    });
    assert_eq!(failures, 0, "live reconfiguration not observed");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("profile"), _) => profile(),
        (Some("bench"), Some(label)) => bench(label),
        (Some("checksum"), Some(label)) => checksum(label),
        (Some("reconfig"), Some(label)) => reconfig(label),
        _ => panic!("usage: windows-local-proof profile | bench <label> | checksum <label>"),
    }
}
