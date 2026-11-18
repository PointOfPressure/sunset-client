// com.seedfinding.mccore.version.MCVersion, newest first: the order is the order of the Java enum, so `ordinal` and
// the comparisons mean what they mean there. Generated from the table of the Java source, one row for one constant.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum MCVersion {
    v1_21_3,
    v1_21_2,
    v1_21_1,
    v1_21,
    v1_20_6,
    v1_20_5,
    v1_20_4,
    v1_20_3,
    v1_20_2,
    v1_20_1,
    v1_20,
    v1_19_4,
    v1_19_3,
    v1_19_2,
    v1_19_1,
    v1_19,
    v1_18_2,
    v1_18_1,
    v1_18,
    v1_17_1,
    v1_17,
    v1_16_5,
    v1_16_4,
    v1_16_3,
    v1_16_2,
    v1_16_1,
    v1_16,
    v1_15_2,
    v1_15_1,
    v1_15,
    v1_14_4,
    v1_14_3,
    v1_14_2,
    v1_14_1,
    v1_14,
    v1_13_2,
    v1_13_1,
    v1_13,
    v1_12_2,
    v1_12_1,
    v1_12,
    v1_11_2,
    v1_11_1,
    v1_11,
    v1_10_2,
    v1_10_1,
    v1_10,
    v1_9_4,
    v1_9_3,
    v1_9_2,
    v1_9_1,
    v1_9,
    v1_8_9,
    v1_8_8,
    v1_8_7,
    v1_8_6,
    v1_8_5,
    v1_8_4,
    v1_8_3,
    v1_8_2,
    v1_8_1,
    v1_8,
    v1_7_10,
    v1_7_9,
    v1_7_8,
    v1_7_7,
    v1_7_6,
    v1_7_5,
    v1_7_4,
    v1_7_3,
    v1_7_2,
    v1_6_4,
    v1_6_2,
    v1_6_1,
    v1_5_2,
    v1_5_1,
    v1_4_7,
    v1_4_6,
    v1_4_5,
    v1_4_4,
    v1_4_2,
    v1_3_2,
    v1_3_1,
    v1_2_5,
    v1_2_4,
    v1_2_3,
    v1_2_2,
    v1_2_1,
    v1_1,
    v1_0,
    vb1_8_1,
    vb1_8,
    vb1_7_3,
    vb1_7_2,
    vb1_7,
    vb1_6_6,
    vb1_6_5,
    vb1_6_4,
    vb1_6_3,
    vb1_6_2,
    vb1_6_1,
    vb1_6,
    vb1_5_01,
    vb1_5,
    vb1_4_01,
    vb1_4,
    vb1_3_01,
    vb1_3b,
    vb1_2_02,
    vb1_2_01,
    vb1_2,
    vb1_1_02,
    vb1_1_01,
    vb1_0_2,
    vb1_0_01,
    vb1_0,
    va1_2_6,
    va1_2_5,
    va1_2_4_01,
    va1_2_3_04,
    va1_2_3_02,
    va1_2_3_01,
    va1_2_3,
    va1_2_2b,
    va1_2_2a,
    va1_2_1_01,
    va1_2_1,
    va1_2_0_02,
    va1_2_0_01,
    va1_2_0,
    va1_1_2_01,
    va1_1_2,
    va1_1_0,
    va1_0_17_04,
    va1_0_17_02,
    va1_0_16,
    va1_0_15,
    va1_0_14,
    va1_0_11,
    va1_0_5_01,
    va1_0_4,
}

