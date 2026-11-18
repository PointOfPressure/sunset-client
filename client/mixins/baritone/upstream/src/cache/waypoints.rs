// baritone.api.cache.IWaypoint and IWaypointCollection: places worth remembering, by tag. Stored as
// text, one waypoint a line, so a save is a single small file.
use super::baritone_utils::Pos;

/// baritone.api.cache.IWaypoint.Tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Bed,
    Home,
    Death,
    User,
}

impl Tag {
    pub const ALL: [Tag; 4] = [Tag::Bed, Tag::Home, Tag::Death, Tag::User];

    pub fn names(self) -> &'static [&'static str] {
        match self {
            Tag::Bed => &["bed", "b"],
            Tag::Home => &["home", "h"],
            Tag::Death => &["death", "d"],
            Tag::User => &["user", "u"],
        }
    }

    /// The name it is printed and saved by.
    pub fn name(self) -> &'static str {
        self.names()[0]
    }

    pub fn by_name(word: &str) -> Option<Tag> {
        Tag::ALL.into_iter().find(|t| t.names().iter().any(|n| n.eq_ignore_ascii_case(word)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waypoint {
    pub tag: Tag,
    pub name: String,
    pub pos: Pos,
    pub dimension: String,
    /// Milliseconds since 1970: what picks one waypoint out of several.
    pub created: u64,
}

#[derive(Default, Debug)]
pub struct Waypoints {
    pub all: Vec<Waypoint>,
}

impl Waypoints {
    pub fn add(&mut self, wp: Waypoint) {
        self.all.push(wp);
    }

    pub fn remove(&mut self, wp: &Waypoint) {
        self.all.retain(|w| w != wp);
    }

    pub fn in_dimension(&self, dimension: &str) -> Vec<&Waypoint> {
        self.all.iter().filter(|w| w.dimension == dimension).collect()
    }

    pub fn by_tag(&self, tag: Tag, dimension: &str) -> Vec<&Waypoint> {
        self.all.iter().filter(|w| w.tag == tag && w.dimension == dimension).collect()
    }

    /// ForWaypoints: a tag name picks every waypoint with that tag, anything else is a waypoint's name.
    pub fn lookup(&self, word: &str, dimension: &str) -> Vec<&Waypoint> {
        match Tag::by_name(word) {
            Some(tag) => self.by_tag(tag, dimension),
            None => self.all.iter().filter(|w| w.dimension == dimension && w.name.eq_ignore_ascii_case(word)).collect(),
        }
    }

    pub fn to_text(&self) -> String {
        self.all
            .iter()
            .map(|w| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}", w.tag.name(), w.name, w.pos[0], w.pos[1], w.pos[2], w.dimension, w.created))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn from_text(text: &str) -> Self {
        let all = text
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() < 7 {
                    return None;
                }
                Some(Waypoint {
                    tag: Tag::by_name(f[0])?,
                    name: f[1].to_owned(),
                    pos: [f[2].parse().ok()?, f[3].parse().ok()?, f[4].parse().ok()?],
                    dimension: f[5].to_owned(),
                    created: f[6].parse().ok()?,
                })
            })
            .collect();
        Self { all }
    }
}
