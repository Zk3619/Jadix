/// Which fields are restored and saved.
///
/// Matches the flag set of tauri-plugin-window-state; default is [`Self::all`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateFlags(u32);

impl StateFlags {
    pub const SIZE: Self = Self(1 << 0);
    pub const POSITION: Self = Self(1 << 1);
    pub const MAXIMIZED: Self = Self(1 << 2);
    pub const VISIBLE: Self = Self(1 << 3);
    pub const DECORATIONS: Self = Self(1 << 4);
    pub const FULLSCREEN: Self = Self(1 << 5);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self(
            Self::SIZE.0
                | Self::POSITION.0
                | Self::MAXIMIZED.0
                | Self::VISIBLE.0
                | Self::DECORATIONS.0
                | Self::FULLSCREEN.0,
        )
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl Default for StateFlags {
    fn default() -> Self {
        Self::all()
    }
}

impl std::ops::BitOr for StateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for StateFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl std::ops::BitAnd for StateFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

#[cfg(test)]
#[path = "tests/flags.rs"]
mod tests;