/// `(name, release, subVersion)` for each constant, in declaration order.
const TABLE: [(&str, i32, i32); 141] = [
    ("1.21.3", 21, 3), // October 23, 2024
    ("1.21.2", 21, 2), // October 22, 2024
    ("1.21.1", 21, 1), // August 8, 2024
    ("1.21", 21, 0), // June 13, 2024
    ("1.20.6", 20, 6), // April 29, 2024
    ("1.20.5", 20, 5), // April 23, 2024
    ("1.20.4", 20, 4), // December 7, 2023
    ("1.20.3", 20, 3), // December 4, 2023
    ("1.20.2", 20, 2), // September 20, 2023
    ("1.20.1", 20, 1), // June 12, 2023
    ("1.20", 20, 0), // June 2, 2023
    ("1.19.4", 19, 4), // March 14, 2023
    ("1.19.3", 19, 3), // December 7, 2022
    ("1.19.2", 19, 2), // August 5, 2022
    ("1.19.1", 19, 1), // July 27, 2022
    ("1.19", 19, 0), // June 7, 2022
    ("1.18.2", 18, 2), // February 28, 2022
    ("1.18.1", 18, 1), // December 10, 2021
    ("1.18", 18, 0), // November 30, 2021
    ("1.17.1", 17, 1), // July 6, 2021
    ("1.17", 17, 0), // June 8, 2021
    ("1.16.5", 16, 5), // January 14, 2021
    ("1.16.4", 16, 4), // October 29, 2020
    ("1.16.3", 16, 3), // September 10, 2020
    ("1.16.2", 16, 2), // August 11, 2020
    ("1.16.1", 16, 1), // June 24, 2020
    ("1.16", 16, 0), // June 23, 2020
    ("1.15.2", 15, 2), // January 17, 2020
    ("1.15.1", 15, 1), // December 16, 2019
    ("1.15", 15, 0), // December 9, 2019
    ("1.14.4", 14, 4), // July 19, 2019
    ("1.14.3", 14, 3), // June 24, 2019
    ("1.14.2", 14, 2), // May 27, 2019
    ("1.14.1", 14, 1), // May 13, 2019
    ("1.14", 14, 0), // April 23, 2019
    ("1.13.2", 13, 2), // October 22, 2018
    ("1.13.1", 13, 1), // August 22, 2018
    ("1.13", 13, 0), // July 18, 2018
    ("1.12.2", 12, 2), // September 18, 2017
    ("1.12.1", 12, 1), // August 3, 2017
    ("1.12", 12, 0), // June 2, 2017
    ("1.11.2", 11, 2), // December 21, 2016
    ("1.11.1", 11, 1), // December 20, 2016
    ("1.11", 11, 0), // November 14, 2016
    ("1.10.2", 10, 2), // June 23, 2016
    ("1.10.1", 10, 1), // June 22, 2016
    ("1.10", 10, 0), // June 8, 2016
    ("1.9.4", 9, 4), // May 10, 2016
    ("1.9.3", 9, 3), // May 10, 2016
    ("1.9.2", 9, 2), // March 30, 2016
    ("1.9.1", 9, 1), // March 30, 2016
    ("1.9", 9, 0), // February 29, 2016
    ("1.8.9", 8, 9), // December 3, 2015
    ("1.8.8", 8, 8), // July 27, 2015
    ("1.8.7", 8, 7), // June 5, 2015
    ("1.8.6", 8, 6), // May 25, 2015
    ("1.8.5", 8, 5), // May 22, 2015
    ("1.8.4", 8, 4), // April 17, 2015
    ("1.8.3", 8, 3), // February 20, 2015
    ("1.8.2", 8, 2), // February 19, 2015
    ("1.8.1", 8, 1), // November 24, 2014
    ("1.8", 8, 0), // September 2, 2014
    ("1.7.10", 7, 10), // May 14, 2014
    ("1.7.9", 7, 9), // April 14, 2014
    ("1.7.8", 7, 8), // April 9, 2014
    ("1.7.7", 7, 7), // April 9, 2014
    ("1.7.6", 7, 6), // April 9, 2014
    ("1.7.5", 7, 5), // February 26, 2014
    ("1.7.4", 7, 4), // December 9, 2013
    ("1.7.3", 7, 3), // December 6, 2013
    ("1.7.2", 7, 2), // October 25, 2013
    ("1.6.4", 6, 4), // September 19, 2013
    ("1.6.2", 6, 2), // July 5, 2013
    ("1.6.1", 6, 1), // June 28, 2013
    ("1.5.2", 5, 2), // April 25, 2013
    ("1.5.1", 5, 1), // March 20, 2013
    ("1.4.7", 4, 7), // December 27, 2012
    ("1.4.6", 4, 6), // December 19, 2012
    ("1.4.5", 4, 5), // December 19, 2012
    ("1.4.4", 4, 4), // December 13, 2012
    ("1.4.2", 4, 2), // November 24, 2012
    ("1.3.2", 3, 2), // August 15, 2012
    ("1.3.1", 3, 1), // July 31, 2012
    ("1.2.5", 2, 5), // March 29, 2012
    ("1.2.4", 2, 4), // March 21, 2012
    ("1.2.3", 2, 3), // March 1, 2012
    ("1.2.2", 2, 2), // February 29, 2012
    ("1.2.1", 2, 1), // February 29, 2012
    ("1.1", 1, 0), // January 11, 2012
    ("1.0", 0, 0), // November 17, 2011
    ("b1.8.1", 8, 1), // September 18, 2011
    ("b1.8", 8, 0), // September 14, 2011
    ("b1.7.3", 7, 3), // July 7, 2011
    ("b1.7.2", 7, 2), // June 30, 2011
    ("b1.7", 7, 0), // June 29, 2011
    ("b1.6.6", 6, 6), // May 30, 2011
    ("b1.6.5", 6, 5), // May 27, 2011
    ("b1.6.4", 6, 4), // May 25, 2011
    ("b1.6.3", 6, 3), // May 25, 2011
    ("b1.6.2", 6, 2), // May 25, 2011
    ("b1.6.1", 6, 1), // May 25, 2011
    ("b1.6", 6, 0), // May 25, 2011
    ("b1.5_01", 501, 0), // April 19, 2011
    ("b1.5", 5, 0), // April 18, 2011
    ("b1.4_01", 401, 0), // April 4, 2011
    ("b1.4", 4, 0), // March 30, 2011
    ("b1.3_01", 301, 0), // February 22, 2011
    ("b1.3b", 3, 0), // February 21, 2011
    ("b1.2_02", 202, 0), // January 20, 2011
    ("b1.2_01", 201, 0), // January 13, 2011
    ("b1.2", 2, 0), // January 12, 2011
    ("b1.1_02", 102, 0), // December 21, 2010
    ("b1.1_01", 101, 0), // December 21, 2010
    ("b1.0.2", 0, 2), // December 20, 2010
    ("b1.0_01", 1, 0), // December 19, 2010
    ("b1.0", 0, 0), // December 19, 2010
    ("a1.2.6", 2, 6), // December 2, 2010
    ("a1.2.5", 2, 5), // November 30, 2010
    ("a1.2.4_01", 2, 401), // November 29, 2010
    ("a1.2.3_04", 2, 304), // November 25, 2010
    ("a1.2.3_02", 2, 302), // November 24, 2010
    ("a1.2.3_01", 2, 301), // November 23, 2010
    ("a1.2.3", 2, 3), // November 23, 2010
    ("a1.2.2b", 2, 2), // November 9, 2010
    ("a1.2.2a", 2, 2), // November 9, 2010
    ("a1.2.1_01", 2, 101), // November 4, 2010
    ("a1.2.1", 2, 1), // November 4, 2010
    ("a1.2.0_02", 2, 2), // November 3, 2010
    ("a1.2.0_01", 2, 1), // October 30, 2010
    ("a1.2.0", 2, 0), // October 29, 2010
    ("a1.1.2_01", 1, 201), // September 22, 2010
    ("a1.1.2", 1, 2), // September 19, 2010
    ("a1.1.0", 1, 0), // September 12, 2010
    ("a1.0.17_04", 0, 1704), // August 22, 2010
    ("a1.0.17_02", 0, 1702), // August 19, 2010
    ("a1.0.16", 0, 16), // August 11, 2010
    ("a1.0.15", 0, 15), // August 3, 2010
    ("a1.0.14", 0, 14), // July 29, 2010
    ("a1.0.11", 0, 11), // July 22, 2010
    ("a1.0.5_01", 0, 501), // July 12, 2010
    ("a1.0.4", 0, 4), // July 8, 2010
];

