//! Which older copies the storage keeps (the world's backups), and the removal of the rest.
//!
//! Kept: the newest [`NEWEST`] generations (the last hour or so of play, one every few
//! minutes), the last one of each of the newest [`DAYS`] days, the last one of each of the
//! newest [`WEEKS`] weeks, and whatever a load needs (the copy the last load replaced, so it can
//! be undone, and the backup a request asks for). The copies share their blocks, so each costs
//! only what changed since the one before.
//!
//! The whole bucket stays under [`BUDGET`], R2's free 10 GB with room to spare: when it would
//! not, the oldest backups go first, until it fits or only the newest [`ALWAYS`], the current
//! copy and the ones a load needs are left.
//!
//! A pack is removed only when no kept generation points into it and it is older than the
//! world's current generation, so one that is being uploaded right now is never touched. If any
//! kept manifest can't be read, nothing is removed at all.
use crate::{Error, Result, manifest::GenId, sync::Remote};
use std::collections::{BTreeMap, BTreeSet};

pub const NEWEST: usize = 20;
pub const DAYS: usize = 30;
pub const WEEKS: usize = 26;
/// What the whole bucket is kept under, all its worlds together (bytes): R2's free 10 GB, less
/// room for copies on their way up and anything else kept there.
pub const BUDGET: u64 = 8_000_000_000;
/// Backups never removed for space: the newest few, so a world always has the ones from just
/// before.
pub const ALWAYS: usize = 3;
/// A sweep is worth its requests once this many generations are stored (or the bucket is over
/// its budget).
pub const SWEEP_AT: usize = NEWEST + DAYS + WEEKS + 8;

/// The ISO week a `yyyymmdd` day falls in, as (year, week); a day that isn't a date counts as
/// its own week.
fn week(day: u32) -> (i32, u32) {
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::try_from(day / 10_000).unwrap_or(0),
        day / 100 % 100,
        day % 100,
    );
    match date {
        Some(d) => {
            let w = chrono::Datelike::iso_week(&d);
            (w.year(), w.week())
        }
        None => (-1, day),
    }
}

/// The generations to keep of `all`, the world's newest copy being `current`, `protected`
/// always among them (when they are stored).
pub fn kept(all: &[GenId], current: GenId, protected: &[GenId]) -> BTreeSet<GenId> {
    let mut older: Vec<GenId> = all.iter().copied().filter(|id| *id <= current).collect();
    older.sort_unstable_by(|a, b| b.cmp(a));
    older.dedup();
    let mut keep: BTreeSet<GenId> = older.iter().copied().take(NEWEST).collect();
    let mut days: Vec<u32> = Vec::new();
    let mut weeks: Vec<(i32, u32)> = Vec::new();
    // `older` runs from the newest back: the first of each day (or week) met is its newest.
    for id in &older {
        if days.len() < DAYS && !days.contains(&id.day) {
            days.push(id.day);
            keep.insert(*id);
        }
        let w = week(id.day);
        if weeks.len() < WEEKS && !weeks.contains(&w) {
            weeks.push(w);
            keep.insert(*id);
        }
        if days.len() == DAYS && weeks.len() == WEEKS {
            break;
        }
    }
    keep.extend(protected.iter().filter(|id| all.contains(id)));
    keep.insert(current);
    keep
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub manifests: usize,
    pub packs: usize,
    pub bytes: u64,
    /// What the whole bucket holds once the sweep is done (bytes).
    pub used: u64,
    /// Over the budget even with every backup that may go gone.
    pub full: bool,
}

/// The generations whose manifests the storage lists.
pub fn generations(remote: Remote<'_>) -> Result<Vec<GenId>> {
    let prefix = remote.key("");
    Ok(remote
        .store
        .list(&remote.key("gen/"))?
        .iter()
        .filter_map(|o| GenId::from_manifest_key(o.key.strip_prefix(&prefix)?))
        .collect())
}

