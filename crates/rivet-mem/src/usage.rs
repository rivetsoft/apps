/// How much memory a process uses, in bytes, from `/proc/PID/smaps_rollup`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Resident: in memory now, counting shared pages in full.
    pub rss: u64,
    /// Proportional: shared pages split between the processes sharing them. The fairest single
    /// number for "how much does this app cost".
    pub pss: u64,
    /// The private, anonymous part (the heap, textures, buffers): what the app itself holds.
    pub anon: u64,
    /// Swapped out.
    pub swap: u64,
}

impl Usage {
    /// This process.
    pub fn now() -> Option<Usage> {
        Self::read("/proc/self/smaps_rollup")
    }

    /// Another process (one of the user's own; others' can't be read).
    pub fn of(pid: u32) -> Option<Usage> {
        Self::read(&format!("/proc/{pid}/smaps_rollup"))
    }

    fn read(path: &str) -> Option<Usage> {
        Some(Self::parse(&std::fs::read_to_string(path).ok()?))
    }

    fn parse(text: &str) -> Usage {
        let mut u = Usage::default();
        for line in text.lines() {
            let Some((key, rest)) = line.split_once(':') else { continue };
            let kb: u64 = rest.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            match key {
                "Rss" => u.rss = kb * 1024,
                "Pss" => u.pss = kb * 1024,
                "Anonymous" => u.anon = kb * 1024,
                "Swap" => u.swap = kb * 1024,
                _ => {}
            }
        }
        u
    }

    /// One line for logs and checks: `rss 120.4 MB, pss 98.1 MB, anon 80.0 MB`.
    pub fn line(&self) -> String {
        let mb = |b: u64| b as f64 / 1e6;
        format!("rss {:.1} MB, pss {:.1} MB, anon {:.1} MB, swap {:.1} MB", mb(self.rss), mb(self.pss), mb(self.anon), mb(self.swap))
    }
}

/// The allocator's own view of this process's heap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Heap {
    /// Handed out and not yet freed.
    pub used: usize,
    /// Freed but still held by the allocator (what a trim could give back, at most).
    pub free: usize,
}

/// The heap as the C library sees it (glibc only; None elsewhere).
pub fn heap() -> Option<Heap> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let m = unsafe { libc::mallinfo2() };
        Some(Heap { used: m.uordblks + m.hblkhd, free: m.fordblks })
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_smaps_rollup() {
        let text = "55a0-7ffd ---p 00000000 00:00 0 [rollup]\nRss:              120000 kB\nPss:               98000 kB\nPss_Anon:          80000 kB\nAnonymous:         80500 kB\nSwap:                  0 kB\n";
        let u = Usage::parse(text);
        assert_eq!((u.rss, u.pss, u.anon, u.swap), (120000 * 1024, 98000 * 1024, 80500 * 1024, 0));
        assert!(Usage::now().is_some_and(|u| u.rss > 0 && u.pss > 0));
        assert!(heap().is_some_and(|h| h.used > 0));
    }
}