pub const ALL: [MCVersion; 141] = [
    MCVersion::v1_21_3,
    MCVersion::v1_21_2,
    MCVersion::v1_21_1,
    MCVersion::v1_21,
    MCVersion::v1_20_6,
    MCVersion::v1_20_5,
    MCVersion::v1_20_4,
    MCVersion::v1_20_3,
    MCVersion::v1_20_2,
    MCVersion::v1_20_1,
    MCVersion::v1_20,
    MCVersion::v1_19_4,
    MCVersion::v1_19_3,
    MCVersion::v1_19_2,
    MCVersion::v1_19_1,
    MCVersion::v1_19,
    MCVersion::v1_18_2,
    MCVersion::v1_18_1,
    MCVersion::v1_18,
    MCVersion::v1_17_1,
    MCVersion::v1_17,
    MCVersion::v1_16_5,
    MCVersion::v1_16_4,
    MCVersion::v1_16_3,
    MCVersion::v1_16_2,
    MCVersion::v1_16_1,
    MCVersion::v1_16,
    MCVersion::v1_15_2,
    MCVersion::v1_15_1,
    MCVersion::v1_15,
    MCVersion::v1_14_4,
    MCVersion::v1_14_3,
    MCVersion::v1_14_2,
    MCVersion::v1_14_1,
    MCVersion::v1_14,
    MCVersion::v1_13_2,
    MCVersion::v1_13_1,
    MCVersion::v1_13,
    MCVersion::v1_12_2,
    MCVersion::v1_12_1,
    MCVersion::v1_12,
    MCVersion::v1_11_2,
    MCVersion::v1_11_1,
    MCVersion::v1_11,
    MCVersion::v1_10_2,
    MCVersion::v1_10_1,
    MCVersion::v1_10,
    MCVersion::v1_9_4,
    MCVersion::v1_9_3,
    MCVersion::v1_9_2,
    MCVersion::v1_9_1,
    MCVersion::v1_9,
    MCVersion::v1_8_9,
    MCVersion::v1_8_8,
    MCVersion::v1_8_7,
    MCVersion::v1_8_6,
    MCVersion::v1_8_5,
    MCVersion::v1_8_4,
    MCVersion::v1_8_3,
    MCVersion::v1_8_2,
    MCVersion::v1_8_1,
    MCVersion::v1_8,
    MCVersion::v1_7_10,
    MCVersion::v1_7_9,
    MCVersion::v1_7_8,
    MCVersion::v1_7_7,
    MCVersion::v1_7_6,
    MCVersion::v1_7_5,
    MCVersion::v1_7_4,
    MCVersion::v1_7_3,
    MCVersion::v1_7_2,
    MCVersion::v1_6_4,
    MCVersion::v1_6_2,
    MCVersion::v1_6_1,
    MCVersion::v1_5_2,
    MCVersion::v1_5_1,
    MCVersion::v1_4_7,
    MCVersion::v1_4_6,
    MCVersion::v1_4_5,
    MCVersion::v1_4_4,
    MCVersion::v1_4_2,
    MCVersion::v1_3_2,
    MCVersion::v1_3_1,
    MCVersion::v1_2_5,
    MCVersion::v1_2_4,
    MCVersion::v1_2_3,
    MCVersion::v1_2_2,
    MCVersion::v1_2_1,
    MCVersion::v1_1,
    MCVersion::v1_0,
    MCVersion::vb1_8_1,
    MCVersion::vb1_8,
    MCVersion::vb1_7_3,
    MCVersion::vb1_7_2,
    MCVersion::vb1_7,
    MCVersion::vb1_6_6,
    MCVersion::vb1_6_5,
    MCVersion::vb1_6_4,
    MCVersion::vb1_6_3,
    MCVersion::vb1_6_2,
    MCVersion::vb1_6_1,
    MCVersion::vb1_6,
    MCVersion::vb1_5_01,
    MCVersion::vb1_5,
    MCVersion::vb1_4_01,
    MCVersion::vb1_4,
    MCVersion::vb1_3_01,
    MCVersion::vb1_3b,
    MCVersion::vb1_2_02,
    MCVersion::vb1_2_01,
    MCVersion::vb1_2,
    MCVersion::vb1_1_02,
    MCVersion::vb1_1_01,
    MCVersion::vb1_0_2,
    MCVersion::vb1_0_01,
    MCVersion::vb1_0,
    MCVersion::va1_2_6,
    MCVersion::va1_2_5,
    MCVersion::va1_2_4_01,
    MCVersion::va1_2_3_04,
    MCVersion::va1_2_3_02,
    MCVersion::va1_2_3_01,
    MCVersion::va1_2_3,
    MCVersion::va1_2_2b,
    MCVersion::va1_2_2a,
    MCVersion::va1_2_1_01,
    MCVersion::va1_2_1,
    MCVersion::va1_2_0_02,
    MCVersion::va1_2_0_01,
    MCVersion::va1_2_0,
    MCVersion::va1_1_2_01,
    MCVersion::va1_1_2,
    MCVersion::va1_1_0,
    MCVersion::va1_0_17_04,
    MCVersion::va1_0_17_02,
    MCVersion::va1_0_16,
    MCVersion::va1_0_15,
    MCVersion::va1_0_14,
    MCVersion::va1_0_11,
    MCVersion::va1_0_5_01,
    MCVersion::va1_0_4,
];

