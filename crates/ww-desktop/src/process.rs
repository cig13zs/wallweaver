//! Memory and CPU use of the current process, for status displays and the
//! benchmark harness.

use std::time::Duration;

/// Resident set size (physical memory in use) of this process, in bytes.
pub fn rss_bytes() -> Option<u64> {
    imp::rss_bytes()
}

/// Total CPU time (user + system) this process has used so far.
pub fn cpu_time() -> Option<Duration> {
    imp::cpu_time()
}

#[cfg(target_os = "linux")]
mod imp {
    use std::time::Duration;

    pub fn rss_bytes() -> Option<u64> {
        // Second field of statm is resident pages.
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages * page_size())
    }

    fn page_size() -> u64 {
        // SAFETY: sysconf has no preconditions.
        let v = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if v > 0 { v as u64 } else { 4096 }
    }

    pub fn cpu_time() -> Option<Duration> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: usage points to writable memory of the right size.
        let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        // SAFETY: getrusage succeeded, so the struct is initialized.
        let u = unsafe { usage.assume_init() };
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, (t.tv_usec as u32) * 1000);
        Some(tv(u.ru_utime) + tv(u.ru_stime))
    }
}

#[cfg(target_vendor = "apple")]
mod imp {
    use std::time::Duration;

    pub fn rss_bytes() -> Option<u64> {
        let mut info = std::mem::MaybeUninit::<libc::mach_task_basic_info>::zeroed();
        let mut count = (std::mem::size_of::<libc::mach_task_basic_info>()
            / std::mem::size_of::<libc::natural_t>())
            as libc::mach_msg_type_number_t;
        // SAFETY: info and count describe a correctly sized output buffer.
        #[allow(deprecated)]
        let kr = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                info.as_mut_ptr() as libc::task_info_t,
                &mut count,
            )
        };
        if kr != 0 {
            return None;
        }
        // SAFETY: task_info succeeded and filled the struct.
        Some(unsafe { info.assume_init() }.resident_size)
    }

    pub fn cpu_time() -> Option<Duration> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: usage points to writable memory of the right size.
        let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        // SAFETY: getrusage succeeded.
        let u = unsafe { usage.assume_init() };
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, (t.tv_usec as u32) * 1000);
        Some(tv(u.ru_utime) + tv(u.ru_stime))
    }
}

#[cfg(windows)]
mod imp {
    use std::time::Duration;

    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    pub fn rss_bytes() -> Option<u64> {
        let mut c = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        // SAFETY: the pseudo handle needs no closing; c is correctly sized.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        ok.as_bool().then_some(c.WorkingSetSize as u64)
    }

    pub fn cpu_time() -> Option<Duration> {
        let (mut a, mut b, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: all out-pointers are valid for the call.
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u) }.ok()?;
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Some(Duration::from_nanos((ticks(k) + ticks(u)) * 100))
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple", windows)))]
mod imp {
    use std::time::Duration;

    pub fn rss_bytes() -> Option<u64> {
        None
    }

    pub fn cpu_time() -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_something_on_supported_platforms() {
        if cfg!(any(target_os = "linux", windows, target_vendor = "apple")) {
            let rss = super::rss_bytes().unwrap();
            assert!(rss > 1024 * 1024, "{rss}");
            let before = super::cpu_time().unwrap();
            let mut x = 0u64;
            for i in 0..5_000_000u64 {
                x = x.wrapping_add(i * i);
            }
            std::hint::black_box(x);
            assert!(super::cpu_time().unwrap() >= before);
        }
    }
}
