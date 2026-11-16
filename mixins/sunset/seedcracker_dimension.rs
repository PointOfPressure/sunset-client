// com.seedfinding.mccore.state.Dimension.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

impl Dimension {
    pub const ALL: [Dimension; 3] = [Dimension::Overworld, Dimension::Nether, Dimension::End];

    pub fn name(self) -> &'static str {
        match self {
            Dimension::Overworld => "overworld",
            Dimension::Nether => "the_nether",
            Dimension::End => "the_end",
        }
    }

    pub fn id(self) -> i32 {
        match self {
            Dimension::Overworld => 0,
            Dimension::Nether => -1,
            Dimension::End => 1,
        }
    }

    pub fn from_string(name: &str) -> Option<Dimension> {
        Self::ALL.into_iter().find(|d| d.name() == name)
    }

    pub fn from_id(id: i32) -> Option<Dimension> {
        Self::ALL.into_iter().find(|d| d.id() == id)
    }
}