impl MCVersion {
    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `values()[i]`.
    pub fn values() -> &'static [MCVersion] {
        &ALL
    }

    pub fn name(self) -> &'static str {
        TABLE[self as usize].0
    }

    pub fn release(self) -> i32 {
        TABLE[self as usize].1
    }

    pub fn sub_version(self) -> i32 {
        TABLE[self as usize].2
    }

    pub fn from_string(name: &str) -> Option<MCVersion> {
        ALL.iter().copied().find(|v| v.name() == name)
    }

    pub fn latest() -> MCVersion {
        ALL[0]
    }

    pub fn oldest() -> MCVersion {
        ALL[ALL.len() - 1]
    }

    /// `compareTo`: negative when this one is newer, as the enum is declared newest first.
    fn compare_to(self, other: MCVersion) -> i32 {
        self as i32 - other as i32
    }

    pub fn is_newer_than(self, v: MCVersion) -> bool {
        self.compare_to(v) < 0
    }

    pub fn is_newer_or_equal_to(self, v: MCVersion) -> bool {
        self.compare_to(v) <= 0
    }

    pub fn is_older_than(self, v: MCVersion) -> bool {
        self.compare_to(v) > 0
    }

    pub fn is_older_or_equal_to(self, v: MCVersion) -> bool {
        self.compare_to(v) >= 0
    }

    pub fn is_equal_to(self, v: MCVersion) -> bool {
        self == v
    }

    /// `isBetween(a, b)`: `a` is the older end, `b` the newer, both included (the Java compares ordinals, newest first).
    pub fn is_between(self, a: MCVersion, b: MCVersion) -> bool {
        self.compare_to(a) <= 0 && self.compare_to(b) >= 0
    }

    pub fn is_between_exclusive(self, a: MCVersion, b: MCVersion) -> bool {
        self.compare_to(a) < 0 && self.compare_to(b) > 0
    }

    pub fn newer(self) -> Option<MCVersion> {
        (self as usize).checked_sub(1).map(|i| ALL[i])
    }

    pub fn older(self) -> Option<MCVersion> {
        ALL.get(self as usize + 1).copied()
    }

    pub fn is_release(self) -> bool {
        self.is_older_or_equal_to(MCVersion::v1_0)
    }

    pub fn is_alpha(self) -> bool {
        self.is_between(MCVersion::va1_0_4, MCVersion::va1_2_6)
    }

    pub fn is_beta(self) -> bool {
        self.is_between(MCVersion::vb1_0, MCVersion::vb1_8_1)
    }
}