/// Removes the copies no longer kept, and keeps the bucket under `budget`. Only the world's host
/// calls this, with the generation its lease names, and the ones a load needs (`protected`).
pub fn sweep(
    remote: Remote<'_>,
    current: GenId,
    protected: &[GenId],
    budget: u64,
) -> Result<Swept> {
    let prefix = remote.key("");
    // Everything in the bucket: the other worlds' share counts against the budget too.
    let everything = remote.store.list("")?;
    let mut others = 0u64;
    let mut sizes: BTreeMap<String, u64> = BTreeMap::new();
    for object in &everything {
        match object.key.strip_prefix(&prefix) {
            Some(name) => {
                sizes.insert(name.to_owned(), object.size);
            }
            None => others += object.size,
        }
    }
    let mine: u64 = sizes.values().sum();
    let all = generations(remote)?;
    if all.len() < SWEEP_AT && others + mine <= budget {
        return Ok(Swept {
            used: others + mine,
            ..Swept::default()
        });
    }
    if !all.contains(&current) {
        return Err(Error::Store(
            "the storage doesn't list the world's newest copy: nothing is removed".into(),
        ));
    }
    let mut keep = kept(&all, current, protected);
    // Every pack a kept generation points into. A manifest that can't be read ends the sweep
    // here, before anything is removed.
    let mut packs_of: BTreeMap<GenId, Vec<String>> = BTreeMap::new();
    for id in &keep {
        packs_of.insert(*id, remote.manifest(*id)?.packs);
    }
    // A pack of a copy older than the current one: the only kind ever removed (one being
    // uploaded now is newer; one whose name this game doesn't know is left alone).
    let older =
        |name: &str| GenId::of_pack(name).is_some_and(|made| made < (current.epoch, current.seq));
    // What stays whatever is kept: the lease and the world's other small objects, and packs
    // that are never removed here.
    let fixed: u64 = sizes
        .iter()
        .filter(|(name, _)| {
            !name.starts_with("gen/") && (!name.starts_with("pack/") || !older(name))
        })
        .map(|(_, size)| size)
        .sum();
    let held = |keep: &BTreeSet<GenId>| -> u64 {
        let live: BTreeSet<&String> = keep.iter().flat_map(|id| &packs_of[id]).collect();
        let packs: u64 = live
            .iter()
            .filter(|name| older(name))
            .map(|name| sizes.get(name.as_str()).copied().unwrap_or(0))
            .sum();
        let manifests: u64 = keep
            .iter()
            .map(|id| sizes.get(&id.manifest_key()).copied().unwrap_or(0))
            .sum();
        packs + manifests
    };
    // For space, the oldest first: never the current copy, the protected ones, or the newest
    // few backups.
    let mut droppable: Vec<GenId> = keep
        .iter()
        .copied()
        .filter(|id| *id != current && !protected.contains(id))
        .collect();
    droppable.truncate(droppable.len().saturating_sub(ALWAYS));
    let mut full = false;
    let used = loop {
        let total = others + fixed + held(&keep);
        if total <= budget {
            break total;
        }
        if droppable.is_empty() {
            full = true;
            break total;
        }
        keep.remove(&droppable.remove(0));
    };
    let live: BTreeSet<String> = keep
        .iter()
        .flat_map(|id| packs_of[id].iter().cloned())
        .collect();
    let stamps: Vec<String> = sizes
        .keys()
        .filter(|name| name.starts_with("stamp/"))
        .cloned()
        .collect();
    let mut swept = Swept {
        used,
        full,
        ..Swept::default()
    };
    for id in all.iter().filter(|id| **id < current && !keep.contains(id)) {
        remote.store.delete(&remote.key(&id.manifest_key()))?;
        for stamp in crate::backups::stamp_keys(&stamps, *id) {
            remote.store.delete(&remote.key(&stamp))?;
        }
        swept.manifests += 1;
    }
    for (name, size) in &sizes {
        if name.starts_with("pack/") && older(name) && !live.contains(name) {
            remote.store.delete(&remote.key(name))?;
            swept.packs += 1;
            swept.bytes += size;
        }
    }
    Ok(swept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        scan::{LocalIndex, tests::Scratch},
        store::{MemoryStore, Store},
        sync::tests::{PREFIX, publish_world},
    };

    fn id(seq: u32, day: u32) -> GenId {
        GenId { epoch: 1, seq, day }
    }

    fn yyyymmdd(d: chrono::NaiveDate) -> u32 {
        use chrono::Datelike;
        u32::try_from(d.year()).unwrap() * 10_000 + d.month() * 100 + d.day()
    }

    #[test]
    fn the_newest_and_one_a_day_are_kept() {
        // Three copies a day for 30 days.
        let all: Vec<GenId> = (0..90).map(|i| id(i + 1, 20261001 + i / 3)).collect();
        let current = all[89];
        let keep = kept(&all, current, &[]);
        // The newest 20 are seq 71..=90 (days 24..=30); every older day keeps its last copy (a
        // week's newest is the newest of its last day: nothing more).
        let extra: Vec<u32> = keep.iter().filter(|g| g.seq <= 70).map(|g| g.seq).collect();
        assert_eq!(extra, (1..=23).map(|d| d * 3).collect::<Vec<_>>());
        assert_eq!(keep.len(), 43);
        assert!(keep.contains(&current));
        // A copy newer than the current one (an upload under way, a fenced host's) is no keeper.
        let mut with_newer = all.clone();
        with_newer.push(id(91, 20261030));
        assert!(!kept(&with_newer, current, &[]).contains(&id(91, 20261030)));
        assert_eq!(kept(&[], current, &[]).len(), 1);
    }

    #[test]
    fn one_a_week_is_kept_for_half_a_year() {
        // One copy a day for 400 days.
        let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let days: Vec<chrono::NaiveDate> = (0..400).map(|i| start + chrono::Days::new(i)).collect();
        let all: Vec<GenId> = days
            .iter()
            .enumerate()
            .map(|(i, d)| id(i as u32 + 1, yyyymmdd(*d)))
            .collect();
        let current = *all.last().unwrap();
        let keep = kept(&all, current, &[]);
        // The newest 26 weeks each keep a copy; nothing older does.
        let mut weeks: Vec<(i32, u32)> = all.iter().rev().map(|g| week(g.day)).collect();
        weeks.dedup();
        let newest: Vec<(i32, u32)> = weeks.iter().copied().take(WEEKS).collect();
        for w in &newest {
            assert!(keep.iter().any(|g| week(g.day) == *w), "week {w:?}");
        }
        assert!(keep.iter().all(|g| newest.contains(&week(g.day))));
        // The last 30 days are each kept.
        for g in &all[all.len() - DAYS..] {
            assert!(keep.contains(g));
        }
        assert!(keep.len() <= DAYS + WEEKS);
        // What a load needs is kept however old.
        let old = all[3];
        assert!(kept(&all, current, &[old]).contains(&old));
        // A protected copy the storage doesn't have isn't made up.
        let gone = id(9999, 20250101);
        assert!(!kept(&all, current, &[gone]).contains(&gone));
        // A day that isn't a date is a week of its own.
        assert_eq!(week(20261399), (-1, 20261399));
    }

    #[test]
    fn a_sweep_removes_what_nothing_kept_points_into() {
        let store = MemoryStore::new();
        let remote = Remote {
            store: &store,
            prefix: PREFIX,
        };
        let world = Scratch::new("sweep");
        let mut index = LocalIndex::default();
        let mut previous = None;
        let total = SWEEP_AT as u32 + 12;
        // A file rewritten in every copy, and one that never changes.
        world.write("stays.dat", b"always here");
        for seq in 1..=total {
            world.write("level.dat", format!("copy {seq}").as_bytes());
            index.scanned = 0;
            let day = 20261001 + (seq - 1) / 30;
            let gen_id = GenId { epoch: 1, seq, day };
            previous = Some(publish_world(
                &store,
                &world,
                &mut index,
                previous.as_ref(),
                gen_id,
            ));
            crate::backups::stamp(remote, gen_id, crate::backups::Kind::Checkpoint, "A", 1)
                .unwrap();
        }
        let newest = previous.unwrap();
        // An upload under way: a pack newer than the current copy, in no manifest yet.
        let in_flight = remote.key(
            &GenId {
                epoch: 1,
                seq: total + 1,
                day: 0,
            }
            .pack_name(0),
        );
        store
            .put(
                &in_flight,
                b"being uploaded",
                &crate::store::Condition::Always,
            )
            .unwrap();
        // And something this game doesn't know, under the world's packs.
        let unknown = remote.key("pack/not-ours.bin");
        store
            .put(&unknown, b"x", &crate::store::Condition::Always)
            .unwrap();
        let before = generations(remote).unwrap().len();
        assert_eq!(before, total as usize);
        let swept = sweep(remote, newest.id, &[], u64::MAX).unwrap();
        let after = generations(remote).unwrap();
        let keep = kept(
            &(1..=total)
                .map(|s| GenId {
                    epoch: 1,
                    seq: s,
                    day: 20261001 + (s - 1) / 30,
                })
                .collect::<Vec<_>>(),
            newest.id,
            &[],
        );
        assert_eq!(after.iter().copied().collect::<BTreeSet<_>>(), keep);
        assert_eq!(swept.manifests, before - keep.len());
        assert!(swept.packs > 0 && swept.bytes > 0 && !swept.full);
        // The removed copies' stamps went with them; the kept ones' stayed.
        let listed = crate::backups::list(remote).unwrap();
        assert_eq!(listed.len(), keep.len());
        assert!(listed.iter().all(|b| b.at == Some(1)));
        assert_eq!(
            store
                .keys()
                .iter()
                .filter(|k| k.contains("/stamp/"))
                .count(),
            keep.len()
        );
        // Every kept copy still reads whole, the first pack (the file that never changed) with it.
        for kept_id in &keep {
            let manifest = remote.manifest(*kept_id).unwrap();
            for file in &manifest.files {
                for i in 0..file.blocks.len() as u64 {
                    crate::sync::read_run(remote, &manifest, file, i, 1).unwrap();
                }
            }
        }
        assert!(
            store.get(&in_flight).unwrap().is_some(),
            "an upload under way is left alone"
        );
        assert!(
            store.get(&unknown).unwrap().is_some(),
            "an unknown object is left alone"
        );
        // Nothing more to remove.
        let again = sweep(remote, newest.id, &[], u64::MAX).unwrap();
        assert_eq!((again.manifests, again.packs), (0, 0));
    }

    /// `n` copies of a world whose 40 kB file changes whole in each, a day apart, behind
    /// another world's 1 MB in the same bucket.
    fn crowded(store: &MemoryStore, n: u32) -> GenId {
        let remote = Remote {
            store,
            prefix: PREFIX,
        };
        store
            .put(
                "w/ffffffffffffffff/pack/0000000001-000001-000.pack",
                &vec![7u8; 1_000_000],
                &crate::store::Condition::Always,
            )
            .unwrap();
        let world = Scratch::new("budget");
        let mut index = LocalIndex::default();
        let mut previous = None;
        let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for seq in 1..=n {
            let data: Vec<u8> = (0..40_000u32)
                .map(|i| (i.wrapping_mul(seq * 2 + 1) >> 3) as u8)
                .collect();
            world.write("region/r.0.0.mca", &data);
            index.scanned = 0;
            let day = yyyymmdd(start + chrono::Days::new(u64::from(seq)));
            previous = Some(publish_world(
                store,
                &world,
                &mut index,
                previous.as_ref(),
                GenId { epoch: 1, seq, day },
            ));
        }
        let _ = remote;
        previous.unwrap().id
    }

    #[test]
    fn the_bucket_stays_under_its_budget_the_oldest_backups_going_first() {
        let store = MemoryStore::new();
        let remote = Remote {
            store: &store,
            prefix: PREFIX,
        };
        let current = crowded(&store, 40);
        let all = generations(remote).unwrap();
        let protected = all.iter().copied().min().unwrap();
        let used = |s: &MemoryStore| -> u64 { s.list("").unwrap().iter().map(|o| o.size).sum() };
        let before = used(&store);
        // Room for the other world and about ten of this one's copies.
        let budget = 1_000_000 + 10 * 42_000;
        assert!(before > budget);
        let swept = sweep(remote, current, &[protected], budget).unwrap();
        assert!(!swept.full);
        assert!(used(&store) <= budget, "{} > {budget}", used(&store));
        assert_eq!(swept.used, used(&store));
        let left = generations(remote).unwrap();
        // The current copy, the protected one and the newest backups stay; the oldest went.
        assert!(left.contains(&current) && left.contains(&protected));
        let mut newest = all.clone();
        newest.sort_unstable_by(|a, b| b.cmp(a));
        for g in &newest[..=ALWAYS] {
            assert!(left.contains(g), "{g:?}");
        }
        let oldest_left = left.iter().filter(|g| **g != protected).min().unwrap();
        for g in &all {
            if g < oldest_left && *g != protected {
                assert!(!left.contains(g), "{g:?} kept while a newer one went");
            }
        }
        // The other world is never touched.
        assert!(store.list("w/ffffffffffffffff/").unwrap().len() == 1);
        // Whatever is left reads whole.
        for g in &left {
            let manifest = remote.manifest(*g).unwrap();
            for file in &manifest.files {
                crate::sync::read_run(remote, &manifest, file, 0, file.blocks.len() as u64)
                    .unwrap();
            }
        }
    }

    #[test]
    fn a_budget_nothing_can_meet_keeps_what_must_stay_and_says_so() {
        let store = MemoryStore::new();
        let remote = Remote {
            store: &store,
            prefix: PREFIX,
        };
        let current = crowded(&store, 12);
        let all = generations(remote).unwrap();
        let protected = all.iter().copied().min().unwrap();
        let swept = sweep(remote, current, &[protected], 10).unwrap();
        assert!(swept.full);
        let left: BTreeSet<GenId> = generations(remote).unwrap().into_iter().collect();
        let mut newest = all.clone();
        newest.sort_unstable_by(|a, b| b.cmp(a));
        let mut must: BTreeSet<GenId> = newest[..=ALWAYS].iter().copied().collect();
        must.insert(protected);
        assert_eq!(left, must);
    }

    #[test]
    fn a_manifest_that_cant_be_read_stops_the_sweep_before_it_removes_anything() {
        let store = MemoryStore::new();
        let remote = Remote {
            store: &store,
            prefix: PREFIX,
        };
        let world = Scratch::new("sweepstop");
        let mut index = LocalIndex::default();
        let mut previous = None;
        let total = SWEEP_AT as u32 + 3;
        for seq in 1..=total {
            world.write("level.dat", format!("copy {seq}").as_bytes());
            index.scanned = 0;
            let gen_id = GenId {
                epoch: 1,
                seq,
                day: 20261001,
            };
            previous = Some(publish_world(
                &store,
                &world,
                &mut index,
                previous.as_ref(),
                gen_id,
            ));
        }
        let newest = previous.unwrap();
        let keys = store.keys();
        let damaged = remote.key(
            &GenId {
                epoch: 1,
                seq: total - 1,
                day: 20261001,
            }
            .manifest_key(),
        );
        store
            .put(&damaged, b"{", &crate::store::Condition::Always)
            .unwrap();
        assert!(sweep(remote, newest.id, &[], u64::MAX).is_err());
        assert_eq!(store.keys(), keys);
        // And a current copy the storage doesn't list.
        let missing = GenId {
            epoch: 9,
            seq: 1,
            day: 20261001,
        };
        assert!(sweep(remote, missing, &[], u64::MAX).is_err());
        assert_eq!(store.keys(), keys);
    }
}