impl std::fmt::Display for MCVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// `VersionMap`: values keyed by the version they begin at, kept oldest first.
#[derive(Clone, Debug)]
pub struct VersionMap<V> {
    entries: Vec<(MCVersion, V)>,
}

impl<V> Default for VersionMap<V> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<V> VersionMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `add` / `put`: replaces the value of a version that is already there.
    pub fn add(mut self, key: MCVersion, value: V) -> Self {
        self.put(key, value);
        self
    }

    pub fn put(&mut self, key: MCVersion, value: V) -> Option<V> {
        if let Some(e) = self.entries.iter_mut().find(|e| e.0 == key) {
            return Some(std::mem::replace(&mut e.1, value));
        }
        self.entries.push((key, value));
        // Oldest first: an older version has the greater ordinal.
        self.entries.sort_by(|a, b| b.0.ordinal().cmp(&a.0.ordinal()));
        None
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(MCVersion, V)> {
        self.entries.iter()
    }

    /// `getAsOf`: the value of the newest entry that is not newer than `version`.
    pub fn get_as_of(&self, version: MCVersion) -> Option<&V> {
        let mut value = None;
        for (k, v) in &self.entries {
            if k.is_newer_than(version) {
                break;
            }
            value = Some(v);
        }
        value
    }

    pub fn get_latest(&self) -> Option<&V> {
        self.entries.last().map(|e| &e.1)
    }

    pub fn get_latest_version(&self) -> Option<MCVersion> {
        self.entries.last().map(|e| e.0)
    }

    pub fn get_oldest(&self) -> Option<&V> {
        self.entries.first().map(|e| &e.1)
    }

    pub fn get_oldest_version(&self) -> Option<MCVersion> {
        self.entries.first().map(|e| e.0)
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> VersionMap<std::collections::HashMap<K, V>> {
    /// `getMapUntil`: the maps of every entry up to `version`, the newer entry's keys over the older's.
    pub fn get_map_until(&self, version: MCVersion) -> Option<std::collections::HashMap<K, V>> {
        let mut value: Option<std::collections::HashMap<K, V>> = None;
        // Newest first, as the Java sorts them, and the first one it meets starts the map.
        let mut entries: Vec<&(MCVersion, std::collections::HashMap<K, V>)> = self.entries.iter().collect();
        entries.sort_by(|a, b| a.0.ordinal().cmp(&b.0.ordinal()).reverse());
        for e in entries {
            if e.0.is_newer_than(version) {
                continue;
            }
            match &mut value {
                None => value = Some(e.1.clone()),
                Some(map) => {
                    for (k, v) in &e.1 {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        value
    }
}
